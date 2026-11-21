//! karukan-imserver: stdio JSON-RPC engine server for the macOS frontend.
//!
//! Reads newline-delimited JSON-RPC 2.0 requests from stdin and writes one
//! response per line to stdout. Logs go to stderr (`RUST_LOG` controls the
//! filter; defaults to `info`). The learning cache is saved on EOF, so the
//! frontend should close the child's stdin (or send `save_learning`) before
//! terminating it.
//!
//! `--version` prints the build (`karukan-imserver 0.1.0+g1396bb2`) and
//! exits. `--prefetch-models` downloads every conversion model defined in the
//! config's `[models]` table into the HuggingFace cache and exits (used by
//! `make install` to avoid a multi-minute download on first launch).

use std::io::{BufRead, Write};

use anyhow::Context;
use karukan_im::config::Settings;
use karukan_im::server::ImServer;

/// Warm the HF cache for every `[models]` entry; local-path entries are
/// only checked for existence. Stops at the first failure: the next one
/// would only repeat the cause after another network timeout.
fn prefetch_models() -> anyhow::Result<()> {
    let settings = Settings::load()?;
    for key in settings.models.keys() {
        let (gguf, tokenizer) = settings
            .model_source(key)?
            .resolve()
            .with_context(|| format!("model '{key}'"))?;
        tracing::info!(
            "Model '{}' ready: {} (tokenizer: {})",
            key,
            gguf.display(),
            tokenizer.display()
        );
    }
    Ok(())
}

fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("karukan-imserver {}", karukan_engine::VERSION);
        return;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    if std::env::args().any(|arg| arg == "--prefetch-models") {
        if let Err(e) = prefetch_models() {
            tracing::error!("model prefetch failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let mut server = ImServer::new();
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();

    tracing::info!(
        "karukan-imserver {} started (pid={})",
        karukan_engine::VERSION,
        std::process::id()
    );

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
