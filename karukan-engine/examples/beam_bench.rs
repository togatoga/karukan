//! Conversion latency bench: sweeps n_threads for greedy and beam search.
//!
//! The engine's hot paths are `ParallelBeam` (main greedy + light beam-3 in
//! parallel; wall clock is the slower of the two) and the live-conversion
//! greedy, so those are what get measured.
//!
//! Usage: cargo run --release -p karukan-engine --example beam_bench

use karukan_engine::{Backend, KanaKanjiConverter, ModelSource};
use std::time::Instant;

fn median(mut v: Vec<u128>) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn bench(conv: &KanaKanjiConverter, reading: &str, n: usize, iters: usize) -> (u128, usize) {
    // Warmup (first call touches cold caches / lazy init)
    let _ = conv.convert(reading, "", n);
    let mut count = 0;
    let mut times = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let r = conv.convert(reading, "", n).unwrap();
        times.push(t.elapsed().as_millis());
        count = r.len();
        std::hint::black_box(r);
    }
    (median(times), count)
}

fn main() {
    // 21 chars: about one chunk, the size the windowed path beams.
    let reading = "きょうはてんきがいいのでこうえんをさんぽした";
    let short = "きょうはいいてんき";
    let iters = 10;

    for (repo, filename, label) in [
        (
            "togatogah/jinen-v2-xsmall.gguf",
            "jinen-v2-xsmall-Q5_K_M.gguf",
            "light(xsmall)",
        ),
        (
            "togatogah/jinen-v2-small.gguf",
            "jinen-v2-small-Q5_K_M.gguf",
            "main(small)",
        ),
    ] {
        let source = ModelSource::Hf {
            repo: repo.to_string(),
            filename: filename.to_string(),
        };
        let backend = Backend::from_source(&source).expect("model load");
        let mut conv = KanaKanjiConverter::new(backend).expect("converter");
        println!("== {label} ==");
        for n_threads in [0u32, 1, 2, 4, 8, 16] {
            conv.set_n_threads(n_threads);
            let (beam_long, c1) = bench(&conv, reading, 3, iters);
            let (beam_short, c2) = bench(&conv, short, 3, iters);
            let (greedy_long, _) = bench(&conv, reading, 1, iters);
            println!(
                "t={n_threads:>2}: beam3(21字)={beam_long:>4}ms [{c1}cand] beam3(9字)={beam_short:>4}ms [{c2}cand] greedy(21字)={greedy_long:>4}ms"
            );
        }
    }
}
