//! ITfTextInputProcessorEx implementation — the main TSF entry point.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::candidate::window::CandidateWindow;
use crate::engine_bridge::EngineBridge;
use crate::globals::{
    dll_add_ref, dll_release, CLSID_KARUKAN_TEXT_SERVICE, GUID_DISPLAY_ATTRIBUTE_CONVERTED,
    GUID_DISPLAY_ATTRIBUTE_INPUT, GUID_KARUKAN_PROFILE, GUID_PRESERVED_KEY_ONOFF,
    LANGID_JAPANESE,
};

/// The main karukan text service COM object.
///
/// Implements ITfTextInputProcessorEx (and its base ITfTextInputProcessor),
/// ITfKeyEventSink, ITfCompositionSink, and ITfDisplayAttributeProvider.
#[implement(
    ITfTextInputProcessorEx,
    ITfTextInputProcessor,
    ITfKeyEventSink,
    ITfCompositionSink,
    ITfDisplayAttributeProvider,
    ITfThreadMgrEventSink,
)]
pub struct KarukanTextService {
    pub(crate) inner: RefCell<KarukanTextServiceInner>,
}

pub(crate) struct KarukanTextServiceInner {
    pub(crate) thread_mgr: Option<ITfThreadMgr>,
    pub(crate) client_id: u32,
    pub(crate) engine: EngineBridge,
    pub(crate) composition: Option<ITfComposition>,
    /// Cookie for ITfThreadMgrEventSink
    pub(crate) thread_mgr_sink_cookie: u32,
    /// Cookie for ITfKeyEventSink
    pub(crate) keystroke_mgr_cookie: bool,
    /// Cached EngineResult from OnTestKeyDown for reuse in OnKeyDown
    pub(crate) cached_result: Option<karukan_im::EngineResult>,
    /// Whether the IME is enabled (toggled via PreservedKey)
    pub(crate) enabled: bool,
    /// Candidate window for displaying conversion candidates
    pub(crate) candidate_window: Option<Rc<RefCell<CandidateWindow>>>,
    /// Display attribute atom for input (composing) text
    pub(crate) display_attr_atom_input: u32,
    /// Display attribute atom for converted text
    pub(crate) display_attr_atom_converted: u32,
}

impl KarukanTextService {
    pub fn new() -> Self {
        dll_add_ref();
        Self {
            inner: RefCell::new(KarukanTextServiceInner {
                thread_mgr: None,
                client_id: 0,
                engine: EngineBridge::new(),
                composition: None,
                thread_mgr_sink_cookie: TF_INVALID_COOKIE,
                keystroke_mgr_cookie: false,
                cached_result: None,
                enabled: true,
                candidate_window: None,
                display_attr_atom_input: 0,
                display_attr_atom_converted: 0,
            }),
        }
    }
}

impl Drop for KarukanTextService {
    fn drop(&mut self) {
        dll_release();
    }
}

// ITfTextInputProcessor implementation
impl ITfTextInputProcessor_Impl for KarukanTextService_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        self.ActivateEx(ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        let mut inner = self.inner.borrow_mut();

        // Save learning cache before deactivation
        inner.engine.save_learning();

        // Unadvise key event sink
        if inner.keystroke_mgr_cookie {
            if let Some(ref thread_mgr) = inner.thread_mgr {
                unsafe {
                    let keystroke_mgr: Result<ITfKeystrokeMgr> = thread_mgr.cast();
                    if let Ok(km) = keystroke_mgr {
                        let _ = km.UnadviseKeyEventSink(inner.client_id);
                        // Unpreserve key
                        let pk = TF_PRESERVEDKEY {
                            uVKey: 0x19, // VK_KANJI (Hankaku/Zenkaku)
                            uModifiers: 0,
                        };
                        let _ = km.UnpreserveKey(&GUID_PRESERVED_KEY_ONOFF, &pk);
                    }
                }
            }
            inner.keystroke_mgr_cookie = false;
        }

        // Unadvise thread manager event sink
        if inner.thread_mgr_sink_cookie != TF_INVALID_COOKIE {
            if let Some(ref thread_mgr) = inner.thread_mgr {
                unsafe {
                    let source: Result<ITfSource> = thread_mgr.cast();
                    if let Ok(src) = source {
                        let _ = src.UnadviseSink(inner.thread_mgr_sink_cookie);
                    }
                }
            }
            inner.thread_mgr_sink_cookie = TF_INVALID_COOKIE;
        }

        // Drop active composition reference (TSF handles cleanup)
        inner.composition = None;

        // Destroy candidate window
        if let Some(cw) = inner.candidate_window.take() {
            cw.borrow_mut().destroy();
        }

        inner.engine.reset();
        inner.thread_mgr = None;
        inner.client_id = 0;
        inner.enabled = true;

        Ok(())
    }
}

// ITfTextInputProcessorEx implementation
impl ITfTextInputProcessorEx_Impl for KarukanTextService_Impl {
    fn ActivateEx(
        &self,
        ptim: Option<&ITfThreadMgr>,
        tid: u32,
        _dwflags: u32,
    ) -> Result<()> {
        let mut inner = self.inner.borrow_mut();

        let thread_mgr = ptim.ok_or(E_INVALIDARG)?;
        inner.thread_mgr = Some(thread_mgr.clone());
        inner.client_id = tid;

        // Initialize the engine (loads models, dictionaries, learning cache)
        if let Err(e) = inner.engine.initialize() {
            tracing::error!("Failed to initialize engine: {}", e);
            // Continue anyway — engine works without models (romaji-only mode)
        }

        // Register display attribute atoms via ITfCategoryMgr
        unsafe {
            if let Ok(cat_mgr) = windows::Win32::System::Com::CoCreateInstance::<
                _,
                ITfCategoryMgr,
            >(
                &CLSID_TF_CategoryMgr,
                None,
                windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            ) {
                let mut atom_input = 0u32;
                if cat_mgr
                    .RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_INPUT, &mut atom_input)
                    .is_ok()
                {
                    inner.display_attr_atom_input = atom_input;
                }
                let mut atom_converted = 0u32;
                if cat_mgr
                    .RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_CONVERTED, &mut atom_converted)
                    .is_ok()
                {
                    inner.display_attr_atom_converted = atom_converted;
                }
                tracing::debug!(
                    "Display attribute atoms: input={}, converted={}",
                    inner.display_attr_atom_input,
                    inner.display_attr_atom_converted
                );
            }
        }

        // Create candidate window
        inner.candidate_window = Some(Rc::new(RefCell::new(CandidateWindow::new())));

        // Advise key event sink
        unsafe {
            let keystroke_mgr: ITfKeystrokeMgr = thread_mgr.cast()?;
            let this_sink: ITfKeyEventSink = self.cast()?;
            keystroke_mgr.AdviseKeyEventSink(tid, &this_sink, TRUE)?;

            // Register preserved key: Hankaku/Zenkaku (VK_KANJI = 0x19) for IME toggle
            let pk = TF_PRESERVEDKEY {
                uVKey: 0x19, // VK_KANJI
                uModifiers: 0,
            };
            let desc: Vec<u16> = "karukan IME toggle".encode_utf16().collect();
            let _ = keystroke_mgr.PreserveKey(
                tid,
                &GUID_PRESERVED_KEY_ONOFF,
                &pk,
                &desc,
            );

            inner.keystroke_mgr_cookie = true;
        }

        // Advise thread manager event sink
        unsafe {
            let source: ITfSource = thread_mgr.cast()?;
            let this_sink: ITfThreadMgrEventSink = self.cast()?;
            let cookie = source.AdviseSink(
                &ITfThreadMgrEventSink::IID,
                &this_sink,
            )?;
            inner.thread_mgr_sink_cookie = cookie;
        }

        inner.enabled = true;

        Ok(())
    }
}
