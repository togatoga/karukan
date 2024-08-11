//! Benchmarks for kanji conversion inference speed
//!
//! Run with: cargo bench
//!
//! Note: These benchmarks require downloading models from HuggingFace.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use karukan_engine::kanji::{
    LlamaCppModel, build_jinen_prompt, get_path_by_id, get_tokenizer_path_by_id, registry,
};

// ============================================================================
// llama.cpp backend benchmarks
// ============================================================================

fn bench_llamacpp(c: &mut Criterion) {
    let reg = registry();
    let default_id = &reg.default_model;
    let (_family, _) = match reg.default_variant() {
        Some(v) => v,
        None => {
            eprintln!("Skipping llama.cpp benchmarks: no default model configured");
            return;
        }
    };

    let model = match get_path_by_id(default_id).ok().and_then(|p| {
        let tok = get_tokenizer_path_by_id(default_id).ok()?;
        LlamaCppModel::from_file(&p, &tok).ok()
    }) {
        Some(m) => m,
        None => {
            eprintln!("Skipping llama.cpp benchmarks: model not available");
            return;
        }
    };

    let mut group = c.benchmark_group("llamacpp");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    // Short input - greedy
    group.bench_function("short_greedy", |b| {
        let prompt = build_jinen_prompt("コンニチハ", "");
        let tokens = model.tokenize(&prompt).unwrap();
        b.iter(|| {
            model
                .generate(black_box(&tokens), 20, Some(model.eos_token_id().0))
                .unwrap()
        })
    });

    // Medium input - greedy
    group.bench_function("medium_greedy", |b| {
        let prompt = build_jinen_prompt("キョウハイイテンキデス", "");
        let tokens = model.tokenize(&prompt).unwrap();
        b.iter(|| {
            model
                .generate(black_box(&tokens), 20, Some(model.eos_token_id().0))
                .unwrap()
        })
    });

    // With context
    group.bench_function("with_context", |b| {
        let prompt = build_jinen_prompt("ヘンカン", "日本語の漢字");
        let tokens = model.tokenize(&prompt).unwrap();
        b.iter(|| {
            model
                .generate(black_box(&tokens), 20, Some(model.eos_token_id().0))
                .unwrap()
        })
    });

    // Beam search k=1
    group.bench_function("beam_k1", |b| {
        let prompt = build_jinen_prompt("ヘンカン", "");
        let tokens = model.tokenize(&prompt).unwrap();
        let eos = Some(model.eos_token_id().0);
        b.iter(|| {
            model
                .generate_beam_search(black_box(&tokens), 20, eos, 1)
                .unwrap()
        })
    });

    // Beam search k=3
    group.bench_function("beam_k3", |b| {
        let prompt = build_jinen_prompt("ヘンカン", "");
        let tokens = model.tokenize(&prompt).unwrap();
        let eos = Some(model.eos_token_id().0);
        b.iter(|| {
            model
                .generate_beam_search(black_box(&tokens), 20, eos, 3)
                .unwrap()
        })
    });

    // Beam search k=5
    group.bench_function("beam_k5", |b| {
        let prompt = build_jinen_prompt("ヘンカン", "");
        let tokens = model.tokenize(&prompt).unwrap();
        let eos = Some(model.eos_token_id().0);
        b.iter(|| {
            model
                .generate_beam_search(black_box(&tokens), 20, eos, 5)
                .unwrap()
        })
    });

    // Beam search with longer input
    group.bench_function("beam_k3_long", |b| {
        let prompt = build_jinen_prompt("ジユウガオカ", "");
        let tokens = model.tokenize(&prompt).unwrap();
        let eos = Some(model.eos_token_id().0);
        b.iter(|| {
            model
                .generate_beam_search(black_box(&tokens), 20, eos, 3)
                .unwrap()
        })
    });

    // Beam search with context
    group.bench_function("beam_k3_ctx", |b| {
        let prompt = build_jinen_prompt("シカイ", "歯が痛いので");
        let tokens = model.tokenize(&prompt).unwrap();
        let eos = Some(model.eos_token_id().0);
        b.iter(|| {
            model
                .generate_beam_search(black_box(&tokens), 20, eos, 3)
                .unwrap()
        })
    });

    group.finish();
}

criterion_group!(benches, bench_llamacpp);
criterion_main!(benches);
