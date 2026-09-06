//! HuggingFace model download utilities
//!
//! Downloads GGUF models from HuggingFace Hub and caches them locally.
//!
//! Resolution is cache-first: a file already in the HuggingFace cache is
//! served from disk without any network request, so a cached model resolves
//! instantly whether or not the machine is online. The network is only
//! touched on a cache miss. The flip side is that an update pushed to the
//! same repo/filename is not picked up once the file is cached — model
//! updates must change the filename (or the user clears the HF cache).

use super::error::KanjiError;
type Result<T> = super::error::Result<T>;
use hf_hub::{HFClient, HFError, split_id};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Retry budget for the network path. hf-hub's default (5 retries) meant an
/// unreachable host was retried for minutes — each attempt eats a full OS
/// connect timeout (~30s), and hf-hub only falls back to its cache after the
/// whole cycle. Two retries keeps resilience against transient blips while
/// bounding the worst case; the delay between attempts is dwarfed by the
/// connect timeout anyway.
const NETWORK_MAX_RETRIES: usize = 2;
const NETWORK_RETRY_BASE_DELAY: Duration = Duration::from_millis(200);

/// Paths already resolved in this process, keyed by (repo, filename).
///
/// Resolving the same file twice is not free, and not even safe to do
/// concurrently: unless the revision is a commit hash, hf-hub takes the
/// network path on every call and rebuilds the snapshot symlink by removing
/// it first, so the file briefly does not exist. A second thread that
/// resolved the path just before can then be handed a path that vanishes
/// under it — which is what made parallel test runs fail at random inside
/// `LlamaModel::load_from_file`. Resolving once per process closes the
/// window, and drops a HEAD request from every model load.
fn resolved_paths() -> &'static Mutex<HashMap<(String, String), PathBuf>> {
    static PATHS: OnceLock<Mutex<HashMap<(String, String), PathBuf>>> = OnceLock::new();
    PATHS.get_or_init(Mutex::default)
}

/// Download a GGUF model from HuggingFace Hub
///
/// Returns the local path to the downloaded file.
/// The file is cached in the HuggingFace cache directory (~/.cache/huggingface/hub/).
///
/// # Arguments
/// * `repo_id` - HuggingFace repository ID
/// * `filename` - Filename to download
///
/// # Environment Variables
/// * `HF_TOKEN` - HuggingFace API token (required for private repositories)
pub fn download_gguf(repo_id: &str, filename: &str) -> Result<PathBuf> {
    let key = (repo_id.to_string(), filename.to_string());
    // Held across the download so a second caller waits for the result
    // instead of racing hf-hub for the same file. Only successes are
    // remembered: a transient failure must not stick for the process.
    let mut resolved = resolved_paths().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = resolved.get(&key) {
        return Ok(path.clone());
    }

    // The builder resolves HF_TOKEN (env var or cached login) itself.
    let client = HFClient::builder()
        .retry_max_attempts(NETWORK_MAX_RETRIES)
        .retry_base_delay(NETWORK_RETRY_BASE_DELAY)
        .build_sync()
        .map_err(|e| KanjiError::Download(e.into()))?;

    let (owner, name) = split_id(repo_id);
    let repo = client.model(owner, name);

    // Cache first, never the network. With the default revision (`main`)
    // hf-hub would otherwise revalidate with a HEAD request on every
    // resolve, and offline that means waiting out connect timeout × retries
    // per file before it falls back to this same cache — minutes during
    // which an IME is unusable.
    match repo
        .download_file()
        .filename(filename)
        .local_files_only(true)
        .send()
    {
        Ok(path) => {
            tracing::debug!("Resolved {} from local cache: {:?}", filename, path);
            resolved.insert(key, path.clone());
            return Ok(path);
        }
        // Not cached: fall through to the network.
        Err(HFError::LocalEntryNotFound { .. }) => {}
        // A broken cache entry shouldn't be fatal either — the network path
        // rebuilds it.
        Err(e) => tracing::warn!("cache lookup failed for {}/{}: {}", repo_id, filename, e),
    }

    tracing::info!("Downloading {} from {}...", filename, repo_id);

    let path = repo
        .download_file()
        .filename(filename)
        .send()
        .map_err(|e| KanjiError::Download(e.into()))?;

    tracing::info!("Downloaded to {:?}", path);

    resolved.insert(key, path.clone());
    Ok(path)
}
