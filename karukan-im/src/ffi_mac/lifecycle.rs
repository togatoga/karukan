#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_int;

use crate::config::settings::StrategyMode;
use crate::core::engine::resolve_variant_id;

use super::{KarukanMacEngine, ffi_mut, init_logging};

/// Create a new Karukan macOS engine instance.
/// Returns a pointer to the engine, or null on failure.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_new() -> *mut KarukanMacEngine {
    init_logging();
    let engine = Box::new(KarukanMacEngine::new());
    Box::into_raw(engine)
}

/// Initialize the kanji converter (loads the model).
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_init(engine: *mut KarukanMacEngine) -> c_int {
    let engine = ffi_mut!(engine, -1);
    let settings = &engine.settings;

    let strategy = settings.conversion.strategy;
    tracing::info!(
        "Karukan macOS init: model={:?}, light_model={:?}, strategy={:?}",
        settings.conversion.model,
        settings.conversion.light_model,
        strategy,
    );

    engine
        .engine
        .init_system_dictionary(settings.conversion.dict_path.as_deref());

    engine.engine.init_user_dictionaries();

    engine
        .engine
        .init_learning_cache(settings.learning.enabled, settings.learning.max_entries);

    let n_threads = settings.conversion.n_threads;

    match strategy {
        StrategyMode::Light => {
            let light_variant = match resolve_variant_id(settings.conversion.light_model.as_deref())
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("Invalid light_model settings: {}", e);
                    return -1;
                }
            };
            if let Err(e) = engine
                .engine
                .init_kanji_converter_with_model(&light_variant, n_threads)
            {
                tracing::error!("Failed to initialize light model: {}", e);
                return -1;
            }
        }
        StrategyMode::Main => {
            let main_variant = match resolve_variant_id(settings.conversion.model.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("Invalid model settings: {}", e);
                    return -1;
                }
            };
            if let Err(e) = engine
                .engine
                .init_kanji_converter_with_model(&main_variant, n_threads)
            {
                tracing::error!("Failed to initialize main model: {}", e);
                return -1;
            }
        }
        StrategyMode::Adaptive => {
            let main_variant = match resolve_variant_id(settings.conversion.model.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("Invalid model settings: {}", e);
                    return -1;
                }
            };
            let light_model = settings.conversion.light_model.clone();
            if let Err(e) = engine
                .engine
                .init_kanji_converter_with_model(&main_variant, n_threads)
            {
                tracing::error!("Failed to initialize default model: {}", e);
                return -1;
            }

            let light_variant = match resolve_variant_id(light_model.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("Invalid light_model settings, using default: {}", e);
                    karukan_engine::kanji::registry().default_model.clone()
                }
            };
            if let Err(e) = engine
                .engine
                .init_light_kanji_converter(&light_variant, n_threads)
            {
                tracing::warn!(
                    "Failed to initialize beam model (light_model={:?}): {}",
                    light_model,
                    e
                );
            }
        }
    }

    tracing::info!(
        "Karukan macOS init complete: {}",
        engine.engine.model_name()
    );

    0
}

/// Destroy a Karukan macOS engine instance.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_free(engine: *mut KarukanMacEngine) {
    if !engine.is_null() {
        let engine_ref = unsafe { &mut *engine };
        engine_ref.engine.save_learning();
        unsafe {
            drop(Box::from_raw(engine));
        }
    }
}
