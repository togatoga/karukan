//! llama.cpp based GGUF inference for kanji conversion
//!
//! This module provides an alternative to Candle's GGUF implementation using
//! llama.cpp's optimized inference engine via the llama-cpp-2 crate.
//!
//! Enable with the `llamacpp` feature flag.

use super::error::KanjiError;
type Result<T> = super::error::Result<T>;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

/// Global llama.cpp backend (can only be initialized once)
static LLAMA_BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();

/// Get or initialize the global llama.cpp backend
fn get_backend() -> Result<&'static LlamaBackend> {
    let result = LLAMA_BACKEND.get_or_init(|| {
        let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        backend.void_logs();
        Ok(backend)
    });
    match result {
        Ok(backend) => Ok(backend),
        Err(e) => Err(KanjiError::ModelLoad(
            format!("Failed to initialize llama.cpp backend: {}", e).into(),
        )),
    }
}

/// Convert bytes to hex display format for partial UTF-8 sequences
fn bytes_to_hex_display(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("<{:02X}>", b)).collect()
}

/// Load and configure an external HuggingFace tokenizer from a `tokenizer.json` file.
fn load_tokenizer<P: AsRef<Path>>(path: P) -> Result<tokenizers::Tokenizer> {
    let mut tokenizer =
        tokenizers::Tokenizer::from_file(path.as_ref()).map_err(KanjiError::TokenizerLoad)?;
    // Disable padding and truncation — we handle sequence length ourselves
    // and padding tokens would corrupt the model input.
    tokenizer.with_padding(None);
    tokenizer.with_truncation(None).ok();
    Ok(tokenizer)
}

/// A beam candidate with generated tokens and cumulative score
#[derive(Clone)]
struct BeamState {
    tokens: Vec<LlamaToken>,
    score: f32,
}

/// llama.cpp based GPT-2 model for GGUF inference
pub struct LlamaCppModel {
    model: LlamaModel,
    n_ctx: u32,
    /// External HuggingFace tokenizer (always required).
    /// `tokenize()` and `decode()` use this instead of llama.cpp's built-in tokenizer.
    external_tokenizer: tokenizers::Tokenizer,
    /// Number of threads for inference (0 = use llama.cpp default)
    n_threads: u32,
}

impl LlamaCppModel {
    /// Load a GGUF model using llama.cpp with an external tokenizer.
    ///
    /// GPT-2 models use CPU only (Metal has issues with GPT-2).
    pub fn from_file<P: AsRef<Path>, T: AsRef<Path>>(path: P, tokenizer_json: T) -> Result<Self> {
        let backend = get_backend()?;

        // GPT-2 has Metal issues, use CPU
        let model_params = LlamaModelParams::default().with_n_gpu_layers(0);

        let model = LlamaModel::load_from_file(backend, path.as_ref(), &model_params)
            .map_err(|e| KanjiError::ModelLoad(e.into()))?;
        let external_tokenizer = load_tokenizer(tokenizer_json)?;

        Ok(Self {
            model,
            n_ctx: 256,
            external_tokenizer,
            n_threads: 0,
        })
    }

    /// Load a GGUF model with a pre-tokenizer type override.
    ///
    /// Some models use custom pre-tokenizer types (e.g., `gpt2-small-japanese-char`)
    /// that llama.cpp doesn't recognize. This method overrides the `tokenizer.ggml.pre`
    /// metadata key to a compatible type before loading.
    pub fn from_file_with_pre_tokenizer_override<P: AsRef<Path>, T: AsRef<Path>>(
        path: P,
        tokenizer_json: T,
        pre_tokenizer: &str,
    ) -> Result<Self> {
        use llama_cpp_2::model::params::kv_overrides::ParamOverrideValue;
        use std::ffi::CString;
        use std::pin::pin;

        let backend = get_backend()?;

        let mut params = pin!(LlamaModelParams::default().with_n_gpu_layers(0));

        let key =
            CString::new("tokenizer.ggml.pre").map_err(|e| KanjiError::ModelLoad(e.into()))?;
        let mut str_value: [std::os::raw::c_char; 128] = [0; 128];
        for (i, &byte) in pre_tokenizer.as_bytes().iter().enumerate() {
            if i >= 127 {
                break;
            }
            str_value[i] = byte as std::os::raw::c_char;
        }
        params
            .as_mut()
            .append_kv_override(&key, ParamOverrideValue::Str(str_value));

        let model = LlamaModel::load_from_file(backend, path.as_ref(), &params)
            .map_err(|e| KanjiError::ModelLoad(e.into()))?;
        let external_tokenizer = load_tokenizer(tokenizer_json)?;

        Ok(Self {
            model,
            n_ctx: 256,
            external_tokenizer,
            n_threads: 0,
        })
    }

    /// Load a GGUF model with explicit context window size
    pub fn from_file_with_n_ctx<P: AsRef<Path>, T: AsRef<Path>>(
        path: P,
        tokenizer_json: T,
        n_ctx: u32,
    ) -> Result<Self> {
        let backend = get_backend()?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(0);

        let model = LlamaModel::load_from_file(backend, path.as_ref(), &model_params)
            .map_err(|e| KanjiError::ModelLoad(e.into()))?;
        let external_tokenizer = load_tokenizer(tokenizer_json)?;

        Ok(Self {
            model,
            n_ctx,
            external_tokenizer,
            n_threads: 0,
        })
    }

    /// Set the number of threads for inference.
    /// 0 means use llama.cpp default (typically all cores).
    pub fn set_n_threads(&mut self, n: u32) {
        self.n_threads = n;
    }

    /// Build LlamaContextParams with configured n_threads
    fn context_params(&self) -> LlamaContextParams {
        let params = LlamaContextParams::default().with_n_ctx(Some(
            NonZeroU32::new(self.n_ctx).expect("n_ctx must be non-zero"),
        ));
        if self.n_threads > 0 {
            params
                .with_n_threads(self.n_threads as i32)
                .with_n_threads_batch(self.n_threads as i32)
        } else {
            params
        }
    }

    /// Tokenize a string using the external tokenizer
    pub fn tokenize(&self, text: &str) -> Result<Vec<LlamaToken>> {
        let encoding = self
            .external_tokenizer
            .encode(text, false)
            .map_err(KanjiError::Inference)?;
        let tokens: Vec<LlamaToken> = encoding
            .get_ids()
            .iter()
            .map(|&id| LlamaToken(id as i32))
            .collect();
        Ok(tokens)
    }

    /// Decode tokens to string using the external tokenizer
    ///
    /// When `skip_special_tokens` is true, special tokens (BOS, EOS, EOG) are
    /// excluded from the output.
    pub fn decode(&self, tokens: &[LlamaToken], skip_special_tokens: bool) -> Result<String> {
        let ids: Vec<u32> = tokens.iter().map(|t| t.0 as u32).collect();
        let text = self
            .external_tokenizer
            .decode(&ids, skip_special_tokens)
            .map_err(KanjiError::Inference)?;
        Ok(text)
    }

    /// Decode a single token for display purposes.
    ///
    /// For byte-level BPE tokens that represent partial UTF-8 sequences,
    /// this returns a hex representation like `<0xE3>` instead of replacement characters.
    pub fn decode_token_for_display(&self, token: LlamaToken) -> String {
        match self.model.token_to_piece_bytes(token, 32, true, None) {
            Ok(bytes) => {
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    // Valid UTF-8, return as-is (escape control chars)
                    if s.chars().all(|c| !c.is_control() || c == ' ' || c == '\n') {
                        s.to_string()
                    } else {
                        // Has control characters, show hex
                        bytes_to_hex_display(&bytes)
                    }
                } else {
                    // Invalid UTF-8 (partial sequence), show hex
                    bytes_to_hex_display(&bytes)
                }
            }
            Err(_) => format!("<{}>", token.0),
        }
    }

    /// Generate tokens with greedy decoding
    pub fn generate(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
    ) -> Result<Vec<LlamaToken>> {
        self.generate_with_sampler(
            input_tokens,
            max_new_tokens,
            eos_token_id,
            LlamaSampler::greedy(),
        )
    }

    /// Generate multiple candidates using true beam search algorithm
    ///
    /// This implements proper beam search that tracks cumulative probabilities
    /// at every step and keeps the globally best beam_size candidates.
    ///
    /// # Arguments
    /// * `input_tokens` - Input token sequence
    /// * `max_new_tokens` - Maximum new tokens to generate per candidate
    /// * `eos_token_id` - Optional EOS token ID to stop generation
    /// * `beam_size` - Number of candidates to keep at each step
    ///
    /// Returns candidates sorted by cumulative probability (highest first)
    pub fn generate_beam_search(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        self.generate_beam_search_impl(input_tokens, max_new_tokens, eos_token_id, beam_size)
    }

    /// Generate multiple candidates using depth-1 beam selection followed by greedy decoding
    ///
    /// This is a simplified approach: select top-k initial tokens based on probability,
    /// then generate the rest of each sequence using greedy decoding independently.
    /// This is faster than true beam search but may miss globally optimal candidates.
    ///
    /// # Arguments
    /// * `input_tokens` - Input token sequence
    /// * `max_new_tokens` - Maximum new tokens to generate per candidate
    /// * `eos_token_id` - Optional EOS token ID to stop generation
    /// * `beam_size` - Number of candidates to generate
    ///
    /// Returns candidates sorted by initial token probability (highest first)
    pub fn generate_beam_search_d1_greedy(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        self.generate_beam_search_d1_greedy_batch(
            input_tokens,
            max_new_tokens,
            eos_token_id,
            beam_size,
        )
    }

    /// Generate multiple candidates using batch inference (depth-1 beam + greedy)
    ///
    /// Uses shared KV cache for input tokens across all sequences.
    /// Selects top-k initial tokens, then generates greedily for each beam.
    fn generate_beam_search_d1_greedy_batch(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        let backend = get_backend()?;

        // Set n_batch and n_ubatch large enough to avoid batch splitting
        // which causes "coupled sequences" error
        let batch_size = input_tokens
            .len()
            .saturating_mul(beam_size)
            .saturating_add(64)
            .min(u32::MAX as usize) as u32;
        let ctx_params = self
            .context_params()
            .with_n_seq_max(beam_size.try_into().unwrap_or(32))
            .with_n_batch(batch_size)
            .with_n_ubatch(batch_size);

        let mut ctx = self
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let model_eos = self.model.token_eos();
        let input_len = input_tokens.len();

        // Step 1: Process input tokens for ALL sequences in one batch
        // Add each token separately for each sequence (not coupled)
        let mut batch = LlamaBatch::new(512, 1);

        for (i, token) in input_tokens.iter().enumerate() {
            for seq_id in 0..beam_size as i32 {
                let is_last = i == input_len - 1 && seq_id == 0; // Only first seq needs logits
                batch
                    .add(*token, i as i32, &[seq_id], is_last)
                    .map_err(|e| KanjiError::Inference(e.into()))?;
            }
        }
        ctx.decode(&mut batch)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        // Step 2: Get top-k initial tokens (from any seq, all have same logits at this point)
        let logits = ctx.get_logits();
        let (top_tokens, top_log_probs) = self.get_top_k_tokens(logits, beam_size);

        // Step 3: Initialize beam state
        let mut beam_tokens: Vec<Vec<LlamaToken>> = top_tokens.iter().map(|&t| vec![t]).collect();
        let beam_scores: Vec<f32> = top_log_probs.clone();
        let mut beam_finished: Vec<bool> = vec![false; beam_size];

        // Check if any initial tokens are EOS
        for (i, &token) in top_tokens.iter().enumerate() {
            if self.is_eos_token(token, eos_token_id, model_eos) {
                beam_finished[i] = true;
            }
        }

        // Step 4: Add initial tokens to each beam's sequence
        batch.clear();
        for (beam_idx, &token) in top_tokens.iter().enumerate() {
            if !beam_finished[beam_idx] {
                batch
                    .add(token, input_len as i32, &[beam_idx as i32], true)
                    .map_err(|e| KanjiError::Inference(e.into()))?;
            }
        }

        if batch.n_tokens() > 0 {
            ctx.decode(&mut batch)
                .map_err(|e| KanjiError::Inference(e.into()))?;
        }

        // Step 5: Generate tokens for all beams in parallel
        let mut samplers: Vec<LlamaSampler> =
            (0..beam_size).map(|_| LlamaSampler::greedy()).collect();

        for _step in 0..(max_new_tokens - 1) {
            // Count active beams
            let active_count = beam_finished.iter().filter(|&&f| !f).count();
            if active_count == 0 {
                break;
            }

            // Sample next token for each active beam
            // Track which beams added tokens to know their logit positions
            let mut active_beams: Vec<usize> = Vec::new();
            let mut new_tokens: Vec<LlamaToken> = Vec::new();

            for (beam_idx, finished) in beam_finished.iter().enumerate() {
                if *finished {
                    continue;
                }
                // Logit position corresponds to order in the batch (0, 1, 2, ...)
                let logit_idx = active_beams.len() as i32;
                let new_token = samplers[beam_idx].sample(&ctx, logit_idx);
                active_beams.push(beam_idx);
                new_tokens.push(new_token);
            }

            // Process sampled tokens
            batch.clear();
            for (i, beam_idx) in active_beams.iter().enumerate() {
                let new_token = new_tokens[i];

                // Check for EOS
                if self.is_eos_token(new_token, eos_token_id, model_eos) {
                    beam_finished[*beam_idx] = true;
                } else {
                    beam_tokens[*beam_idx].push(new_token);
                    let pos = (input_len + beam_tokens[*beam_idx].len() - 1) as i32;
                    batch
                        .add(new_token, pos, &[*beam_idx as i32], true)
                        .map_err(|e| KanjiError::Inference(e.into()))?;
                }
            }

            // Decode all active beams at once
            if batch.n_tokens() > 0 {
                ctx.decode(&mut batch)
                    .map_err(|e| KanjiError::Inference(e.into()))?;
            } else {
                break;
            }
        }

        // Collect results
        let results: Vec<(Vec<LlamaToken>, f32)> =
            beam_tokens.into_iter().zip(beam_scores).collect();

        Ok(results)
    }

    /// Internal implementation of true beam search algorithm
    ///
    /// Unlike depth-1 beam methods which select top-k initial tokens
    /// and then generate greedily, this implements proper beam search that
    /// tracks cumulative probabilities at every step and keeps the globally
    /// best beam_size candidates.
    ///
    /// # Algorithm
    ///
    /// 1. Start with top-k initial tokens as beams
    /// 2. At each step:
    ///    - For each active beam, get top-k candidate next tokens
    ///    - Score each candidate: beam_score + log_prob(new_token)
    ///    - Keep only the best beam_size candidates globally
    /// 3. Repeat until all beams reach EOS or max_new_tokens
    ///
    /// True beam search implementation without KV cache sharing.
    /// This implementation processes full sequences at each step to avoid
    /// KV cache copy issues with GPT-2 models. It's slower but more reliable.
    fn generate_beam_search_impl(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        let model_eos = self.model.token_eos();

        // Step 1: Get initial logits
        let initial_logits = self.eval_sequence(input_tokens)?;
        let (top_tokens, top_log_probs) = self.get_top_k_tokens(&initial_logits, beam_size);

        // Initialize beams, partitioning EOS tokens into finished
        let mut beams: Vec<BeamState> = Vec::with_capacity(beam_size);
        let mut finished_beams: Vec<BeamState> = Vec::new();

        for (&token, &log_prob) in top_tokens.iter().zip(top_log_probs.iter()) {
            let beam = BeamState {
                tokens: vec![token],
                score: log_prob,
            };

            if self.is_eos_token(token, eos_token_id, model_eos) {
                finished_beams.push(beam);
            } else {
                beams.push(beam);
            }
        }

        // Expansion factor
        let expand_k = beam_size.max(4);

        // Step 2: Main beam search loop
        for _step in 0..(max_new_tokens - 1) {
            if beams.is_empty() {
                break;
            }

            // Early termination check
            if finished_beams.len() >= beam_size {
                let best_finished = finished_beams
                    .iter()
                    .map(|b| b.score)
                    .fold(f32::NEG_INFINITY, f32::max);
                let best_active = beams
                    .iter()
                    .map(|b| b.score)
                    .fold(f32::NEG_INFINITY, f32::max);
                if best_active < best_finished {
                    break;
                }
            }

            // Collect candidates from all beams
            let mut candidates: Vec<BeamState> = Vec::new();

            for beam in &beams {
                // Build full sequence: input_tokens + beam.tokens
                let mut full_seq: Vec<LlamaToken> = input_tokens.to_vec();
                full_seq.extend(&beam.tokens);

                // Get logits for this sequence
                let logits = self.eval_sequence(&full_seq)?;
                let (top_tokens, top_log_probs) = self.get_top_k_tokens(&logits, expand_k);

                // Create candidates
                for (&token, &log_prob) in top_tokens.iter().zip(top_log_probs.iter()) {
                    let mut new_tokens = beam.tokens.clone();
                    new_tokens.push(token);

                    candidates.push(BeamState {
                        tokens: new_tokens,
                        score: beam.score + log_prob,
                    });
                }
            }

            // Sort and keep top beam_size candidates
            candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
            candidates.truncate(beam_size);

            // Partition into finished and active beams
            beams.clear();
            for candidate in candidates {
                let last_token = match candidate.tokens.last() {
                    Some(&t) => t,
                    None => continue,
                };

                if self.is_eos_token(last_token, eos_token_id, model_eos) {
                    finished_beams.push(candidate);
                } else {
                    beams.push(candidate);
                }
            }
        }

        // Combine all results
        let mut all_results: Vec<(Vec<LlamaToken>, f32)> = finished_beams
            .into_iter()
            .chain(beams)
            .map(|b| (b.tokens, b.score))
            .collect();

        // Sort by score and take top beam_size
        all_results.sort_by(|a, b| b.1.total_cmp(&a.1));
        all_results.truncate(beam_size);

        Ok(all_results)
    }

    /// Process a token sequence and return the logits at the last position.
    ///
    /// Creates a fresh context for each call. Used by true beam search where
    /// each beam needs independent evaluation.
    fn eval_sequence(&self, tokens: &[LlamaToken]) -> Result<Vec<f32>> {
        let backend = get_backend()?;
        let ctx_params = self.context_params();

        let mut ctx = self
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let mut batch = LlamaBatch::new(512, 1);
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| KanjiError::Inference(e.into()))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        Ok(ctx.get_logits().to_vec())
    }

    /// Get top-k tokens from logits with log probabilities
    fn get_top_k_tokens(&self, logits: &[f32], k: usize) -> (Vec<LlamaToken>, Vec<f32>) {
        // Convert logits to log probabilities using log-softmax
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let log_sum_exp: f32 = logits
            .iter()
            .map(|&x| (x - max_logit).exp())
            .sum::<f32>()
            .ln()
            + max_logit;

        let mut token_scores: Vec<(usize, f32)> = logits
            .iter()
            .enumerate()
            .map(|(i, &x)| (i, x - log_sum_exp))
            .collect();
        token_scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        token_scores.truncate(k);

        let tokens: Vec<LlamaToken> = token_scores
            .iter()
            .map(|(i, _)| LlamaToken(*i as i32))
            .collect();
        let log_probs: Vec<f32> = token_scores.iter().map(|(_, lp)| *lp).collect();

        (tokens, log_probs)
    }

    /// Check if a token is an EOS token.
    ///
    /// Uses the model's own EOS/EOG metadata rather than hardcoded token IDs.
    fn is_eos_token(
        &self,
        token: LlamaToken,
        eos_token_id: Option<i32>,
        model_eos: LlamaToken,
    ) -> bool {
        eos_token_id.is_some_and(|eos| token.0 == eos)
            || token == model_eos
            || self.model.is_eog_token(token)
    }

    /// Generate tokens with a custom sampler
    fn generate_with_sampler(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        mut sampler: LlamaSampler,
    ) -> Result<Vec<LlamaToken>> {
        let backend = get_backend()?;
        let ctx_params = self.context_params();

        let mut ctx = self
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let mut batch = LlamaBatch::new(512, 1);
        let mut generated = input_tokens.to_vec();

        // Process input tokens
        for (i, token) in input_tokens.iter().enumerate() {
            let is_last = i == input_tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| KanjiError::Inference(e.into()))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let mut n_cur = input_tokens.len();

        // Get model's EOS token for comparison
        let model_eos = self.model.token_eos();

        // Generate new tokens
        for _ in 0..max_new_tokens {
            let new_token = sampler.sample(&ctx, -1);

            // Check for EOS using the provided token ID
            if let Some(eos) = eos_token_id
                && new_token.0 == eos
            {
                break;
            }

            // Check against model's EOS token
            if new_token == model_eos {
                break;
            }

            // Check if model thinks it's end of generation
            if self.model.is_eog_token(new_token) {
                break;
            }

            generated.push(new_token);

            // Prepare next batch with just the new token
            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|e| KanjiError::Inference(e.into()))?;

            ctx.decode(&mut batch)
                .map_err(|e| KanjiError::Inference(e.into()))?;
            n_cur += 1;
        }

        Ok(generated)
    }

    /// Get the EOS token ID from the model
    pub fn eos_token_id(&self) -> LlamaToken {
        self.model.token_eos()
    }
}

/// Reusable NLL scorer that keeps a single llama.cpp context alive.
///
/// Creating a `LlamaContext` is expensive. This struct amortizes the cost by
/// creating one context and clearing the KV cache between calls.
/// Use one `NllScorer` per thread for parallel scoring.
pub struct NllScorer<'a> {
    model: &'a LlamaCppModel,
    ctx: llama_cpp_2::context::LlamaContext<'a>,
    vocab_size: usize,
}

impl<'a> NllScorer<'a> {
    /// Create a new NLL scorer with a reusable context.
    pub fn new(model: &'a LlamaCppModel, n_ctx: u32) -> Result<Self> {
        let backend = get_backend()?;

        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(
            NonZeroU32::new(n_ctx).expect("n_ctx must be non-zero"),
        ));

        let ctx = model
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let vocab_size = model.model.n_vocab() as usize;

        Ok(Self {
            model,
            ctx,
            vocab_size,
        })
    }

    /// Compute per-character NLL for a single (reading, surface) pair.
    ///
    /// Reuses the internal context by clearing the KV cache between calls.
    pub fn compute_nll(&mut self, reading_katakana: &str, surface: &str) -> Result<f32> {
        use super::{CONTEXT_TOKEN, INPUT_START_TOKEN, OUTPUT_START_TOKEN};

        let prompt = format!(
            "{}{}{}{}{}",
            CONTEXT_TOKEN, "", INPUT_START_TOKEN, reading_katakana, OUTPUT_START_TOKEN
        );
        let full_text = format!("{}{}", prompt, surface);

        let prompt_tokens = self.model.tokenize(&prompt)?;
        let full_tokens = self.model.tokenize(&full_text)?;

        if full_tokens.len() <= prompt_tokens.len() {
            return Ok(100.0);
        }

        let n_tokens = full_tokens.len();

        self.ctx.clear_kv_cache();

        let mut batch = LlamaBatch::new(n_tokens.max(512), 1);
        batch
            .add_sequence(&full_tokens, 0, true)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        self.ctx
            .decode(&mut batch)
            .map_err(|e| KanjiError::Inference(e.into()))?;

        let start_pos = prompt_tokens.len() - 1;
        let end_pos = n_tokens - 1;
        let mut total_nll: f32 = 0.0;
        let mut n_scored = 0;

        for pos in start_pos..end_pos {
            let logits = self.ctx.get_logits_ith(pos as i32);

            let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let log_sum_exp: f32 = logits
                .iter()
                .take(self.vocab_size)
                .map(|&x| (x - max_logit).exp())
                .sum::<f32>()
                .ln()
                + max_logit;

            let target = full_tokens[pos + 1].0 as usize;
            if target < self.vocab_size {
                total_nll -= logits[target] - log_sum_exp;
            }
            n_scored += 1;
        }

        if n_scored == 0 {
            return Ok(100.0);
        }

        let n_chars = surface.chars().count().max(1);
        Ok(total_nll / n_chars as f32)
    }
}
