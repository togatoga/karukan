//! TSF (Text Services Framework) COM interface implementations.
//!
//! This module contains the Windows-specific COM implementations for the
//! karukan text service. Each submodule implements one or more TSF interfaces.

#[cfg(target_os = "windows")]
pub mod class_factory;
#[cfg(target_os = "windows")]
pub mod compartment;
#[cfg(target_os = "windows")]
pub mod composition_sink;
#[cfg(target_os = "windows")]
pub mod context_reader;
#[cfg(target_os = "windows")]
pub mod display_attribute;
#[cfg(target_os = "windows")]
pub mod edit_session;
#[cfg(target_os = "windows")]
pub mod key_event_sink;
#[cfg(target_os = "windows")]
pub mod lang_bar;
#[cfg(target_os = "windows")]
pub mod text_input_processor;
#[cfg(target_os = "windows")]
pub mod thread_mgr_sink;
