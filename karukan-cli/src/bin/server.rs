use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use karukan_engine::RomajiConverter;
use karukan_engine::kana::hiragana_to_katakana;
use karukan_engine::kanji::{LlamaCppModel, LlamaToken, build_jinen_prompt, clean_model_output};
use karukan_im::config::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Karukan kana-kanji conversion server
#[derive(Parser, Debug)]
#[command(name = "karukan-server")]
#[command(about = "Japanese kana-kanji conversion server", long_about = None)]
struct Args {
    /// Enable verbose logging (debug level)
    #[arg(short, long)]
    verbose: bool,

    /// Enable debug mode (exposes /api/tokenize endpoint)
    #[arg(long)]
    debug: bool,

    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

#[derive(Clone)]
struct LlamaCppModelInfo {
    model: Arc<LlamaCppModel>,
    display_name: String,
}

#[derive(Clone)]
struct AppState {
    converter: Arc<RomajiConverter>,
    /// Accumulated raw input for incremental conversion
    romaji_input: Arc<RwLock<String>>,
    /// llama.cpp models keyed by `[models]` key (e.g. "jinen-v2-small-q5")
    llamacpp_models: Arc<RwLock<HashMap<String, LlamaCppModelInfo>>>,
    /// The config's `[conversion] model` key, preferred as the default
    default_model: Arc<String>,
    /// Debug mode enabled (--debug flag)
    debug_mode: bool,
}

#[derive(Debug, Deserialize)]
struct ConvertRequest {
    input: String,
    #[serde(default)]
    incremental: bool,
}

#[derive(Debug, Serialize)]
struct ConvertResponse {
    output: String,
    buffer: String,
}

#[derive(Debug, Deserialize)]
struct KanjiConvertRequest {
    hiragana: String,
    #[serde(default)]
    context: String,
    #[serde(default = "default_num_candidates")]
    num_candidates: usize,
    /// Model to use (optional, uses default if not specified)
    #[serde(default)]
    model: Option<String>,
    /// Beam search algorithm: "true" (default) or "d1_greedy"
    /// - "true": Full beam search with cumulative probability tracking
    /// - "d1_greedy": Depth-1 beam selection followed by greedy decoding (faster)
    #[serde(default)]
    beam_search_type: Option<String>,
}

fn default_num_candidates() -> usize {
    1
}

#[derive(Debug, Clone, Serialize)]
struct TokenVisualization {
    /// Token ID
    id: i32,
    /// Token text (decoded)
    text: String,
    /// Token type: "input" or "output"
    token_type: String,
}

#[derive(Debug, Serialize)]
struct CandidateVisualization {
    /// Candidate index (0-based)
    index: usize,
    /// Decoded text
    text: String,
    /// Cumulative log probability score
    score: f32,
    /// All tokens for this candidate (input + output)
    tokens: Vec<TokenVisualization>,
    /// Number of input tokens
    input_token_count: usize,
    /// Number of output tokens
    output_token_count: usize,
}

#[derive(Debug, Serialize)]
struct KanjiConvertResponse {
    candidates: Vec<String>,
    katakana: String,
    inference_time_ms: f64,
    top_k: Option<usize>,
    model: String,
    /// Number of input tokens (prompt)
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<usize>,
    /// Number of output tokens (generated)
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<usize>,
    /// Token visualization data (input + output tokens) - first candidate only (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    tokens: Option<Vec<TokenVisualization>>,
    /// Detailed visualization for all candidates with scores
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_tokens: Option<Vec<CandidateVisualization>>,
    /// Beam search algorithm used: "true", "d1_greedy", or "greedy" (single candidate)
    #[serde(skip_serializing_if = "Option::is_none")]
    beam_search_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ModelInfo {
    id: String,
    name: String,
    model_id: String,
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
    default: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize tracing
    // Default: info level, with --verbose: debug level
    let default_filter = if args.verbose {
        "karukan_server=debug,karukan_engine=debug,tower_http=debug"
    } else {
        "karukan_server=info,karukan_engine=info,tower_http=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load every model defined in the config's [models] table.
    let settings = Settings::load().expect("failed to load config.toml");
    let mut llamacpp_models = HashMap::new();

    for model_key in settings.models.keys() {
        let source = match settings.model_source(model_key) {
            Ok(source) => source,
            Err(e) => {
                tracing::warn!("Skipping model '{}': {:#}", model_key, e);
                continue;
            }
        };
        tracing::info!("Resolving llama.cpp model '{}'...", model_key);
        match source.resolve() {
            Ok((path, tok_path)) => {
                tracing::info!(
                    "Loading llama.cpp model '{}' from {} (tokenizer: {})...",
                    model_key,
                    path.display(),
                    tok_path.display()
                );
                match LlamaCppModel::from_file(&path, &tok_path) {
                    Ok(model) => {
                        tracing::info!("llama.cpp model '{}' loaded successfully", model_key);
                        llamacpp_models.insert(
                            model_key.clone(),
                            LlamaCppModelInfo {
                                model: Arc::new(model),
                                display_name: source.display_name(),
                            },
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load llama.cpp model '{}': {}", model_key, e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve llama.cpp model '{}': {}. Set HF_TOKEN for private repos.",
                    model_key,
                    e
                );
            }
        }
    }

    if llamacpp_models.is_empty() {
        tracing::info!("No llama.cpp models loaded");
    } else {
        tracing::info!("Loaded {} llama.cpp model(s)", llamacpp_models.len());
    }

    if args.debug {
        tracing::info!("Debug mode enabled - tokenization API available at /api/tokenize");
    }

    let state = AppState {
        converter: Arc::new(RomajiConverter::new()),
        romaji_input: Arc::new(RwLock::new(String::new())),
        llamacpp_models: Arc::new(RwLock::new(llamacpp_models)),
        default_model: Arc::new(settings.conversion.model.clone()),
        debug_mode: args.debug,
    };

    // Setup CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let mut app = Router::new()
        .route("/api/convert", post(convert_handler))
        .route("/api/reset", post(reset_handler))
        .route("/api/kanji/convert", post(kanji_convert_handler))
        .route("/api/models", get(models_handler))
        .route("/health", get(health_handler));

    // Add debug-only routes
    if args.debug {
        app = app.route("/api/tokenize", post(tokenize_handler));
    }

    let app = app
        .fallback_service(ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(256 * 1024)) // 256 KB
        .layer(cors)
        .with_state(state);

    // Start server
    let bind_addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind server address");

    tracing::info!("Server listening on http://{}", bind_addr);

    axum::serve(listener, app)
        .await
        .expect("failed to run server");
}

async fn convert_handler(
    State(state): State<AppState>,
    Json(req): Json<ConvertRequest>,
) -> Result<Json<ConvertResponse>, StatusCode> {
    let mut raw = state.romaji_input.write().expect("lock poisoned");

    if !req.incremental {
        // Convert from scratch
        raw.clear();
    }
    raw.push_str(&req.input);

    let converted = state.converter.convert(&raw);
    let response = ConvertResponse {
        output: converted.text,
        buffer: converted.pending,
    };

    Ok(Json(response))
}

async fn reset_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.romaji_input.write().expect("lock poisoned").clear();
    StatusCode::OK
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "karukan-engine"
    }))
}

/// Resolve the default model id from the loaded models.
///
/// Prefers the config's `[conversion] model` if loaded, otherwise falls
/// back to any loaded model.
fn resolve_default_model_id(
    models: &HashMap<String, LlamaCppModelInfo>,
    default_id: &str,
) -> String {
    if models.contains_key(default_id) {
        default_id.to_string()
    } else {
        models.keys().next().cloned().unwrap_or_default()
    }
}

async fn models_handler(State(state): State<AppState>) -> impl IntoResponse {
    let llamacpp_models = state.llamacpp_models.read().expect("lock poisoned");

    let mut models: Vec<ModelInfo> = llamacpp_models
        .iter()
        .map(|(model_id, info)| ModelInfo {
            id: model_id.clone(),
            name: info.display_name.clone(),
            model_id: model_id.clone(),
        })
        .collect();

    // Sort models by name
    models.sort_by(|a, b| a.name.cmp(&b.name));

    let default_model = resolve_default_model_id(&llamacpp_models, &state.default_model);

    Json(ModelsResponse {
        models,
        default: default_model,
    })
}

async fn kanji_convert_handler(
    State(state): State<AppState>,
    Json(req): Json<KanjiConvertRequest>,
) -> Result<Json<KanjiConvertResponse>, (StatusCode, String)> {
    let katakana = hiragana_to_katakana(&req.hiragana);

    // Determine which model to use
    let model_id = if let Some(ref model_str) = req.model {
        model_str.clone()
    } else {
        let llamacpp_models = state.llamacpp_models.read().expect("lock poisoned");
        let default_id = resolve_default_model_id(&llamacpp_models, &state.default_model);
        if default_id.is_empty() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "No models loaded".to_string(),
            ));
        }
        default_id
    };

    llamacpp_convert(&state, &req, &katakana, &model_id).await
}

/// Tokenize request (debug mode only)
#[derive(Debug, Deserialize)]
struct TokenizeRequest {
    text: String,
    /// Model to use for tokenization (must be a llama.cpp model)
    #[serde(default)]
    model: Option<String>,
}

/// Token information with probability
#[derive(Debug, Serialize)]
struct TokenInfo {
    id: i32,
    text: String,
}

/// Tokenize response
#[derive(Debug, Serialize)]
struct TokenizeResponse {
    tokens: Vec<TokenInfo>,
    prompt: String,
    model: String,
}

/// Handle tokenization request (debug mode only)
async fn tokenize_handler(
    State(state): State<AppState>,
    Json(req): Json<TokenizeRequest>,
) -> Result<Json<TokenizeResponse>, (StatusCode, String)> {
    if !state.debug_mode {
        return Err((
            StatusCode::FORBIDDEN,
            "Tokenize API is only available in debug mode".to_string(),
        ));
    }

    // Get the first available llama.cpp model or the specified one
    let models_guard = state.llamacpp_models.read().expect("lock poisoned");

    let (model_id, model_info) = if let Some(ref model_str) = req.model {
        let info = models_guard.get(model_str).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("llama.cpp model '{}' not loaded", model_str),
            )
        })?;
        (model_str.clone(), info)
    } else {
        // Use the first available model
        let (id, info) = models_guard.iter().next().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "No llama.cpp models loaded".to_string(),
            )
        })?;
        (id.clone(), info)
    };

    let model = &model_info.model;
    let display_name = model_info.display_name.clone();

    // Tokenize the input
    let tokens = model.tokenize(&req.text).map_err(|e| {
        tracing::error!("Tokenize error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tokenize error".to_string(),
        )
    })?;

    // Convert tokens to response format
    let token_infos: Vec<TokenInfo> = tokens
        .iter()
        .map(|&token| {
            let text = model
                .decode(&[token], false)
                .unwrap_or_else(|_| "<??>".to_string());
            TokenInfo { id: token.0, text }
        })
        .collect();

    Ok(Json(TokenizeResponse {
        tokens: token_infos,
        prompt: req.text,
        model: format!("{} ({})", display_name, model_id),
    }))
}

/// Handle kanji conversion using llama.cpp backend
async fn llamacpp_convert(
    state: &AppState,
    req: &KanjiConvertRequest,
    katakana: &str,
    model_id: &str,
) -> Result<Json<KanjiConvertResponse>, (StatusCode, String)> {
    // Get the llama.cpp model
    let models_guard = state.llamacpp_models.read().expect("lock poisoned");
    let model_info = models_guard.get(model_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("llama.cpp model '{}' not loaded", model_id),
        )
    })?;
    let model = &model_info.model;
    let display_name = model_info.display_name.clone();

    // Helper: build token visualization from input and output token slices
    let build_token_viz = |input_toks: &[LlamaToken], output_toks: &[LlamaToken]| {
        let mut tokens: Vec<TokenVisualization> = input_toks
            .iter()
            .map(|token| TokenVisualization {
                id: token.0,
                text: model.decode_token_for_display(*token),
                token_type: "input".to_string(),
            })
            .collect();
        tokens.extend(output_toks.iter().map(|token| TokenVisualization {
            id: token.0,
            text: model.decode_token_for_display(*token),
            token_type: "output".to_string(),
        }));
        tokens
    };

    // Build prompt in jinen format
    // Note: NFKC normalization is handled by the tokenizer's normalizer (tokenizer.json).
    let prompt = build_jinen_prompt(katakana, &req.context);
    tracing::debug!(
        "llama.cpp prompt: katakana='{}', context='{}', prompt_len={}",
        katakana,
        req.context,
        prompt.len()
    );

    let start = std::time::Instant::now();

    // Tokenize and generate
    let input_tokens = model.tokenize(&prompt).map_err(|e| {
        tracing::error!("llama.cpp tokenize error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Tokenize error: {}", e),
        )
    })?;

    let input_token_count = input_tokens.len();

    // Get EOS token ID from model
    let eos_token = model.eos_token_id();
    let eos_token_id = Some(eos_token.0);
    tracing::debug!("llama.cpp EOS token ID: {:?}", eos_token_id);

    let beam_size = req.num_candidates.clamp(1, 20);

    // Track generated tokens for visualization
    let mut first_generated_tokens = Vec::new();

    // Use greedy decoding for single candidate, beam search for multiple
    let (candidates, candidate_viz): (Vec<String>, Option<Vec<CandidateVisualization>>) =
        if beam_size == 1 {
            // Fast path: greedy decoding
            let output_tokens = model
                .generate(&input_tokens, 64, eos_token_id)
                .map_err(|e| {
                    tracing::error!("llama.cpp generate error: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Generate error: {}", e),
                    )
                })?;

            let generated_tokens = &output_tokens[input_tokens.len()..];
            first_generated_tokens = generated_tokens.to_vec();

            let output = model.decode(generated_tokens, true).map_err(|e| {
                tracing::error!("llama.cpp decode error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Decode error: {}", e),
                )
            })?;

            let output = clean_model_output(&output);

            if output.is_empty() {
                // Greedy produced empty output; fall back to beam search
                // to find non-empty candidates (some models emit EOS as the
                // top greedy choice but have valid alternatives).
                tracing::debug!("Greedy produced empty output, falling back to beam search");
                let beam_results = model
                    .generate_beam_search(&input_tokens, 64, eos_token_id, 3)
                    .map_err(|e| {
                        tracing::error!("llama.cpp beam search fallback error: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Generate error: {}", e),
                        )
                    })?;

                let mut fallback_output = None;
                for (gen_tokens, _score) in &beam_results {
                    let text = model.decode(gen_tokens, true).map_err(|e| {
                        tracing::error!("llama.cpp decode error: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Decode error: {}", e),
                        )
                    })?;
                    let clean = clean_model_output(&text);
                    if !clean.is_empty() {
                        first_generated_tokens = gen_tokens.clone();
                        fallback_output = Some(clean);
                        break;
                    }
                }

                if let Some(output) = fallback_output {
                    (vec![output], None)
                } else {
                    // Even beam search found nothing; return original reading
                    (vec![req.hiragana.clone()], None)
                }
            } else {
                // Build candidate visualization for single greedy result (input + output)
                let input_count = input_tokens.len();
                let output_count = generated_tokens.len();
                let tokens = build_token_viz(&input_tokens, generated_tokens);

                let viz = vec![CandidateVisualization {
                    index: 0,
                    text: output.clone(),
                    score: 0.0, // Greedy doesn't track score
                    tokens,
                    input_token_count: input_count,
                    output_token_count: output_count,
                }];

                (vec![output], Some(viz))
            }
        } else {
            // Beam search for multiple candidates (deterministic, probability-ordered)
            // Choose algorithm based on beam_search_type parameter
            let use_d1_greedy = req
                .beam_search_type
                .as_ref()
                .is_some_and(|t| t == "d1_greedy");

            let beam_results = if use_d1_greedy {
                model.generate_beam_search_d1_greedy(&input_tokens, 64, eos_token_id, beam_size)
            } else {
                model.generate_beam_search(&input_tokens, 64, eos_token_id, beam_size)
            }
            .map_err(|e| {
                tracing::error!("llama.cpp beam search error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Generate error: {}", e),
                )
            })?;

            // Decode and deduplicate candidates, build candidate visualizations
            let mut seen = std::collections::HashSet::new();
            let mut candidates = Vec::new();
            let mut candidate_viz: Vec<CandidateVisualization> = Vec::new();

            for (i, (generated_tokens, score)) in beam_results.into_iter().enumerate() {
                // Keep first candidate's tokens for legacy visualization
                if i == 0 {
                    first_generated_tokens = generated_tokens.clone();
                }

                let output = model.decode(&generated_tokens, true).map_err(|e| {
                    tracing::error!("llama.cpp decode error: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Decode error: {}", e),
                    )
                })?;

                let output = clean_model_output(&output);

                if !output.is_empty() && seen.insert(output.clone()) {
                    // Build token visualization for this candidate (input + output)
                    let tokens = build_token_viz(&input_tokens, &generated_tokens);
                    let input_count = input_tokens.len();
                    let output_count = generated_tokens.len();

                    candidate_viz.push(CandidateVisualization {
                        index: candidates.len(),
                        text: output.clone(),
                        score,
                        tokens,
                        input_token_count: input_count,
                        output_token_count: output_count,
                    });

                    candidates.push(output);
                }
            }

            (candidates, Some(candidate_viz))
        };

    let inference_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Build legacy token visualization (input + first candidate output)
    let token_viz = build_token_viz(&input_tokens, &first_generated_tokens);

    let output_token_count = first_generated_tokens.len();
    let model_info = display_name;

    // Determine which beam search type was used
    let beam_search_type_used = if beam_size == 1 {
        "greedy"
    } else if req
        .beam_search_type
        .as_ref()
        .is_some_and(|t| t == "d1_greedy")
    {
        "d1_greedy"
    } else {
        "true"
    };

    Ok(Json(KanjiConvertResponse {
        candidates,
        katakana: katakana.to_string(),
        inference_time_ms,
        top_k: None,
        model: model_info,
        input_tokens: Some(input_token_count),
        output_tokens: Some(output_token_count),
        tokens: Some(token_viz),
        candidate_tokens: candidate_viz,
        beam_search_type: Some(beam_search_type_used.to_string()),
    }))
}
