//! karukan-imserver: stdio JSON-RPC engine server for the macOS frontend.
//!
//! Reads newline-delimited JSON-RPC 2.0 requests from stdin and writes one
//! response per line to stdout. Logs go to stderr (`RUST_LOG` controls the
//! filter; defaults to `info`). The learning cache is saved on EOF, so the
//! frontend should close the child's stdin (or send `save_learning`) before
//! terminating it.
//!
//! `--prefetch-models` downloads every conversion model defined in the
//! config's `[models]` table into the HuggingFace cache and exits (used by
//! `make install` to avoid a multi-minute download on first launch).

use std::io::{BufRead, Write};

use karukan_im::config::Settings;
use karukan_im::server::ImServer;

/// Warm the HF cache for every `[models]` entry; local-path entries are
/// only checked for existence.
fn prefetch_models() -> i32 {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(e) => {
            tracing::error!("failed to load settings: {e:#}");
            return 1;
        }
    };
    let mut failed = false;
    for key in settings.models.keys() {
        let resolved = settings
            .model_source(key)
            .and_then(|source| source.resolve().map_err(anyhow::Error::from));
        match resolved {
            Ok((gguf, tokenizer)) => tracing::info!(
                "Model '{}' ready: {} (tokenizer: {})",
                key,
                gguf.display(),
                tokenizer.display()
            ),
            Err(e) => {
                tracing::error!("model '{}' prefetch failed: {e:#}", key);
                failed = true;
            }
        }
    }
    i32::from(failed)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    if std::env::args().any(|arg| arg == "--prefetch-models") {
        std::process::exit(prefetch_models());
    }

    let mut server = ImServer::new();
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();

    tracing::info!("karukan-imserver started (pid={})", std::process::id());

    for line in stdin.lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                tracing::error!("stdin read error: {e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_line(&line)
            && writeln!(stdout, "{response}")
                .and_then(|_| stdout.flush())
                .is_err()
        {
            // stdout closed: frontend is gone
            break;
        }
    }

    tracing::info!("stdin closed, saving learning cache and exiting");
    server.save_learning();
}
