//! Sudachi CSV → scored JSON dictionary builder
//!
//! Parses Sudachi dictionary CSV files, optionally scores (reading, surface)
//! pairs using a jinen model's NLL, and outputs JSON compatible with `dict-build`.
//!
//! Scoring uses thread parallelism: each thread gets its own NllScorer (own
//! LlamaContext) and processes a chunk of pairs sequentially. The model weights
//! are shared read-only across threads.

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use karukan_engine::dict::parse_sudachi_csvs;
use karukan_engine::kana::hiragana_to_katakana;
use karukan_engine::kanji::{
    KanjiError, LlamaCppModel, NllScorer, get_path_by_id, get_tokenizer_path_by_id, registry,
};
use rayon::prelude::*;
use serde::Serialize;
use std::path::PathBuf;

/// Build a scored JSON dictionary from Sudachi CSV files.
///
/// Parses one or more Sudachi CSV files, groups entries by reading,
/// optionally scores each (reading, surface) pair using a jinen model,
/// and outputs JSON compatible with `dict-build`.
#[derive(Parser)]
#[command(name = "sudachi-dict")]
#[command(about = "Build scored JSON dictionary from Sudachi CSV files")]
struct Cli {
    /// Input Sudachi CSV files
    #[arg(required = true)]
    csv_files: Vec<PathBuf>,

    /// Model variant id (e.g. jinen-v1-xsmall-q5) or path to GGUF file
    #[arg(long, default_value = "jinen-v1-xsmall-q5")]
    model: String,

    /// Path to tokenizer.json (required when --model is a GGUF file path)
    #[arg(long)]
    tokenizer_json: Option<PathBuf>,

    /// Output JSON file
    #[arg(short, long, default_value = "scored.json")]
    output: PathBuf,

    /// Score candidates using a jinen model (default: use Sudachi cost directly)
    #[arg(long)]
    model_scores: bool,

    /// Number of parallel scoring threads (default: half of CPU count)
    #[arg(long)]
    threads: Option<usize>,

    /// Context window size for the model
    #[arg(long, default_value_t = 256)]
    n_ctx: u32,
}

/// A flattened (reading, surface) pair with its original indices for reassembly.
struct FlatPair {
    entry_idx: usize,
    surface_idx: usize,
    reading_katakana: String,
    surface: String,
    cost: i32,
}

#[derive(Serialize)]
struct JsonCandidate {
    surface: String,
    score: f32,
}

#[derive(Serialize)]
struct JsonEntry {
    reading: String,
    candidates: Vec<JsonCandidate>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configure rayon thread pool (default: half of CPU count)
    let threads = cli.threads.unwrap_or_else(|| {
        (std::thread::available_parallelism().map_or(4, |n| n.get()) / 2).max(1)
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .ok(); // ignore error if already initialized

    // Step 1: Parse CSV files
    eprintln!("Parsing {} CSV file(s)...", cli.csv_files.len());
    let reading_map =
        parse_sudachi_csvs(&cli.csv_files).context("Failed to parse Sudachi CSV files")?;

    eprintln!("Parsed {} unique readings", reading_map.len());

    // Step 2: Convert to sorted entries
    let mut entries: Vec<(String, Vec<(String, i32)>)> = reading_map
        .into_iter()
        .map(|(reading, surfaces)| {
            let mut surface_list: Vec<(String, i32)> = surfaces.into_iter().collect();
            surface_list.sort_by(|(s1, c1), (s2, c2)| c1.cmp(c2).then_with(|| s1.cmp(s2)));
            (reading, surface_list)
        })
        .collect();

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let total_pairs: usize = entries.iter().map(|(_, s)| s.len()).sum();
    eprintln!(
        "{} entries, {} total (reading, surface) pairs",
        entries.len(),
        total_pairs
    );

    // Step 3: Score with model or use Sudachi cost directly
    let json_entries: Vec<JsonEntry> = if cli.model_scores {
        score_with_model(&cli, entries, total_pairs)?
    } else {
        eprintln!("Using Sudachi cost directly (use --model-scores for model scoring)");
        entries
            .into_iter()
            .map(|(reading, surfaces)| {
                let candidates = surfaces
                    .into_iter()
                    .map(|(surface, cost)| JsonCandidate {
                        surface,
                        score: cost as f32,
                    })
                    .collect();
                JsonEntry {
                    reading,
                    candidates,
                }
            })
            .collect()
    };

    // Step 4: Write JSON output
    eprintln!(
        "Writing {} entries to {:?}...",
        json_entries.len(),
        cli.output
    );
    let json = serde_json::to_string_pretty(&json_entries)?;
    std::fs::write(&cli.output, &json)
        .with_context(|| format!("Failed to write {}", cli.output.display()))?;

    eprintln!("Done. Output: {}", cli.output.display());
    Ok(())
}

/// Score entries using thread parallelism.
///
/// Each rayon thread creates its own NllScorer (own LlamaContext) and processes
/// a chunk of pairs sequentially. Model weights are shared read-only.
fn score_with_model(
    cli: &Cli,
    entries: Vec<(String, Vec<(String, i32)>)>,
    total_pairs: usize,
) -> Result<Vec<JsonEntry>> {
    let model = load_model(&cli.model, cli.tokenizer_json.as_ref(), cli.n_ctx)?;
    let num_threads = rayon::current_num_threads();

    eprintln!("Scoring {} pairs (threads={})...", total_pairs, num_threads);

    // Flatten all (reading, surface) pairs for parallel processing
    let mut flat_pairs: Vec<FlatPair> = Vec::with_capacity(total_pairs);
    for (entry_idx, (reading, surfaces)) in entries.iter().enumerate() {
        let reading_katakana = hiragana_to_katakana(reading);
        let katakana = if reading_katakana == *reading {
            reading.clone()
        } else {
            reading_katakana
        };
        for (surface_idx, (surface, cost)) in surfaces.iter().enumerate() {
            flat_pairs.push(FlatPair {
                entry_idx,
                surface_idx,
                reading_katakana: katakana.clone(),
                surface: surface.clone(),
                cost: *cost,
            });
        }
    }

    // Progress bar (thread-safe)
    let pb = ProgressBar::new(total_pairs as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec}, ETA: {eta})")
            .unwrap()
            .progress_chars("=>-"),
    );

    // Split into ~num_threads chunks; each thread creates one NllScorer
    let chunk_size = flat_pairs.len().div_ceil(num_threads);

    let scores: Vec<f32> = flat_pairs
        .par_chunks(chunk_size.max(1))
        .flat_map_iter(|chunk| {
            let mut scorer =
                NllScorer::new(&model, cli.n_ctx).expect("Failed to create NLL scorer");

            let chunk_scores: Vec<f32> = chunk
                .iter()
                .map(|fp| {
                    let score = scorer
                        .compute_nll(&fp.reading_katakana, &fp.surface)
                        .unwrap_or(fp.cost as f32 / 1000.0);
                    pb.inc(1);
                    score
                })
                .collect();

            chunk_scores.into_iter()
        })
        .collect();

    pb.finish_with_message("done");

    // Reassemble scores back into entries
    let mut json_entries: Vec<JsonEntry> = entries
        .into_iter()
        .map(|(reading, surfaces)| {
            let candidates = surfaces
                .into_iter()
                .map(|(surface, _cost)| JsonCandidate {
                    surface,
                    score: 0.0, // placeholder, filled below
                })
                .collect();
            JsonEntry {
                reading,
                candidates,
            }
        })
        .collect();

    // Fill in scores from the flat result
    for (fp, score) in flat_pairs.iter().zip(scores.iter()) {
        json_entries[fp.entry_idx].candidates[fp.surface_idx].score = *score;
    }

    // Sort candidates by score within each entry
    for entry in &mut json_entries {
        entry.candidates.sort_by(|a, b| a.score.total_cmp(&b.score).then_with(|| a.surface.cmp(&b.surface)));
    }

    Ok(json_entries)
}

/// Load a model by variant id or GGUF file path.
fn load_model(
    model_spec: &str,
    tokenizer_json: Option<&PathBuf>,
    n_ctx: u32,
) -> Result<LlamaCppModel> {
    let path = PathBuf::from(model_spec);
    if path.exists() {
        let tok_path = tokenizer_json.ok_or_else(|| {
            anyhow::anyhow!("--tokenizer-json is required when --model is a GGUF file path")
        })?;
        eprintln!("Loading GGUF from {}...", path.display());
        let model = LlamaCppModel::from_file_with_n_ctx(&path, tok_path, n_ctx)
            .with_context(|| format!("Failed to load GGUF from {}", path.display()))?;
        return Ok(model);
    }

    let reg = registry();
    let (_family, _variant) = reg
        .find_variant(model_spec)
        .ok_or(KanjiError::UnknownVariant(model_spec.to_string()))?;

    eprintln!("Downloading/loading model variant: {} ...", model_spec);
    let gguf_path = get_path_by_id(model_spec)?;
    let tok_path = get_tokenizer_path_by_id(model_spec)?;
    eprintln!("Model path: {}", gguf_path.display());
    eprintln!("Tokenizer: {}", tok_path.display());
    Ok(LlamaCppModel::from_file_with_n_ctx(
        &gguf_path, &tok_path, n_ctx,
    )?)
}
