//! macOS C FFI interface for InputMethodKit integration
//!
//! This module provides C-compatible functions that can be called from
//! the Swift InputMethodKit app. It mirrors the structure of the `ffi/` module
//! but uses macOS key codes and character-based caret positions.

use std::ffi::CString;
use std::sync::Once;

mod input;
mod lifecycle;
mod query;

#[cfg(test)]
mod tests;

use crate::config::Settings;
use crate::core::engine::{EngineAction, EngineConfig, InputMethodEngine};

static INIT_LOGGING: Once = Once::new();

fn init_logging() {
    INIT_LOGGING.call_once(|| {
        // macOS IME processes don't have stderr visible, so log to a file
        let log_path = format!(
            "{}/Library/Logs/Karukan-rust.log",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        );
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path);

        match log_file {
            Ok(file) => {
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
                    )
                    .with_writer(std::sync::Mutex::new(file))
                    .with_ansi(false)
                    .try_init();
            }
            Err(_) => {
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
                    )
                    .with_writer(std::io::stderr)
                    .try_init();
            }
        }
    });
}

/// Null-check + deref for `*const` FFI pointers. Returns `$default` if null.
macro_rules! ffi_ref {
    ($ptr:expr, $default:expr) => {{
        if $ptr.is_null() {
            return $default;
        }
        unsafe { &*$ptr }
    }};
}

/// Null-check + deref for `*mut` FFI pointers. Returns `$default` if null.
/// Use without default for void functions.
macro_rules! ffi_mut {
    ($ptr:expr) => {{
        if $ptr.is_null() {
            return;
        }
        unsafe { &mut *$ptr }
    }};
    ($ptr:expr, $default:expr) => {{
        if $ptr.is_null() {
            return $default;
        }
        unsafe { &mut *$ptr }
    }};
}

pub(crate) use ffi_mut;
pub(crate) use ffi_ref;

/// Cached preedit text and caret position for FFI consumption.
#[derive(Default)]
struct PreeditCache {
    text: CString,
    /// Caret position in **characters** (not bytes) — macOS setMarkedText uses character offsets.
    caret_chars: u32,
    dirty: bool,
}

/// Cached candidate list for FFI consumption.
#[derive(Default)]
struct CandidateCache {
    texts: Vec<CString>,
    annotations: Vec<CString>,
    count: usize,
    cursor: usize,
    dirty: bool,
    hide: bool,
}

/// Cached commit text for FFI consumption.
#[derive(Default)]
struct CommitCache {
    text: CString,
    dirty: bool,
}

/// Cached aux text for FFI consumption.
#[derive(Default)]
struct AuxCache {
    text: CString,
    dirty: bool,
}

/// Opaque handle to an IME engine instance for macOS.
pub struct KarukanMacEngine {
    engine: InputMethodEngine,
    settings: Settings,
    preedit: PreeditCache,
    candidates: CandidateCache,
    commit: CommitCache,
    aux: AuxCache,
    last_conversion_ms: u64,
    last_process_key_ms: u64,
}

impl KarukanMacEngine {
    fn new() -> Self {
        let settings = Settings::load().unwrap_or_default();

        let config = EngineConfig {
            num_candidates: settings.conversion.num_candidates,
            display_context_len: 10,
            max_api_context_len: if settings.conversion.use_context {
                settings.conversion.max_context_length
            } else {
                0
            },
            short_input_threshold: settings.conversion.short_input_threshold,
            beam_width: settings.conversion.beam_width,
            max_latency_ms: settings.conversion.max_latency_ms,
            strategy: settings.conversion.strategy,
        };
        let engine = InputMethodEngine::with_config(config);
        Self {
            engine,
            settings,
            preedit: PreeditCache::default(),
            candidates: CandidateCache::default(),
            commit: CommitCache::default(),
            aux: AuxCache::default(),
            last_conversion_ms: 0,
            last_process_key_ms: 0,
        }
    }

    fn clear_flags(&mut self) {
        self.preedit.dirty = false;
        self.candidates.dirty = false;
        self.candidates.hide = false;
        self.commit.dirty = false;
        self.aux.dirty = false;
    }

    fn sync_timing(&mut self) {
        self.last_conversion_ms = self.engine.last_conversion_ms();
        self.last_process_key_ms = self.engine.last_process_key_ms();
    }

    fn apply_actions(&mut self, actions: Vec<EngineAction>) {
        for action in actions {
            match action {
                EngineAction::UpdatePreedit(preedit) => {
                    // macOS uses character offsets for setMarkedText
                    self.preedit.caret_chars = preedit.caret() as u32;
                    self.preedit.text = CString::new(preedit.text()).unwrap_or_default();
                    self.preedit.dirty = true;
                }
                EngineAction::ShowCandidates(candidates) => {
                    let page = candidates.page_candidates();
                    self.candidates.texts = page
                        .iter()
                        .filter_map(|c| CString::new(c.text.as_str()).ok())
                        .collect();
                    self.candidates.annotations = page
                        .iter()
                        .map(|c| {
                            let ann = c.annotation.as_deref().unwrap_or("");
                            CString::new(ann).unwrap_or_default()
                        })
                        .collect();
                    self.candidates.count = self.candidates.texts.len();
                    self.candidates.cursor = candidates.page_cursor();
                    self.candidates.dirty = true;
                    self.candidates.hide = false;
                }
                EngineAction::HideCandidates => {
                    self.candidates.hide = true;
                    self.candidates.dirty = true;
                }
                EngineAction::Commit(text) => {
                    self.commit.text = CString::new(text).unwrap_or_default();
                    self.commit.dirty = true;
                }
                EngineAction::UpdateAuxText(text) => {
                    self.aux.text = CString::new(text).unwrap_or_default();
                    self.aux.dirty = true;
                }
                EngineAction::HideAuxText => {
                    self.aux.text = CString::default();
                    self.aux.dirty = true;
                }
            }
        }
    }
}
