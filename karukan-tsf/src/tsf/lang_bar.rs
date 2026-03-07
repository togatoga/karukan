//! Language Bar integration — ITfLangBarItemButton implementation.
//!
//! Displays the current input mode (あ/カ/A/×) in the Windows taskbar
//! language indicator area.

use std::cell::RefCell;

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::HICON;
use windows::core::*;

// OLE connection point error codes (not exported in windows 0.58)
const CONNECT_E_NOCONNECTION: HRESULT = HRESULT(0x80040200_u32 as i32);
const CONNECT_E_ADVISELIMIT: HRESULT = HRESULT(0x80040201_u32 as i32);
const CONNECT_E_CANNOTCONNECT: HRESULT = HRESULT(0x80040202_u32 as i32);

use karukan_im::InputMode;

use crate::globals::GUID_LANGBAR_ITEM_BUTTON;

/// Language bar button that shows the current input mode.
#[implement(ITfLangBarItemButton, ITfLangBarItem, ITfSource)]
pub struct KarukanLangBarButton {
    inner: RefCell<LangBarInner>,
}

struct LangBarInner {
    mode: InputMode,
    enabled: bool,
    sink: Option<ITfLangBarItemSink>,
    sink_cookie: u32,
    next_cookie: u32,
    toggle_callback: Option<Box<dyn Fn()>>,
}

#[allow(clippy::new_without_default)]
impl KarukanLangBarButton {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(LangBarInner {
                mode: InputMode::Hiragana,
                enabled: true,
                sink: None,
                sink_cookie: 0,
                next_cookie: 1,
                toggle_callback: None,
            }),
        }
    }

    /// Set a callback to be invoked when the language bar button is clicked.
    pub fn set_toggle_callback(&self, callback: Box<dyn Fn()>) {
        self.inner.borrow_mut().toggle_callback = Some(callback);
    }

    /// Update the displayed mode. Notifies the language bar sink if changed.
    pub fn update_mode(&self, mode: InputMode, enabled: bool) {
        let mut inner = self.inner.borrow_mut();
        if inner.mode == mode && inner.enabled == enabled {
            return;
        }
        inner.mode = mode;
        inner.enabled = enabled;
        let sink = inner.sink.clone();
        drop(inner);

        if let Some(sink) = sink {
            unsafe {
                let _ = sink.OnUpdate(TF_LBI_TEXT | TF_LBI_TOOLTIP);
            }
        }
    }

    fn display_text(mode: InputMode, enabled: bool) -> &'static str {
        if !enabled {
            return "\u{00D7}"; // ×
        }
        match mode {
            InputMode::Hiragana => "\u{3042}",          // あ
            InputMode::Katakana => "\u{30AB}",          // カ
            InputMode::HalfWidthKatakana => "\u{FF76}", // ｶ
            InputMode::Alphabet => "A",
        }
    }

    fn tooltip_text(mode: InputMode, enabled: bool) -> &'static str {
        if !enabled {
            return "\u{7121}\u{52B9}"; // 無効
        }
        match mode {
            InputMode::Hiragana => "\u{3072}\u{3089}\u{304C}\u{306A}", // ひらがな
            InputMode::Katakana => "\u{30AB}\u{30BF}\u{30AB}\u{30CA}", // カタカナ
            InputMode::HalfWidthKatakana => "\u{534A}\u{89D2}\u{30AB}\u{30BF}\u{30AB}\u{30CA}", // 半角カタカナ
            InputMode::Alphabet => "\u{82F1}\u{6570}", // 英数
        }
    }
}

// ITfLangBarItem implementation
#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl ITfLangBarItem_Impl for KarukanLangBarButton_Impl {
    fn GetInfo(&self, pinfo: *mut TF_LANGBARITEMINFO) -> Result<()> {
        unsafe {
            if pinfo.is_null() {
                return Err(E_POINTER.into());
            }
            let info = &mut *pinfo;
            info.clsidService = crate::globals::CLSID_KARUKAN_TEXT_SERVICE;
            info.guidItem = GUID_LANGBAR_ITEM_BUTTON;
            info.dwStyle = TF_LBI_STYLE_BTN_BUTTON | TF_LBI_STYLE_SHOWNINTRAY;
            info.ulSort = 0;

            let desc = "karukan";
            let desc_wide: Vec<u16> = desc.encode_utf16().collect();
            let len = desc_wide.len().min(info.szDescription.len() - 1);
            info.szDescription[..len].copy_from_slice(&desc_wide[..len]);
            info.szDescription[len] = 0;
        }
        Ok(())
    }

    fn GetStatus(&self) -> Result<u32> {
        Ok(0) // enabled
    }

    fn Show(&self, _fshow: BOOL) -> Result<()> {
        Ok(())
    }

    fn GetTooltipString(&self) -> Result<BSTR> {
        let inner = self.inner.borrow();
        let text = KarukanLangBarButton::tooltip_text(inner.mode, inner.enabled);
        Ok(BSTR::from(text))
    }
}

// ITfLangBarItemButton implementation
impl ITfLangBarItemButton_Impl for KarukanLangBarButton_Impl {
    fn OnClick(&self, _click: TfLBIClick, _pt: &POINT, _prcarea: *const RECT) -> Result<()> {
        let inner = self.inner.borrow();
        if let Some(ref callback) = inner.toggle_callback {
            callback();
        }
        Ok(())
    }

    fn InitMenu(&self, _pmenu: Option<&ITfMenu>) -> Result<()> {
        Ok(())
    }

    fn OnMenuSelect(&self, _wid: u32) -> Result<()> {
        Ok(())
    }

    fn GetIcon(&self) -> Result<HICON> {
        Err(E_NOTIMPL.into())
    }

    fn GetText(&self) -> Result<BSTR> {
        let inner = self.inner.borrow();
        let text = KarukanLangBarButton::display_text(inner.mode, inner.enabled);
        Ok(BSTR::from(text))
    }
}

// ITfSource implementation — manages ITfLangBarItemSink
#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl ITfSource_Impl for KarukanLangBarButton_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Option<&windows::core::IUnknown>) -> Result<u32> {
        unsafe {
            if riid.is_null() {
                return Err(E_INVALIDARG.into());
            }
            if *riid != ITfLangBarItemSink::IID {
                return Err(CONNECT_E_CANNOTCONNECT.into());
            }
        }

        let punk = punk.ok_or(E_INVALIDARG)?;
        let sink: ITfLangBarItemSink = punk.cast()?;

        let mut inner = self.inner.borrow_mut();
        if inner.sink.is_some() {
            return Err(CONNECT_E_ADVISELIMIT.into());
        }
        let cookie = inner.next_cookie;
        inner.next_cookie += 1;
        inner.sink = Some(sink);
        inner.sink_cookie = cookie;
        Ok(cookie)
    }

    fn UnadviseSink(&self, dwcookie: u32) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        if inner.sink_cookie == dwcookie {
            inner.sink = None;
            inner.sink_cookie = 0;
            Ok(())
        } else {
            Err(CONNECT_E_NOCONNECTION.into())
        }
    }
}
