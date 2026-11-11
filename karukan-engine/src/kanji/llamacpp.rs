//! llama.cpp based GGUF inference for kanji conversion
//!
//! This module provides an alternative to Candle's GGUF implementation using
//! llama.cpp's optimized inference engine via the llama-cpp-2 crate.
//!
//! Enable with the `llamacpp` feature flag.

use super::error::KanjiError;
type Result<T> = super::error::Result<T>;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

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

/// Beam ceiling: live + scratch slots must stay within llama.cpp's
/// LLAMA_MAX_SEQ (256), which throws a C++ exception that aborts across FFI.
const MAX_BEAM_SIZE: usize = 128;

/// Spare KV cells beyond the computed prompt + generation rows.
const KV_HEADROOM_CELLS: usize = 64;

/// Spare batch slots beyond the computed prompt / sequence rows.
const BATCH_HEADROOM: usize = 8;

/// Wrap any llama.cpp error as [`KanjiError::Inference`].
fn inference_err(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> KanjiError {
    KanjiError::Inference(e.into())
}

/// Convert bytes to hex display format for partial UTF-8 sequences
fn bytes_to_hex_display(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("<{:02X}>", b)).collect()
}

/// Whether an added-token string is a byte-fallback token like `<0xCE>`.
/// tokenizer.json marks them `special: true`, but they carry real output
/// bytes and must never be skipped as special. The check mirrors the
/// tokenizer's own `ByteFallback` decoder match exactly.
fn is_byte_fallback_token(token: &str) -> bool {
    token.len() == 6
        && token.starts_with("<0x")
        && token.ends_with('>')
        && u8::from_str_radix(&token[3..5], 16).is_ok()
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

/// How many trailing tokens of the previous prompt may differ for its output
/// to be offered as a draft. A keystroke rewrites the last kana token or two
/// plus the output marker behind them; a prompt that differs further back is
/// a different conversion, whose old output would be decoded only to be
/// thrown away.
const DRAFT_PROMPT_SLACK: usize = 4;

/// KV state the greedy path carries between calls: a persistent context and
/// the token sequence its cache currently represents, kept as the last
/// prompt and the output generated from it. Typing grows the prompt at the
/// tail, so the next call usually only has to decode the few tokens past the
/// common prefix, and most of its output is the previous one again.
struct GreedySession {
    /// Borrows the model next to it in [`LoadedModel`]; the lifetime is
    /// erased there under the invariants documented on that field.
    ctx: LlamaContext<'static>,
    /// The last prompt, cached at positions `0..prompt.len()`.
    prompt: Vec<LlamaToken>,
    /// What was generated from it, cached right behind the prompt.
    output: Vec<LlamaToken>,
    /// The settings the context was built with. The session is shared by
    /// every wrapper of the model, and one asking for other settings
    /// rebuilds it.
    n_ctx: u32,
    n_threads: u32,
    /// Decode calls made so far, so a test can see the draft saving them.
    #[cfg(test)]
    decodes: usize,
}

// SAFETY: the context is only touched while holding the mutex it is stored
// behind in `LoadedModel`, so accesses never overlap; llama.cpp allows
// moving a context between threads when calls are serialized.
unsafe impl Send for GreedySession {}

/// Whether a conversion given `deadline` has run out of time. Asked between
/// decode calls — a call in progress cannot be interrupted, so a conversion
/// overruns its deadline by at most one of them.
fn out_of_time(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

/// Length of the longest common prefix of two token sequences.
fn common_prefix_len(a: &[LlamaToken], b: &[LlamaToken]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// The highest-scoring token, the lower id winning a tie — what llama.cpp's
/// greedy sampler picks, without building its vocab-sized candidate array.
///
/// Two passes on purpose: tracking the index alongside the maximum chains
/// every comparison to the one before it, while a bare maximum vectorizes
/// and the search for it stops at the first hit.
fn argmax_token(logits: &[f32]) -> LlamaToken {
    let max = max_logit(logits);
    let best = logits.iter().position(|&logit| logit == max).unwrap_or(0);
    LlamaToken(best as i32)
}

fn max_logit(logits: &[f32]) -> f32 {
    logits.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

/// The `k` highest logits with their positions, best first; among equal
/// logits the lower position comes first, as a stable full sort would have it.
///
/// A single pass with a `k`-entry list (k is the beam width, a handful):
/// sorting a vocab-sized vector per beam per step used to dominate the beam
/// loop. Once the list is full almost nothing in the vocabulary beats its
/// last entry, so the row is walked in blocks and a block whose maximum does
/// not is skipped whole.
fn top_k_logits(logits: &[f32], k: usize) -> Vec<(usize, f32)> {
    const BLOCK: usize = 16;
    let k = k.min(logits.len());
    if k == 0 {
        return Vec::new();
    }
    let mut top: Vec<(usize, f32)> = Vec::with_capacity(k + 1);
    for (block, chunk) in logits.chunks(BLOCK).enumerate() {
        if top.len() == k && max_logit(chunk) <= top[k - 1].1 {
            continue;
        }
        for (offset, &x) in chunk.iter().enumerate() {
            if top.len() == k && x <= top[k - 1].1 {
                continue;
            }
            let pos = top.partition_point(|&(_, s)| s >= x);
            top.insert(pos, (block * BLOCK + offset, x));
            top.truncate(k);
        }
    }
    top
}

/// ln Σ e^x over `logits`: the log-softmax normalizer. The maximum is
/// taken out first so no term overflows.
fn log_sum_exp(logits: &[f32]) -> f32 {
    let max = max_logit(logits);
    logits.iter().map(|&x| (x - max).exp()).sum::<f32>().ln() + max
}

/// A loaded GGUF model with what is derived from it, shared by every
/// [`LlamaCppModel`] that loaded the same files.
struct LoadedModel {
    /// Persistent context for the greedy path. Shared like the model
    /// itself: it is tens of megabytes of KV and compute buffers, and a
    /// frontend that keeps one engine per input field would otherwise hold
    /// one per field. Declared before `model` so it drops first — its
    /// context borrows the model next to it.
    greedy_session: Mutex<Option<GreedySession>>,
    /// Never replaced after construction: `greedy_session` borrows it, and
    /// the `Arc` this struct lives in keeps its address stable.
    model: LlamaModel,
    /// External HuggingFace tokenizer (always required).
    /// `tokenize()` and `decode()` use this instead of llama.cpp's built-in tokenizer.
    tokenizer: tokenizers::Tokenizer,
    /// Token ids `decode(_, skip_special_tokens=true)` removes before
    /// detokenizing: added tokens with `special: true`, minus the
    /// byte-fallback tokens (see [`is_byte_fallback_token`]).
    special_token_ids: HashSet<u32>,
}

/// llama.cpp based model for GGUF inference
pub struct LlamaCppModel {
    shared: Arc<LoadedModel>,
    n_ctx: u32,
    /// Number of threads for inference (0 = use llama.cpp default)
    n_threads: u32,
}

/// Loaded models by their (model, tokenizer) paths.
type ModelCache = HashMap<(String, String), Arc<LoadedModel>>;

/// Process-global cache of loaded GGUF models, keyed by their
/// (model, tokenizer) paths.
///
/// Karukan creates one engine per input context (window/field) and each
/// engine loads its own converters; without this cache every new context
/// reloaded the GGUF from disk (and held a separate copy in memory). The
/// greedy session is shared along with the model, behind its mutex: what it
/// remembers of the previous conversion is only ever a head start, so an
/// engine picking it up after another one costs a prefill, not correctness.
static MODEL_CACHE: OnceLock<Mutex<ModelCache>> = OnceLock::new();

/// llama.cpp aborts the process when the model file is absent, so check
/// first and fail as an ordinary error the caller can degrade on.
fn ensure_model_file_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(KanjiError::ModelNotFound(path.to_path_buf()));
    }
    Ok(())
}

impl LlamaCppModel {
    /// Load a GGUF model using llama.cpp with an external tokenizer.
    ///
    /// Inference runs on the CPU only; no GPU device is attached.
    pub fn from_file<P: AsRef<Path>, T: AsRef<Path>>(path: P, tokenizer_json: T) -> Result<Self> {
        Self::from_file_with_n_ctx(path, tokenizer_json, 256)
    }

    /// Load a GGUF model with explicit context window size
    pub fn from_file_with_n_ctx<P: AsRef<Path>, T: AsRef<Path>>(
        path: P,
        tokenizer_json: T,
        n_ctx: u32,
    ) -> Result<Self> {
        let cache_key = (
            path.as_ref().to_string_lossy().to_string(),
            tokenizer_json.as_ref().to_string_lossy().to_string(),
        );

        // Cache hit: reuse what is already loaded.
        let cache = MODEL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(shared) = cache.lock().unwrap().get(&cache_key).cloned() {
            return Ok(Self::wrap(shared, n_ctx));
        }

        let loaded = Arc::new(Self::load(path, tokenizer_json)?);
        // Two engines can be loading the same model at once. Whichever gets
        // here first is the copy everyone shares; the other is dropped.
        let shared = cache
            .lock()
            .unwrap()
            .entry(cache_key)
            .or_insert(loaded)
            .clone();
        Ok(Self::wrap(shared, n_ctx))
    }

    /// Load the model outside the process-global cache, with a greedy
    /// session of its own — for tests that count what the session does.
    #[cfg(test)]
    fn from_file_unshared<P: AsRef<Path>, T: AsRef<Path>>(
        path: P,
        tokenizer_json: T,
    ) -> Result<Self> {
        Ok(Self::wrap(Arc::new(Self::load(path, tokenizer_json)?), 256))
    }

    fn wrap(shared: Arc<LoadedModel>, n_ctx: u32) -> Self {
        Self {
            shared,
            n_ctx,
            n_threads: 0,
        }
    }

    /// Read the GGUF and its external tokenizer from disk.
    fn load<P: AsRef<Path>, T: AsRef<Path>>(path: P, tokenizer_json: T) -> Result<LoadedModel> {
        ensure_model_file_exists(path.as_ref())?;
        let backend = get_backend()?;

        // CPU only, and that takes more than keeping the layers off the GPU.
        // With the default device list llama.cpp still attaches a GPU
        // backend to every context: on macOS each `new_context` initialized
        // Metal and reserved a 35 MiB compute buffer on it that no layer
        // ever used. That is a millisecond of every conversion when the GPU
        // is idle, and a wait behind whatever owns the GPU when it is not —
        // a game in the foreground stalled typing. An empty device list
        // leaves the CPU backend alone. (The models are small enough that
        // the CPU wins anyway: fully offloaded to Metal, jinen-v2-xsmall
        // converts in 12 ms against 7 ms.)
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(0)
            .with_devices(&[])
            .map_err(|e| KanjiError::ModelLoad(e.into()))?;

        let model = LlamaModel::load_from_file(backend, path.as_ref(), &model_params)
            .map_err(|e| KanjiError::ModelLoad(e.into()))?;

        let tokenizer = load_tokenizer(tokenizer_json)?;
        let special_token_ids = tokenizer
            .get_added_tokens_decoder()
            .into_iter()
            .filter(|(_, tok)| tok.special && !is_byte_fallback_token(&tok.content))
            .map(|(id, _)| id)
            .collect();

        Ok(LoadedModel {
            greedy_session: Mutex::new(None),
            model,
            tokenizer,
            special_token_ids,
        })
    }

    /// Set the number of threads for inference.
    /// 0 means use llama.cpp default (typically all cores).
    pub fn set_n_threads(&mut self, n: u32) {
        self.n_threads = n;
    }

    fn lock_greedy_session(&self) -> std::sync::MutexGuard<'_, Option<GreedySession>> {
        self.shared
            .greedy_session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Build LlamaContextParams with configured n_threads
    fn context_params(&self) -> LlamaContextParams {
        self.context_params_with_n_ctx(self.n_ctx)
    }

    /// New inference context on the global backend.
    fn new_context(&self, params: LlamaContextParams) -> Result<LlamaContext<'_>> {
        let backend = get_backend()?;
        self.shared
            .model
            .new_context(backend, params)
            .map_err(inference_err)
    }

    /// Build LlamaContextParams with configured n_threads and an explicit KV
    /// cache size. Beam search needs more cells than a greedy run: every beam
    /// keeps its own generated tokens alongside the shared prompt.
    fn context_params_with_n_ctx(&self, n_ctx: u32) -> LlamaContextParams {
        let params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(n_ctx).unwrap_or(NonZeroU32::MIN)));
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
            .shared
            .tokenizer
            .encode(text, false)
            .map_err(KanjiError::Inference)?;
        let tokens: Vec<LlamaToken> = encoding
            .get_ids()
            .iter()
            .map(|&id| LlamaToken(id as i32))
            .collect();
        Ok(tokens)
    }

    /// Decode tokens via the external tokenizer. Special tokens are
    /// filtered here by id, not by the tokenizer's own skip flag — that
    /// would also drop byte-fallback tokens (`<0xCE>`…) before its
    /// ByteFallback decoder can fuse them back into characters.
    pub fn decode(&self, tokens: &[LlamaToken], skip_special_tokens: bool) -> Result<String> {
        let ids: Vec<u32> = tokens
            .iter()
            .map(|t| t.0 as u32)
            .filter(|id| !(skip_special_tokens && self.shared.special_token_ids.contains(id)))
            .collect();
        let text = self
            .shared
            .tokenizer
            .decode(&ids, false)
            .map_err(KanjiError::Inference)?;
        Ok(text)
    }

    /// Decode a single token for display purposes.
    ///
    /// For byte-level BPE tokens that represent partial UTF-8 sequences,
    /// this returns a hex representation like `<0xE3>` instead of replacement characters.
    pub fn decode_token_for_display(&self, token: LlamaToken) -> String {
        let bytes = self.shared.model.vocab().token_to_piece(token, true, None);
        if bytes.is_empty() {
            // Unknown token type: llama.cpp wrote nothing for it
            return format!("<{}>", token.0);
        }
        // Valid UTF-8 without control characters is shown as-is; a partial UTF-8
        // sequence or a control character falls back to the hex form.
        if let Ok(s) = std::str::from_utf8(&bytes)
            && s.chars().all(|c| !c.is_control() || c == ' ' || c == '\n')
        {
            s.to_string()
        } else {
            bytes_to_hex_display(&bytes)
        }
    }

    /// Generate tokens with greedy decoding.
    ///
    /// Runs on a persistent context, so a prompt that grows keystroke by
    /// keystroke pays for what changed rather than for the whole conversion:
    /// only the prompt past the longest common prefix with the previous call
    /// is decoded, and the previous output rides in that same batch as a
    /// draft to be confirmed (see [`Self::run_greedy`]). Falls back to a
    /// one-shot context when the prompt cannot fit the session.
    pub fn generate(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
    ) -> Result<Vec<LlamaToken>> {
        self.generate_within(input_tokens, max_new_tokens, eos_token_id, None)
    }

    /// [`Self::generate`] with a deadline: a generation still running when
    /// it passes is abandoned with [`KanjiError::Timeout`].
    pub fn generate_within(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        deadline: Option<Instant>,
    ) -> Result<Vec<LlamaToken>> {
        if input_tokens.is_empty() || input_tokens.len() + max_new_tokens >= self.n_ctx as usize {
            return self.generate_with_sampler(
                input_tokens,
                max_new_tokens,
                eos_token_id,
                LlamaSampler::greedy(),
                deadline,
            );
        }

        let mut guard = self.lock_greedy_session();
        // The session belongs to the model, not to this wrapper: one built
        // for another wrapper's settings is rebuilt for these.
        if guard
            .as_ref()
            .is_some_and(|s| (s.n_ctx, s.n_threads) != (self.n_ctx, self.n_threads))
        {
            *guard = None;
        }
        if guard.is_none() {
            *guard = Some(self.new_greedy_session()?);
        }
        let session = guard.as_mut().expect("session initialized above");
        let result = self.run_greedy(
            session,
            input_tokens,
            max_new_tokens,
            eos_token_id,
            deadline,
        );
        // A timeout leaves the session in step with its cache, and what it
        // decoded is the prefix of the next attempt. Any other failure
        // leaves the cache in an unknown state: rebuild next call.
        if matches!(result, Err(ref e) if !matches!(e, KanjiError::Timeout)) {
            *guard = None;
        }
        result
    }

    /// A fresh persistent context for the greedy path. Batches are sized to
    /// n_ctx: unlike the one-shot paths this allocation happens once, not
    /// per conversion, so there is nothing to win by shrinking it.
    fn new_greedy_session(&self) -> Result<GreedySession> {
        let params = self
            .context_params()
            .with_n_batch(self.n_ctx)
            .with_n_ubatch(self.n_ctx);
        let ctx = self.new_context(params)?;
        // SAFETY: the context borrows the model in `self.shared`, and the
        // session is stored only in that same `LoadedModel`. The `Arc` keeps
        // the model's address stable, nothing replaces it, and the session's
        // field is declared before it, so the context drops first.
        let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };
        Ok(GreedySession {
            ctx,
            prompt: Vec::new(),
            output: Vec::new(),
            n_ctx: self.n_ctx,
            n_threads: self.n_threads,
            #[cfg(test)]
            decodes: 0,
        })
    }

    /// Greedy generation on the session context.
    ///
    /// What a conversion costs is the number of decode calls: a call streams
    /// every weight of the model through memory whether it carries one token
    /// or twenty, so generating token by token pays that once per token.
    /// Typing rarely changes more than the tail of the output, so the
    /// previous output is offered as a draft: it is decoded in the same
    /// batch as the changed part of the prompt, and each draft token is kept
    /// while it is what the model itself picks at that position. The first
    /// one that is not ends the draft, and generation continues token by
    /// token from the model's own choice. Every kept token is therefore the
    /// model's argmax given the tokens before it — the draft only decides
    /// how many decode calls it takes to find that out.
    ///
    /// Batched and single-token evaluation round differently inside
    /// llama.cpp's quantized kernels, so where two tokens score within a
    /// rounding error of each other the pick can differ from what a fresh
    /// context makes; nothing else can.
    fn run_greedy(
        &self,
        session: &mut GreedySession,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        deadline: Option<Instant>,
    ) -> Result<Vec<LlamaToken>> {
        let n = input_tokens.len();
        let common = common_prefix_len(&session.prompt, input_tokens);
        // Re-decode at least the final prompt token: picking the first
        // output token needs its logits even when the whole prompt is cached.
        let start = common.min(n - 1);

        // Invalidate before touching the cache: a failed decode leaves it in
        // an unknown state, and `generate` then discards the session.
        let previous_prompt_len = session.prompt.len();
        session.prompt.clear();
        let mut draft = std::mem::take(&mut session.output);
        if common + DRAFT_PROMPT_SLACK < previous_prompt_len {
            draft.clear();
        }
        draft.truncate(max_new_tokens);

        session
            .ctx
            .kv_cache_seq_rm(0, Some(start as u32), None)
            .map_err(inference_err)?;

        // One batch: the changed prompt tail, then the draft behind it.
        // Logits are needed where a token gets picked — after the last
        // prompt token and after every draft token.
        let mut batch = LlamaBatch::new(self.n_ctx as usize, 1);
        for (i, token) in input_tokens.iter().enumerate().skip(start) {
            batch
                .add(*token, i as i32, &[0], i == n - 1)
                .map_err(inference_err)?;
        }
        for (i, token) in draft.iter().enumerate() {
            batch
                .add(*token, (n + i) as i32, &[0], true)
                .map_err(inference_err)?;
        }
        session.ctx.decode(&mut batch).map_err(inference_err)?;
        #[cfg(test)]
        {
            session.decodes += 1;
        }

        let model_eos = self.shared.model.vocab().eos();
        let mut generated = input_tokens.to_vec();

        // Walk the draft. Row `last_prompt_row + i` holds the logits that
        // pick output token `i`: the last prompt token's for the first, the
        // draft token before it for the rest.
        let last_prompt_row = (n - 1 - start) as i32;
        let mut kept = 0;
        // The model's own pick where the draft stops matching, or where it
        // runs out; `None` once the output is complete.
        let mut next = None;
        while kept < max_new_tokens {
            let logits = session.ctx.get_logits_ith(last_prompt_row + kept as i32);
            let token = argmax_token(logits);
            if self.is_eos_token(token, eos_token_id, model_eos) {
                break;
            }
            if draft.get(kept) != Some(&token) {
                next = Some(token);
                break;
            }
            generated.push(token);
            kept += 1;
        }
        // Drop the draft tokens that were not kept from the cache.
        if kept < draft.len() {
            session
                .ctx
                .kv_cache_seq_rm(0, Some((n + kept) as u32), None)
                .map_err(inference_err)?;
        }

        // Past the draft, token by token.
        let mut timed_out = false;
        while let Some(token) = next {
            if out_of_time(deadline) {
                timed_out = true;
                break;
            }
            let pos = generated.len();
            generated.push(token);
            batch.clear();
            batch
                .add(token, pos as i32, &[0], true)
                .map_err(inference_err)?;
            session.ctx.decode(&mut batch).map_err(inference_err)?;
            #[cfg(test)]
            {
                session.decodes += 1;
            }
            if generated.len() - n >= max_new_tokens {
                break;
            }
            let token = argmax_token(session.ctx.get_logits_ith(0));
            next = (!self.is_eos_token(token, eos_token_id, model_eos)).then_some(token);
        }

        // Everything in `generated` has been decoded, on a timeout too, so
        // the session stays in step with its cache either way.
        session.prompt.extend_from_slice(input_tokens);
        session.output.extend_from_slice(&generated[n..]);
        if timed_out {
            return Err(KanjiError::Timeout);
        }
        Ok(generated)
    }

    /// Depth-1 beam: pick the top-k initial tokens, then continue each
    /// sequence greedily and independently, all sharing one prompt KV cache.
    /// Faster than true beam search but may miss globally optimal
    /// candidates. Sorted by initial token probability.
    pub fn generate_beam_search_d1_greedy(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        // Set n_batch and n_ubatch large enough to avoid batch splitting
        // which causes "coupled sequences" error
        let batch_size = input_tokens
            .len()
            .saturating_mul(beam_size)
            .saturating_add(64)
            .min(u32::MAX as usize) as u32;
        let mut ctx = self.new_context(
            self.context_params()
                .with_n_seq_max(beam_size.try_into().unwrap_or(32))
                .with_n_batch(batch_size)
                .with_n_ubatch(batch_size),
        )?;

        let model_eos = self.shared.model.vocab().eos();
        let input_len = input_tokens.len();

        // Step 1: Process input tokens for ALL sequences in one batch
        // Add each token separately for each sequence (not coupled)
        let mut batch = LlamaBatch::new(512, 1);

        for (i, token) in input_tokens.iter().enumerate() {
            for seq_id in 0..beam_size as i32 {
                let is_last = i == input_len - 1 && seq_id == 0; // Only first seq needs logits
                batch
                    .add(*token, i as i32, &[seq_id], is_last)
                    .map_err(inference_err)?;
            }
        }
        ctx.decode(&mut batch).map_err(inference_err)?;

        // Step 2: Get top-k initial tokens (from any seq, all have same logits at this point)
        let top = self.get_top_k_tokens(ctx.get_logits(), beam_size);

        // Step 3: Initialize beam state. A beam whose initial token is
        // already EOS is finished before it generates anything.
        let mut beam_tokens: Vec<Vec<LlamaToken>> = top.iter().map(|&(t, _)| vec![t]).collect();
        let beam_scores: Vec<f32> = top.iter().map(|&(_, score)| score).collect();
        let mut beam_finished: Vec<bool> = top
            .iter()
            .map(|&(token, _)| self.is_eos_token(token, eos_token_id, model_eos))
            .collect();

        // Step 4: Add initial tokens to each beam's sequence
        batch.clear();
        for (beam_idx, &(token, _)) in top.iter().enumerate() {
            if !beam_finished[beam_idx] {
                batch
                    .add(token, input_len as i32, &[beam_idx as i32], true)
                    .map_err(inference_err)?;
            }
        }

        if batch.n_tokens() > 0 {
            ctx.decode(&mut batch).map_err(inference_err)?;
        }

        // Step 5: Generate tokens for all beams in parallel. Greedy sampling
        // is stateless, so one sampler serves every beam.
        let mut sampler = LlamaSampler::greedy();

        for _step in 0..(max_new_tokens - 1) {
            if beam_finished.iter().all(|&finished| finished) {
                break;
            }

            // Sample the next token per active beam. The logit row is the
            // beam's position in the previous batch, i.e. its index among the
            // active beams.
            let sampled: Vec<(usize, LlamaToken)> = beam_finished
                .iter()
                .enumerate()
                .filter(|&(_, &finished)| !finished)
                .enumerate()
                .map(|(logit_idx, (beam_idx, _))| {
                    (beam_idx, sampler.sample(&ctx, logit_idx as i32))
                })
                .collect();

            // Process sampled tokens
            batch.clear();
            for &(beam_idx, new_token) in &sampled {
                if self.is_eos_token(new_token, eos_token_id, model_eos) {
                    beam_finished[beam_idx] = true;
                } else {
                    beam_tokens[beam_idx].push(new_token);
                    let pos = (input_len + beam_tokens[beam_idx].len() - 1) as i32;
                    batch
                        .add(new_token, pos, &[beam_idx as i32], true)
                        .map_err(inference_err)?;
                }
            }

            // Decode all active beams at once
            if batch.n_tokens() == 0 {
                break;
            }
            ctx.decode(&mut batch).map_err(inference_err)?;
        }

        Ok(beam_tokens.into_iter().zip(beam_scores).collect())
    }

    /// True beam search: tracks cumulative probabilities at every step and
    /// keeps the globally best `beam_size` candidates, highest first.
    ///
    /// Runs on a single context with a reused KV cache — the prompt is
    /// decoded once into slot 0 and shared via cache copies, each live beam
    /// owns one sequence slot, and a step decodes only one new token per
    /// beam. Must return the same candidates as
    /// [`Self::generate_beam_search_full_eval`], the re-prefilling reference
    /// the equivalence test compares against.
    ///
    /// KV-cache-reuse formulation contributed by
    /// [kazuph/karukan@707eb10](https://github.com/kazuph/karukan/commit/707eb101f5de7b210f993b74723cf0ba9cc8c2af).
    pub fn generate_beam_search(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        self.generate_beam_search_within(
            input_tokens,
            max_new_tokens,
            eos_token_id,
            beam_size,
            None,
        )
    }

    /// [`Self::generate_beam_search`] with a deadline: a search still
    /// running when it passes is abandoned with [`KanjiError::Timeout`].
    pub fn generate_beam_search_within(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
        deadline: Option<Instant>,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        // max_new_tokens == 0 means "generate nothing": return before paying
        // for the prompt decode instead of emitting one-token pseudo-beams.
        if beam_size == 0 || max_new_tokens == 0 || input_tokens.is_empty() {
            return Ok(Vec::new());
        }
        let beam_size = beam_size.min(MAX_BEAM_SIZE);
        let model_eos = self.shared.model.vocab().eos();
        let input_len = input_tokens.len();

        // Sequence slots 0..beam_size hold the live beams. The scratch slots
        // above them stage a permutation: several new beams can descend from
        // one parent, and a parent slot is itself a destination, so copying
        // straight into the live slots would clobber a prefix still needed.
        let n_seq = beam_size * 2;
        // Cells hold the prompt (shared by every beam) plus one row of
        // generated tokens per beam, doubled as slack for the scratch staging.
        let n_cells = input_len + 2 * beam_size * (max_new_tokens + 1) + KV_HEADROOM_CELLS;
        // n_batch also caps the context's output buffer (n_outputs_max), which
        // llama.cpp reserves for n_seq_max rows — so it must cover n_seq too.
        let batch_cap = input_len.max(n_seq) + BATCH_HEADROOM;

        // One unified KV buffer for all slots. llama.cpp's default gives every
        // sequence its own stream, where a cross-sequence copy is a memcpy of
        // the whole K/V buffer of every layer; the beam permutation issues
        // two of those per beam per step, and they cost more than the decode
        // they follow. In a unified buffer a copy only tags the parent's
        // cells with the child's id, so beams share their common prefix.
        let mut ctx = self.new_context(
            self.context_params_with_n_ctx(n_cells as u32)
                .with_kv_unified(true)
                .with_n_seq_max(n_seq.try_into().unwrap_or(u32::MAX))
                .with_n_batch(batch_cap as u32)
                .with_n_ubatch(batch_cap as u32),
        )?;

        // Step 1: decode the prompt once into slot 0 and read its logits.
        let mut batch = LlamaBatch::new(batch_cap, 1);
        self.add_input_tokens(&mut batch, input_tokens)?;
        ctx.decode(&mut batch).map_err(inference_err)?;

        let top = self.get_top_k_tokens(ctx.get_logits(), beam_size);
        let (mut beams, mut finished_beams) =
            self.partition_initial_beams(top, eos_token_id, model_eos);

        // Every surviving beam starts from the same prompt prefix.
        for slot in 1..beams.len() {
            ctx.copy_kv_cache_seq(0, slot as i32, None, None)
                .map_err(inference_err)?;
        }

        // Expansion factor
        let expand_k = beam_size.max(4);
        let mut candidates: Vec<(usize, LlamaToken, f32)> =
            Vec::with_capacity(beam_size * expand_k);
        let mut next: Vec<(usize, BeamState)> = Vec::with_capacity(beam_size);

        // Step 2: Main beam search loop
        for _step in 0..max_new_tokens.saturating_sub(1) {
            if beams.is_empty() || Self::beam_search_converged(&finished_beams, &beams, beam_size) {
                break;
            }
            if out_of_time(deadline) {
                return Err(KanjiError::Timeout);
            }

            // Decode one token per beam: the token chosen last step, appended
            // to that beam's cached prefix. Batch row `i` is beam slot `i`, so
            // the logits row index below is the slot index.
            batch.clear();
            for (slot, beam) in beams.iter().enumerate() {
                let last = *beam
                    .tokens
                    .last()
                    .expect("a live beam always has at least one token");
                let pos = (input_len + beam.tokens.len() - 1) as i32;
                batch
                    .add(last, pos, &[slot as i32], true)
                    .map_err(inference_err)?;
            }
            ctx.decode(&mut batch).map_err(inference_err)?;

            // Collect (parent slot, token, score) for every expansion — the
            // slot lets the cache follow the selection, and the token Vecs
            // are materialized only for the survivors below.
            candidates.clear();
            for (slot, beam) in beams.iter().enumerate() {
                let logits = ctx.get_logits_ith(slot as i32);
                for (token, log_prob) in self.get_top_k_tokens(logits, expand_k) {
                    candidates.push((slot, token, beam.score + log_prob));
                }
            }

            // Sort and keep top beam_size candidates
            candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
            candidates.truncate(beam_size);

            // Partition into finished and active beams
            next.clear();
            for &(parent, token, score) in &candidates {
                let mut tokens = beams[parent].tokens.clone();
                tokens.push(token);
                let candidate = BeamState { tokens, score };
                if self.is_eos_token(token, eos_token_id, model_eos) {
                    finished_beams.push(candidate);
                } else {
                    next.push((parent, candidate));
                }
            }

            let parents: Vec<usize> = next.iter().map(|(parent, _)| *parent).collect();
            Self::permute_beam_kv(&mut ctx, &parents, beam_size)?;

            beams = next.drain(..).map(|(_, beam)| beam).collect();
        }

        Ok(Self::finalize_beams(finished_beams, beams, beam_size))
    }

    /// Reference beam search: a fresh context and a full re-prefill per beam per
    /// step. This is what [`Self::generate_beam_search`] replaced; it is kept so
    /// the equivalence test can assert the fast path still selects the same
    /// candidates. Far too slow for the input path — do not call it there.
    #[cfg(test)]
    pub(crate) fn generate_beam_search_full_eval(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        beam_size: usize,
    ) -> Result<Vec<(Vec<LlamaToken>, f32)>> {
        // Same degenerate-input contract as the fast path.
        if beam_size == 0 || max_new_tokens == 0 || input_tokens.is_empty() {
            return Ok(Vec::new());
        }
        let model_eos = self.shared.model.vocab().eos();

        let initial_logits = self.eval_sequence(input_tokens)?;
        let top = self.get_top_k_tokens(&initial_logits, beam_size);
        let (mut beams, mut finished_beams) =
            self.partition_initial_beams(top, eos_token_id, model_eos);

        let expand_k = beam_size.max(4);

        for _step in 0..max_new_tokens.saturating_sub(1) {
            if beams.is_empty() || Self::beam_search_converged(&finished_beams, &beams, beam_size) {
                break;
            }

            let mut candidates: Vec<BeamState> = Vec::new();

            for beam in &beams {
                // The whole point of the reference: re-prefill the full
                // sequence in a fresh context instead of reusing a KV cache.
                let mut full_seq: Vec<LlamaToken> = input_tokens.to_vec();
                full_seq.extend(&beam.tokens);

                let logits = self.eval_sequence(&full_seq)?;
                for (token, log_prob) in self.get_top_k_tokens(&logits, expand_k) {
                    let mut new_tokens = beam.tokens.clone();
                    new_tokens.push(token);

                    candidates.push(BeamState {
                        tokens: new_tokens,
                        score: beam.score + log_prob,
                    });
                }
            }

            candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
            candidates.truncate(beam_size);

            beams.clear();
            for candidate in candidates {
                let Some(&last_token) = candidate.tokens.last() else {
                    continue;
                };
                if self.is_eos_token(last_token, eos_token_id, model_eos) {
                    finished_beams.push(candidate);
                } else {
                    beams.push(candidate);
                }
            }
        }

        Ok(Self::finalize_beams(finished_beams, beams, beam_size))
    }

    /// Add input tokens to a batch as a single sequence, requesting logits
    /// only for the last token.
    fn add_input_tokens(&self, batch: &mut LlamaBatch, tokens: &[LlamaToken]) -> Result<()> {
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(inference_err)?;
        }
        Ok(())
    }

    /// Process a token sequence and return the logits at the last position.
    ///
    /// Creates a fresh context for each call, so it costs a full prefill every
    /// time. Only the reference beam search
    /// ([`Self::generate_beam_search_full_eval`]) still works this way.
    #[cfg(test)]
    fn eval_sequence(&self, tokens: &[LlamaToken]) -> Result<Vec<f32>> {
        let mut ctx = self.new_context(self.context_params())?;

        let mut batch = LlamaBatch::new(512, 1);
        self.add_input_tokens(&mut batch, tokens)?;
        ctx.decode(&mut batch).map_err(inference_err)?;

        Ok(ctx.get_logits().to_vec())
    }

    /// Top-k tokens with their log probabilities, best first.
    fn get_top_k_tokens(&self, logits: &[f32], k: usize) -> Vec<(LlamaToken, f32)> {
        if k == 0 || logits.is_empty() {
            return Vec::new();
        }
        // log-softmax normalizer. Subtracting a constant never changes the
        // ranking, so top-k selection runs on the raw logits and only the k
        // winners are normalized.
        let log_sum_exp = log_sum_exp(logits);
        top_k_logits(logits, k)
            .into_iter()
            .map(|(i, x)| (LlamaToken(i as i32), x - log_sum_exp))
            .collect()
    }

    /// Seed beams from the prompt's top-k tokens, routing tokens that are
    /// already EOS into the finished set. Returns (active, finished).
    fn partition_initial_beams(
        &self,
        top: Vec<(LlamaToken, f32)>,
        eos_token_id: Option<i32>,
        model_eos: LlamaToken,
    ) -> (Vec<BeamState>, Vec<BeamState>) {
        let mut active = Vec::with_capacity(top.len());
        let mut finished = Vec::new();
        for (token, log_prob) in top {
            let beam = BeamState {
                tokens: vec![token],
                score: log_prob,
            };
            if self.is_eos_token(token, eos_token_id, model_eos) {
                finished.push(beam);
            } else {
                active.push(beam);
            }
        }
        (active, finished)
    }

    /// Whether the search can stop: enough beams have finished and none of
    /// the active ones can still outscore the best finished beam (scores only
    /// decrease as tokens are appended).
    fn beam_search_converged(
        finished: &[BeamState],
        active: &[BeamState],
        beam_size: usize,
    ) -> bool {
        if finished.len() < beam_size {
            return false;
        }
        let best = |beams: &[BeamState]| {
            beams
                .iter()
                .map(|b| b.score)
                .fold(f32::NEG_INFINITY, f32::max)
        };
        best(active) < best(finished)
    }

    /// Final candidate list: every beam, best score first, capped at
    /// `beam_size`.
    fn finalize_beams(
        finished: Vec<BeamState>,
        active: Vec<BeamState>,
        beam_size: usize,
    ) -> Vec<(Vec<LlamaToken>, f32)> {
        let mut all: Vec<(Vec<LlamaToken>, f32)> = finished
            .into_iter()
            .chain(active)
            .map(|b| (b.tokens, b.score))
            .collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1));
        all.truncate(beam_size);
        all
    }

    /// Move each surviving beam's KV prefix into its new slot. Several beams
    /// can descend from one parent and a parent slot is itself a destination,
    /// so the copies stage through scratch slots (`beam_size..`) instead of
    /// writing straight into the live slots and clobbering a prefix still in
    /// use. Clearing every live slot in between also reclaims the cells of
    /// beams that just finished.
    fn permute_beam_kv(ctx: &mut LlamaContext, parents: &[usize], beam_size: usize) -> Result<()> {
        for (j, &parent) in parents.iter().enumerate() {
            let scratch = (beam_size + j) as i32;
            ctx.clear_kv_cache_seq(Some(scratch as u32), None, None)
                .map_err(inference_err)?;
            ctx.copy_kv_cache_seq(parent as i32, scratch, None, None)
                .map_err(inference_err)?;
        }
        for slot in 0..beam_size {
            ctx.clear_kv_cache_seq(Some(slot as u32), None, None)
                .map_err(inference_err)?;
        }
        for j in 0..parents.len() {
            let scratch = (beam_size + j) as i32;
            ctx.copy_kv_cache_seq(scratch, j as i32, None, None)
                .map_err(inference_err)?;
            ctx.clear_kv_cache_seq(Some(scratch as u32), None, None)
                .map_err(inference_err)?;
        }
        Ok(())
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
            || self.shared.model.vocab().is_eog(token)
    }

    /// Generate tokens with a custom sampler
    fn generate_with_sampler(
        &self,
        input_tokens: &[LlamaToken],
        max_new_tokens: usize,
        eos_token_id: Option<i32>,
        mut sampler: LlamaSampler,
        deadline: Option<Instant>,
    ) -> Result<Vec<LlamaToken>> {
        // Size the batch to the prompt instead of inheriting llama.cpp's
        // defaults (n_batch 2048 / n_ubatch 512). The compute buffers are
        // allocated from those numbers on every `new_context`, and a
        // conversion only ever decodes the prompt plus one token at a time —
        // paying for a 512-token ubatch made each conversion allocate (and
        // zero) an order of magnitude more scratch memory than it uses.
        let batch_cap = input_tokens.len().max(1) + BATCH_HEADROOM;
        let mut ctx = self.new_context(
            self.context_params()
                .with_n_batch(batch_cap as u32)
                .with_n_ubatch(batch_cap as u32),
        )?;

        let mut batch = LlamaBatch::new(batch_cap, 1);
        let mut generated = input_tokens.to_vec();

        // Process input tokens
        self.add_input_tokens(&mut batch, input_tokens)?;

        ctx.decode(&mut batch).map_err(inference_err)?;

        // Get model's EOS token for comparison
        let model_eos = self.shared.model.vocab().eos();

        // Generate new tokens
        for n_cur in (input_tokens.len()..).take(max_new_tokens) {
            let new_token = sampler.sample(&ctx, -1);

            // Check for EOS
            if self.is_eos_token(new_token, eos_token_id, model_eos) {
                break;
            }

            if out_of_time(deadline) {
                return Err(KanjiError::Timeout);
            }
            generated.push(new_token);

            // Prepare next batch with just the new token
            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(inference_err)?;

            ctx.decode(&mut batch).map_err(inference_err)?;
        }

        Ok(generated)
    }

    /// Get the EOS token ID from the model
    pub fn eos_token_id(&self) -> LlamaToken {
        self.shared.model.vocab().eos()
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
        // Via the model's own params builder so the configured n_threads
        // applies here too.
        let ctx = model.new_context(model.context_params_with_n_ctx(n_ctx))?;
        let vocab_size = model.shared.model.n_vocab() as usize;

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
        let prompt = super::build_jinen_prompt(reading_katakana, "");
        let full_text = format!("{}{}", prompt, crate::kana::normalize_nfkc(surface));

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
            .map_err(inference_err)?;

        self.ctx.decode(&mut batch).map_err(inference_err)?;

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

#[cfg(test)]
mod missing_file_tests {
    use super::*;

    #[test]
    fn missing_model_file_is_err_not_panic() {
        let result = LlamaCppModel::from_file("/nonexistent/model.gguf", "/nonexistent/tok.json");
        assert!(matches!(result, Err(KanjiError::ModelNotFound(_))));
    }
}

#[cfg(test)]
mod byte_fallback_token_tests {
    use super::is_byte_fallback_token;

    #[test]
    fn byte_fallback_tokens_are_recognized() {
        assert!(is_byte_fallback_token("<0x00>"));
        assert!(is_byte_fallback_token("<0xCE>"));
        assert!(is_byte_fallback_token("<0xff>"));
    }

    #[test]
    fn control_tokens_are_not_byte_fallback() {
        assert!(!is_byte_fallback_token("<s>"));
        assert!(!is_byte_fallback_token("</s>"));
        assert!(!is_byte_fallback_token("<unk>"));
        assert!(!is_byte_fallback_token("<pad>"));
        assert!(!is_byte_fallback_token("<0xGG>"));
        assert!(!is_byte_fallback_token("<0x123>"));
        assert!(!is_byte_fallback_token("0xCE"));
    }
}

#[cfg(test)]
mod beam_search_tests {
    use super::*;
    use crate::kanji::{ModelSource, build_jinen_prompt};

    /// Load a real model, or `None` when it isn't available locally (the
    /// tests are skipped rather than failing offline).
    fn load_model() -> Option<LlamaCppModel> {
        let source = ModelSource::HuggingFace {
            repo: "togatogah/jinen-v2-small.gguf".to_string(),
            filename: "jinen-v2-small-Q5_K_M.gguf".to_string(),
        };
        let (path, tok_path) = source.resolve().ok()?;
        LlamaCppModel::from_file(&path, &tok_path).ok()
    }

    fn tokens_for(model: &LlamaCppModel, katakana: &str) -> Vec<LlamaToken> {
        let prompt = build_jinen_prompt(katakana, "");
        model.tokenize(&prompt).expect("tokenize failed")
    }

    /// The KV-cache-reusing beam search must pick the same candidates as the
    /// reference that re-prefills a fresh context for every beam at every step.
    /// This is the guard that makes the optimization a pure speedup: if the
    /// cache bookkeeping (slot assignment, parent permutation, positions) were
    /// wrong, the beams would attend to the wrong prefix and diverge here.
    #[test]
    fn matches_full_eval_reference() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);

        for katakana in ["ヘンカン", "カンジ", "キョウハイイテンキデスネ", "コウエン"]
        {
            let tokens = tokens_for(&model, katakana);
            for beam_size in [1usize, 2, 3, 5] {
                let fast = model
                    .generate_beam_search(&tokens, 20, eos, beam_size)
                    .expect("fast beam search failed");
                let reference = model
                    .generate_beam_search_full_eval(&tokens, 20, eos, beam_size)
                    .expect("reference beam search failed");

                let decode = |results: &[(Vec<LlamaToken>, f32)]| -> Vec<String> {
                    results
                        .iter()
                        .map(|(t, _)| model.decode(t, true).unwrap_or_default())
                        .collect()
                };
                assert_eq!(
                    decode(&fast),
                    decode(&reference),
                    "candidates diverged for {katakana} at beam_size={beam_size}"
                );
            }
        }
    }

    /// A beam wider than the surviving beam count, and a beam of 1, both used
    /// to sit on `max_new_tokens - 1` underflow / empty-slot edges.
    #[test]
    fn handles_edge_case_widths() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);
        let tokens = tokens_for(&model, "ヘンカン");

        assert!(
            model
                .generate_beam_search(&tokens, 0, eos, 3)
                .expect("max_new_tokens=0 must not panic")
                .is_empty(),
            "max_new_tokens=0 must generate nothing"
        );
        assert!(
            !model
                .generate_beam_search(&tokens, 20, eos, 9)
                .expect("wide beam failed")
                .is_empty()
        );
        assert!(
            model
                .generate_beam_search(&[], 20, eos, 3)
                .expect("empty input must not panic")
                .is_empty()
        );
        // beam_size is clamped to 128: 2*200 sequence slots would exceed
        // llama.cpp's LLAMA_MAX_SEQ (256) and abort across the FFI boundary.
        assert!(
            !model
                .generate_beam_search(&tokens, 3, eos, 200)
                .expect("oversized beam must be clamped, not abort")
                .is_empty()
        );
    }
}

#[cfg(test)]
mod log_sum_exp_tests {
    use super::log_sum_exp;

    /// A logit row shaped like the model's: one clear winner, the rest
    /// spread over some thirty units below it.
    fn logits(len: usize) -> Vec<f32> {
        let mut state = 0x2545_f491u32;
        (0..len)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let spread = (state % 30_000) as f32 / 1000.0;
                if i == len / 3 { 12.5 } else { -17.0 - spread }
            })
            .collect()
    }

    #[test]
    fn log_sum_exp_matches_the_f64_sum() {
        // Short rows and a vocabulary-sized one.
        for len in [1, 7, 8, 9, 31, 32_003] {
            let row = logits(len);
            let exact = row.iter().map(|&x| f64::from(x).exp()).sum::<f64>().ln();
            let got = f64::from(log_sum_exp(&row));
            assert!((got - exact).abs() < 1e-5, "len {len}: {got} vs {exact}");
        }
    }

    #[test]
    fn argmax_token_is_the_first_maximum() {
        use super::{LlamaToken, argmax_token};
        let mut row = logits(32_003);
        assert_eq!(argmax_token(&row), LlamaToken(32_003 / 3));
        // A later copy of the maximum does not take over.
        row[20_000] = 12.5;
        assert_eq!(argmax_token(&row), LlamaToken(32_003 / 3));
        row[5] = 12.5;
        assert_eq!(argmax_token(&row), LlamaToken(5));
    }

    #[test]
    fn top_k_logits_matches_a_stable_sort() {
        use super::top_k_logits;
        let mut row = logits(1000);
        // Ties, inside one block and across blocks.
        row[40] = 12.5;
        row[41] = -3.0;
        row[700] = -3.0;
        row[999] = 12.5;
        for len in [0, 1, 15, 16, 17, 1000] {
            let row = &row[..len];
            let mut sorted: Vec<(usize, f32)> = row.iter().copied().enumerate().collect();
            sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
            for k in [0, 1, 3, 9, 40, 2000] {
                let expected: Vec<(usize, f32)> = sorted.iter().copied().take(k).collect();
                assert_eq!(top_k_logits(row, k), expected, "len {len} k {k}");
            }
        }
    }

    #[test]
    fn log_sum_exp_of_equal_logits_is_ln_of_their_count() {
        let got = log_sum_exp(&[3.0; 16]);
        assert!((got - (3.0 + 16f32.ln())).abs() < 1e-5, "{got}");
    }
}

#[cfg(test)]
mod kv_reuse_tests {
    use super::*;
    use crate::kanji::ModelSource;
    use crate::kanji::build_jinen_prompt;

    #[test]
    fn common_prefix_len_basics() {
        let t = |v: &[i32]| v.iter().map(|&x| LlamaToken(x)).collect::<Vec<_>>();
        assert_eq!(common_prefix_len(&t(&[1, 2, 3]), &t(&[1, 2, 4])), 2);
        assert_eq!(common_prefix_len(&t(&[]), &t(&[1])), 0);
        assert_eq!(common_prefix_len(&t(&[1, 2]), &t(&[1, 2])), 2);
        assert_eq!(common_prefix_len(&t(&[1, 2, 3]), &t(&[1, 2])), 2);
        assert_eq!(common_prefix_len(&t(&[9]), &t(&[1, 2])), 0);
    }

    /// Load the default registry model, or `None` when it isn't available
    /// locally (the tests are skipped rather than failing offline).
    fn load_model() -> Option<LlamaCppModel> {
        let (path, tok_path) = ModelSource::HuggingFace {
            repo: "togatogah/jinen-v2-small.gguf".into(),
            filename: "jinen-v2-small-Q5_K_M.gguf".into(),
        }
        .resolve()
        .ok()?;
        // Unshared: these tests count the session's decode calls, and the
        // shared one is also used by whatever other test runs alongside.
        LlamaCppModel::from_file_unshared(&path, &tok_path).ok()
    }

    /// Logit distance under which two tokens count as tied. Batched and
    /// single-token evaluation round differently in llama.cpp's quantized
    /// kernels, which moves a logit by a few hundredths; a wrong token is
    /// off by whole units.
    const NEAR_TIE: f32 = 0.5;

    /// The session's output must be the fresh context's, except where it
    /// took the other side of a tie: at the first token they disagree on,
    /// the reference logits must score the two picks within [`NEAR_TIE`].
    fn assert_same_up_to_ties(
        model: &LlamaCppModel,
        eos: Option<i32>,
        session: &[LlamaToken],
        fresh: &[LlamaToken],
        label: &str,
    ) {
        if session == fresh {
            return;
        }
        let at = common_prefix_len(session, fresh);
        let logits = model
            .eval_sequence(&fresh[..at])
            .expect("reference eval failed");
        let model_eos = model.shared.model.vocab().eos();
        // An output that ended here picked the best end-of-sequence token.
        let end_logit = (0..logits.len())
            .filter(|&id| model.is_eos_token(LlamaToken(id as i32), eos, model_eos))
            .map(|id| logits[id])
            .fold(f32::NEG_INFINITY, f32::max);
        let picked = |tokens: &[LlamaToken]| {
            tokens
                .get(at)
                .map_or(end_logit, |token| logits[token.0 as usize])
        };
        let distance = (picked(session) - picked(fresh)).abs();
        assert!(
            distance < NEAR_TIE,
            "{label}: diverged at token {at} by {distance} logits, not a tie"
        );
    }

    /// Generate through the session and through a fresh context, and hold
    /// the two against each other.
    fn generate_checked(
        model: &LlamaCppModel,
        eos: Option<i32>,
        katakana: &str,
        context: &str,
    ) -> Vec<LlamaToken> {
        let prompt = build_jinen_prompt(katakana, context);
        let tokens = model.tokenize(&prompt).expect("tokenize failed");
        let session = model.generate(&tokens, 20, eos).expect("generate failed");
        let fresh = model
            .generate_with_sampler(&tokens, 20, eos, LlamaSampler::greedy(), None)
            .expect("fresh generate failed");
        let label = format!("「{katakana}」 ctx 「{context}」");
        assert_same_up_to_ties(model, eos, &session, &fresh, &label);
        session[tokens.len()..].to_vec()
    }

    fn decodes(model: &LlamaCppModel) -> usize {
        model
            .lock_greedy_session()
            .as_ref()
            .map_or(0, |session| session.decodes)
    }

    /// The session must produce what a fresh context produces, over the
    /// call sequences typing actually generates: prompts growing one kana
    /// at a time, backspace to a shorter prefix, an exact repeat, a switch
    /// to an unrelated prompt, and a left-context change (which flips the
    /// very front of the prompt).
    #[test]
    fn matches_fresh_context_across_call_sequences() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);

        let cases: [(&str, &str); 9] = [
            ("ワ", ""),
            ("ワタ", ""),
            ("ワタシ", ""),
            ("ワタシハ", ""),
            ("ワタシハガクセイ", ""),
            ("ワタシ", ""),             // backspace
            ("ワタシ", ""),             // repeat
            ("キョウハイイテンキ", ""), // unrelated switch
            ("ハシ", "箸を持つ。"),     // context change
        ];
        for (katakana, context) in cases {
            generate_checked(&model, eos, katakana, context);
        }
    }

    /// A sentence typed one kana at a time, then deleted the same way: every
    /// step offers the previous output as a draft, and whether the model
    /// keeps all of it, part of it or none, the result is still its own.
    #[test]
    fn matches_fresh_context_while_typing_a_sentence() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);

        let typed: Vec<char> = "キョウハテンキガイイノデコウエンヲサンポシタ"
            .chars()
            .collect();
        let grow = 1..=typed.len();
        let shrink = (typed.len() - 6..typed.len()).rev();
        for len in grow.chain(shrink) {
            let katakana: String = typed[..len].iter().collect();
            generate_checked(&model, eos, &katakana, "昨日は雨でしたが、");
        }
    }

    /// The point of the draft: a reading extended by one kana is converted
    /// in fewer decode calls than it has output tokens, where generating
    /// token by token takes one call per token and one for the prompt.
    #[test]
    fn a_draft_saves_decode_calls() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);

        generate_checked(&model, eos, "キョウハテンキガイイノデコウエンヲサンポ", "");
        let before = decodes(&model);
        let output = generate_checked(
            &model,
            eos,
            "キョウハテンキガイイノデコウエンヲサンポシ",
            "",
        );
        let calls = decodes(&model) - before;
        assert!(
            calls < output.len(),
            "{calls} decode calls for {} output tokens",
            output.len()
        );
    }

    /// An unrelated prompt must not be handed the previous output: it would
    /// be decoded only to be rejected.
    #[test]
    fn an_unrelated_prompt_gets_no_draft() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);

        generate_checked(
            &model,
            eos,
            "キョウハテンキガイイノデコウエンヲサンポシタ",
            "",
        );
        let before = decodes(&model);
        let output = generate_checked(&model, eos, "ワタシハガクセイデス", "");
        // Token by token: one call for the prompt, one per output token.
        assert_eq!(decodes(&model) - before, output.len() + 1);
    }

    /// A conversion that runs out of time is abandoned, not answered late —
    /// and the session survives it: the next call, given the time, returns
    /// what a fresh context returns.
    #[test]
    fn a_passed_deadline_abandons_the_generation_and_keeps_the_session() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);
        let katakana = "キョウハテンキガイイノデコウエンヲサンポシタ";
        let tokens = model
            .tokenize(&build_jinen_prompt(katakana, ""))
            .expect("tokenize failed");

        // Already passed: the prompt is decoded, then the clock is read.
        let abandoned = model.generate_within(&tokens, 20, eos, Some(Instant::now()));
        assert!(
            matches!(abandoned, Err(KanjiError::Timeout)),
            "{abandoned:?}"
        );
        assert!(
            model.lock_greedy_session().is_some(),
            "a timeout must not discard the session"
        );

        let before = decodes(&model);
        let output = generate_checked(&model, eos, katakana, "");
        // The prompt was decoded by the abandoned call: only its last token
        // is decoded again, in the same call as the first output token.
        assert_eq!(decodes(&model) - before, output.len() + 1);

        let in_time = Instant::now() + std::time::Duration::from_secs(60);
        let unhurried = model
            .generate_within(&tokens, 20, eos, Some(in_time))
            .expect("generate failed");
        assert_eq!(unhurried[tokens.len()..], output[..]);
    }

    #[test]
    fn a_passed_deadline_abandons_the_beam_search() {
        let Some(model) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let eos = Some(model.eos_token_id().0);
        let tokens = model
            .tokenize(&build_jinen_prompt("キョウハテンキガイイノデ", ""))
            .expect("tokenize failed");
        let abandoned =
            model.generate_beam_search_within(&tokens, 20, eos, 3, Some(Instant::now()));
        assert!(matches!(abandoned, Err(KanjiError::Timeout)));
    }

    /// Engines that load the same files get the same model and the same
    /// session, not a copy each.
    #[test]
    fn wrappers_of_the_same_files_share_the_model_and_its_session() {
        let source = ModelSource::HuggingFace {
            repo: "togatogah/jinen-v2-small.gguf".into(),
            filename: "jinen-v2-small-Q5_K_M.gguf".into(),
        };
        let Ok((path, tok_path)) = source.resolve() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let first = LlamaCppModel::from_file(&path, &tok_path).expect("load failed");
        let second = LlamaCppModel::from_file(&path, &tok_path).expect("load failed");
        assert!(Arc::ptr_eq(&first.shared, &second.shared));
    }

    /// The shared session is built for one wrapper's settings. Another
    /// wrapper with different ones rebuilds it instead of running on a
    /// context sized or threaded for somebody else.
    #[test]
    fn a_wrapper_with_other_settings_rebuilds_the_shared_session() {
        let Some(first) = load_model() else {
            eprintln!("model unavailable, skipping");
            return;
        };
        let mut second = LlamaCppModel::wrap(first.shared.clone(), 256);
        second.set_n_threads(2);
        let eos = Some(first.eos_token_id().0);

        generate_checked(&first, eos, "キョウハテンキガイイ", "");
        let built_for = |model: &LlamaCppModel| {
            model
                .lock_greedy_session()
                .as_ref()
                .map(|session| session.n_threads)
        };
        assert_eq!(built_for(&first), Some(0));

        generate_checked(&second, eos, "キョウハテンキガイイノデ", "");
        assert_eq!(built_for(&second), Some(2));
        // And back: each wrapper gets the session it asked for.
        generate_checked(&first, eos, "キョウハテンキガイイノデコウエン", "");
        assert_eq!(built_for(&first), Some(0));
    }

    #[test]
    fn argmax_token_prefers_the_lower_id_on_a_tie() {
        assert_eq!(argmax_token(&[0.5, 2.0, 2.0, -1.0]), LlamaToken(1));
        assert_eq!(argmax_token(&[3.0]), LlamaToken(0));
    }
}
