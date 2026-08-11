# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

karukan is a Japanese Input Method system for Linux and macOS, consisting of four Rust crates and a Swift package. The IME core and its platform frontends are grouped under `karukan-im/`:

- **karukan-engine** (`karukan-engine/`): Core library — romaji-to-hiragana conversion, neural kana-kanji conversion via llama.cpp, system dictionary, learning cache, candidate rewriter (width/case/symbol variants)
- **karukan-cli** (`karukan-cli/`): CLI tools and server — dictionary builder, Sudachi converter, dict viewer, AJIMEE-Bench, HTTP API server
- **karukan-im** (`karukan-im/core/`): Shared IME engine state machine (Empty → Composing → Conversion) and `karukan-imserver` stdio JSON-RPC server (macOS binary bundled in the macOS frontend)
- **karukan-fcitx5** (`karukan-im/fcitx5/`): fcitx5 Linux frontend — C FFI (`src/ffi/`) and C++ addon (`fcitx5-addon/`) that wrap karukan-im
- **karukan-macos** (`karukan-im/macos/`): Swift/InputMethodKit frontend that spawns `karukan-imserver` as a bundled child process

## Build and Development Commands

This project uses a Cargo workspace. All commands are run from the repository root.

### Full workspace

```bash
cargo build --release       # Build all crates
cargo test --workspace      # Run all tests
```

### karukan-engine

```bash
cargo build -p karukan-engine --release
cargo test -p karukan-engine  # includes integration tests (model auto-downloaded on first run)
```

### karukan-cli

```bash
cargo build -p karukan-cli --release

# Start the server (auto-downloads models from HuggingFace)
cargo run --release --bin karukan-server

# Build dictionary from JSON or Mozc TSV
cargo run --release --bin karukan-dict -- build input.json -o dict.bin

# Build scored dictionary from Sudachi CSV
cargo run --release --bin sudachi-dict -- input.csv -o scored.json

# Dictionary viewer (web UI + CLI search)
cargo run --release --bin karukan-dict -- view dict.bin

# AJIMEE-Bench evaluation
cargo run --release --bin ajimee-bench -- evaluation_items.json
```

### karukan-im

```bash
cargo build -p karukan-im --release
cargo test -p karukan-im
```

### karukan-fcitx5

```bash
cargo build -p karukan-fcitx5 --release
cargo test -p karukan-fcitx5

# Build and install fcitx5 addon
cd karukan-im/fcitx5/fcitx5-addon

# Option A: System install (sudo required, no FCITX_ADDON_DIRS needed)
cmake -B build -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build -j
sudo cmake --install build

# Option B: User-local install (no sudo, requires FCITX_ADDON_DIRS)
cmake -B build -DCMAKE_INSTALL_PREFIX=$HOME/.local
cmake --build build -j
cmake --install build
```

### karukan-macos

```bash
cd karukan-im/macos

make test      # Swift tests (incl. integration tests against a real karukan-imserver)
make install   # Build, assemble Karukan.app, install to ~/Library/Input Methods (auto-downloads dict.bin if missing and prefetches all models.toml models into the HF cache)
```

First install requires logout/login; afterwards `make install` + `killall KarukanIME` suffices.

### Code Quality

```bash
cargo fmt --all       # Format all crates
cargo clippy --workspace  # Lint all crates
```

## Architecture

### karukan-engine (`karukan-engine/src/`)

- `lib.rs` — Library entry point and re-exports
- `romaji/` — Romaji-to-hiragana conversion
  - `trie.rs` — Trie data structure
  - `rules.rs` — 200+ conversion rule
  - `converter.rs` — Stateless converter (`convert`/`flush_pending`/`starts_rule`)
- `kanji/` — Kana-kanji conversion via llama.cpp
  - `backend.rs` — Backend + KanaKanjiConverter
  - `llamacpp.rs` — GGUF inference
  - `hf_download.rs` — HuggingFace model download
  - `model_config.rs` — models.toml registry
  - `error.rs` — KanjiError type
- `rewriter/` — Candidate rewriter system
  - `mod.rs` — Rewriter trait, RewriterChain, default_chain()
  - `alphabet.rs` — Alphabet width/case variants (e.g. `abc` → `ABC`, `ａｂｃ`, `ＡＢＣ`)
  - `half_katakana.rs` — Half-width katakana variants (e.g. `がっこう` → `ｶﾞｯｺｳ`)
  - `symbol.rs` — Symbol variant chains and reading→symbol lookup (Mozc symbol.tsv derived)
- `dict.rs` — Double-array trie system dictionary
- `learning.rs` — Learning cache (user conversion history, TSV persistence, recency+frequency scoring)
- `kana.rs` — Hiragana/katakana utilities, full-width/half-width conversion functions

### karukan-cli (`karukan-cli/src/`)

- `bin/dict.rs` — Dictionary tool: build (JSON or Mozc TSV → binary) and view (web UI + CLI search)
- `bin/sudachi_dict.rs` — Sudachi dictionary → scored JSON converter
- `bin/server.rs` — Axum HTTP API server
- `bin/ajimee_bench.rs` — AJIMEE-Bench evaluation
- `static/` — Web UI assets for server and dict-viewer

### karukan-im (`karukan-im/core/src/`)

- `core/engine/` — IMEEngine state machine (Empty → Composing → Conversion)
  - `mod.rs` — Main InputMethodEngine struct and core processing logic
  - `types.rs` — EngineConfig, EngineResult, EngineAction, Converters, ConversionStrategy
  - `input.rs` — Key input handling for Composing state
  - `input_buffer.rs` — Composition record: per-display-char element array (`Romaji`/`Converted`) + caret index; display/reading/pending are derived views
  - `conversion.rs` — Conversion mode handling (mixed candidate list, key handling, commit)
  - `model.rs` — Model dispatch and the cache in front of it (`run_kana_kanji_conversion`, `cached_convert`, `run_parallel_beam`, `model_candidates`)
  - `filter.rs` — Ctrl+R / Ctrl+T source-filtered candidate views (`FILTER_CYCLE`, `source_view`, `apply_candidate_filter`)
  - `chunk/split.rs` — Where the chunk boundaries fall, engine-free: every char is classified into a `Token`, and `group_chunks` walks them once, packing them into chunks under `ChunkLimits`. Pure functions, unit-tested in place
  - `chunk/mod.rs` — What the engine does with the chunks: `chunked_auto_suggest`, the chunk-grid conversion, and the manual breaks
  - `cache.rs` — LRU conversion cache keyed by the computation: (katakana reading, lctx, model role, beam width)
  - `cursor.rs` — Cursor movement
  - `display.rs` — Preedit text display
  - `mode.rs` — Mode switching (katakana, alphabet, live conversion)
  - `init.rs` — Model loading, dictionary setup, learning cache init
  - `strategy.rs` — Conversion strategy determination and adaptive model selection
  - `tests.rs` — Engine unit tests
- `core/preedit.rs` — Preedit composition with cursor support
- `core/candidate.rs` — Candidate list with pagination support
- `core/keycode.rs` — Key symbol definitions and key event handling
- `core/state.rs` — Engine state definitions
- `config/settings.rs` — User settings (`~/.config/karukan-im/config.toml` on Linux, `~/Library/Application Support/com.karukan.karukan-im/` on macOS)
- `server/` — stdio JSON-RPC 2.0 server for the macOS frontend (`protocol.rs` defines the wire format; `bin/karukan-imserver.rs` is the entry point)

### karukan-fcitx5 (`karukan-im/fcitx5/`)

Linux fcitx5 frontend. Wraps karukan-im via C FFI and exposes the engine to the C++ addon.

- `src/ffi/mod.rs` — `KarukanEngine` opaque struct, action dispatch, cache structs, FFI macros
- `src/ffi/lifecycle.rs` — `karukan_engine_new/init/free`
- `src/ffi/input.rs` — `karukan_engine_process_key/reset/set_surrounding_text`
- `src/ffi/query.rs` — All getter functions (preedit, commit, candidates, aux, timing)
- `include/karukan.h` — C header for the fcitx5 C++ addon
- `fcitx5-addon/src/karukan.cpp` — C++ fcitx5 wrapper

### karukan-macos (`karukan-im/macos/Sources/KarukanIME/`)

Swift/InputMethodKit frontend. All IME state lives in karukan-imserver (spawned as a bundled child process); Swift only adapts IMK events and renders UI.

- `main.swift` — IMKServer startup, engine process spawn, wake-from-sleep restart, SIGPIPE handling
- `KarukanInputController.swift` — IMKInputController; translates keys, applies engine actions (preedit/candidates/commit), JIS かな key and right-Command tap return to hiragana (exit katakana mode)
- `KeyCodeMap.swift` — NSEvent → XKB keysym translation (same keysym representation as fcitx5), RightCommandTapDetector
- `resources/*.tiff` — template menu icon (か), regenerated via `swift scripts/generate_icons.swift`; `resources/{ja,en}.lproj/InfoPlist.strings` localize the input mode name shown in the input menu
- `EngineProcess.swift` — child process lifecycle: crash restart with exponential backoff, EOF-based clean shutdown (lets the server save its learning cache)
- `EngineClient.swift` — JSON-RPC transport (sync for process_key, async for fire-and-forget)
- `EngineProtocol.swift` — Swift mirror of `karukan-im/core/src/server/protocol.rs` (keep in sync; protocol_version guards breaking changes)
- `CandidateWindowController.swift` — custom NSPanel candidate window (engine pre-paginates)

## macOS Input Mode Design

`karukan-macos` registers **only the Japanese input mode** (`dev.togatoga.inputmethod.Karukan.Japanese`) in `Info.plist`. There is no Roman/英数 mode inside Karukan — if the user wants to type in Latin script they switch to the OS-level English input source (e.g. via Karabiner). Do not add a Roman mode back; it is intentionally absent.

The engine-internal `InputMode::Alphabet` (entered via Shift+letter on Linux/fcitx5) is a separate Rust engine concept unrelated to this macOS input mode registration. Do not conflate the two.

## Key Design Patterns

- IMEEngine uses a state machine: Empty → Composing → Conversion
- `input_buf: InputBuffer` in IMEEngine is the source of truth: an element array (one element per display character — `Romaji(char)` unfired keystroke / `Converted(char)` settled character: fired output, passthrough, or direct input) plus a caret index. All views (display, conversion reading, aux romaji tail) are derived from it
- `RomajiConverter` is stateless (pure `convert`/`flush_pending`); after each romaji keystroke the engine re-evaluates only the Romaji run ending at the caret (`evaluate_run`), re-recording fired keystrokes as `Converted`. `Converted` never re-enters evaluation, so settled text never reverts. Backspace/delete remove one element and then evaluate the run the removal joined, so the result always equals typing the remaining keystrokes fresh (`ykt` → BS → `o` → 「yこ」; `yt1t` minus the `1` → 「yっt」). Cursor moves and mode toggles never touch the array
- Models use jinen format with special Unicode tokens (U+EE00–U+EE02) from the Private Use Area; model input is katakana (hiragana is converted to katakana before inference)
- Model registry defined in `karukan-engine/models.toml`; default models use Q5_K_M quantization
- Live conversion (auto-suggest) splits the composing buffer into internal chunks of at most `chunk_chars` reading chars (default 30). Splitting bounds each model call and freezes settled text: once a boundary is behind the caret that chunk's reading and lctx no longer change, so it stays a cache hit and its display stops flickering. Chunks are internal — the user sees one continuous preedit
- Chunk boundaries (`group_chunks`) open at the length cap, at a manual break (Ctrl+J → `insert_chunk_break`; boundaries live in `chunk_breaks` as reading positions, shift with edits via `edit_with_chunk_breaks`, and are cleared when the composition ends), and around non-Japanese chars (`is_japanese`: hiragana, katakana incl. `ー`, and kanji are Japanese; the middle dot `・` is special-cased as non-Japanese). A chunk containing Japanese keeps marks up to `chunk_symbols` (default 1) so 「おい、お前だよ」 keeps converting as one unit instead of freezing a premature 「老、」; digits up to `chunk_digits` (default 0, so digits stay out of the model — it hallucinates on digit runs, dropping or duplicating figures), filled greedily like the symbol budget, so the digits that fit ride along and the rest open the next chunk; and alphabet chars up to `chunk_alphabets` (default 0, so latin text stays passthrough and the unfired romaji tail — the `d` in 「わせだd」 — never reaches the model as part of the reading). A chunk with no Japanese is exempt from both caps and never reaches the model
- `chunked_auto_suggest` re-chunks the whole buffer from scratch on every keystroke and re-runs every chunk through `run_kana_kanji_conversion`, which caches results in an LRU keyed by the computation itself — (katakana reading, lctx, model role, beam width) rather than by the requesting strategy, so strategies sharing a computation share its entry (`ParallelBeam` is exactly a main-greedy plus a light-beam entry, and its halves are the same ones `MainModelOnly` / `LightModelBeam` compute). Unchanged chunks are cache hits and only chunks whose reading or left context changed reach the model (a middle edit therefore reconverts the chunks to its right with their updated lctx). Each chunk's left context (lctx) is the editor surrounding text plus the converted text of the preceding chunks, truncated to `context_chars`
- The aux line is quiet by default — state, reading, candidate source, page. While composing it tracks the caret's chunk: its reading with a `used/max` fill counter (「わせだd 3/30」, `0/30` right after a manual break, making the cut visible). Ctrl+Shift+V (`toggle_verbose`, `[display] verbose` to start that way) adds the debug details, re-rendering the current state's line on the spot: the beam span alone, labelled 🎯 and counted against `beam_chars` (`🎯 うえ 2/8`) — the frozen head is left out, like the composing aux shows only the chunk being typed — plus inference timing, the model that ran, and the lctx handed to it (`conversion_chunk_reading`; the learning/dictionary views and a selected predictive candidate keep the plain reading, since neither is beamed as a span). User-facing doc: `docs/chunking.md`
- Dictionary lookup during composing/conversion is exact match plus predictive (prefix-extending) matches: readings that start with the typed reading surface as extra candidates (2+ typed chars required; up to 3 in the composing suggestion list, uncapped in the paged conversion list), ranked by `score + 500·ln(50·remaining_chars)` — dictionary scores are -500·log(p) costs, so longer completions are demoted on the same scale. Predictive candidates carry their full reading so committing records under the right key. While a romaji tail is unresolved, prediction is narrowed to the kana that tail can become (わせ + `d` keeps わせだ… and drops わせり…; a tail that cannot become kana suppresses prediction)
- Learning cache records user-selected conversions and boosts them on subsequent conversions; candidate priority: Learning → User Dictionary → Model → System Dictionary → Fallback → Rewriter. Surfaces longer than `max_surface_chars` (default 50, `[learning]` in config.toml; both learning limits travel as `LearningConfig`) are not recorded. During conversion, Ctrl+Backspace or Ctrl+Delete (the Mac "delete" key is Backspace) removes the selected learning candidate from the history, mozc-style (`DeleteSelectedCandidate`): the removal clears the entry's whole prefix fan-out (`LearningCache::remove_suggestion`) so deduped twins under longer readings can't resurface, and the conversion is then rebuilt in place (mozc's cancel-and-reconvert, minus the window blink) so a surface that the model/dictionary/fallback also produce survives as an ordinary candidate. With a non-learning candidate selected the chord is consumed but does nothing (mozc's `DoNothing`); cancelling stays on plain Backspace/Escape; the chord exists only in the Conversion state (Composing keeps plain char editing). Deletability is derived from `Candidate::source` (`CandidateSource::is_deletable`, mozc's `USER_HISTORY_PREDICTION` analogue), and while a learning candidate is selected the aux text shows the mozc-style footer hint 「Ctrl+Backspaceで履歴から削除」
- Typing a printable character during Conversion refines instead of committing (deliberate mozc deviation, incremental-search style): the engine drops back to the untouched composition and feeds the keystroke, so the reading grows and the live suggestion rewrites in place; inside a narrowed source view the filter survives the keystroke (`refine_through_composing` re-enters the conversion with the same source; the intermediate composing render is discarded, so its auto-suggest inference is suppressed for that one call — the view runs its own lookup) and plain Backspace mirrors it by shrinking the reading in place (unfiltered Backspace still cancels back to the composition). Ctrl+digit (1-9) selects a candidate — bare digits refine like any other printable char, so typing numbers never conflicts with selection — Ctrl+J (`rebreak_conversion`) inserts a chunk break at the caret and rebuilds in place, narrowing what the beam covers while keeping the active source filter, and Enter commits
- Source views query their own source: learning and dictionary use the live buffer's base reading + romaji tail so mid-word consonants narrow predictively — the tail constraint applies to learning predictions too, so a stale exact match can never swallow the tail on commit; the model and rewriter views use the state's settled reading (the exact text Enter commits)
- Model candidates — Space's mixed list and the AI view share the same split conversion (`model_candidates`) — beam only a span: the trailing chunks on the live-conversion grid fitting `beam_chars` (`beam_span_start` → `trailing_chunks_start` walks `group_chunks`' own output backwards, so it always lands on a boundary; a chunk with no Japanese and a manual break each wall the span, keeping digits out of the model and the frozen text frozen; cutting anywhere else would leave a prefix live conversion never converted, costing an extra inference and showing a seam the user never saw). Everything before the span converts top-1 on the grid, so the prefix is exactly the chunks typing already cached. Cost stays bounded and beam-width alternatives survive however long the reading grows; the adaptive latency downgrade (main model over `max_latency_ms`) lands on `LightModelBeam`, so a slow main model costs quality but never the candidate count. An empty model list means the model produced nothing; a candidate equal to the reading is a real answer (a kana-only word like きゃりーぱみゅぱみゅ converts to itself) and rides like any other
- The model list is headed by the whole-reading top-1 on the live-conversion chunk grid — the exact text live typing displays — and it costs no extra pass: the span is the last chunk, so its `ParallelBeam` main-greedy half *is* what the grid would compute for that chunk, and `prefix + that greedy` is the head. Running it as the beam's main half computes the head and the alternatives in parallel rather than one conversion before the other. The head is normally a pure cache replay, since the cache is keyed by computation and main greedy runs at most once per (reading, lctx) however live typing, Space, and the AI view interleave. A light-model request is additionally served by the main model's entry for the same reading and beam width when one exists (never the reverse — that would downgrade quality), so a latency downgrade doesn't re-infer what the main model already converted. The adaptive gate is fed the main model's own elapsed time — ParallelBeam times its main half separately, and a cache-served half reports no measurement — so an expensive one-shot beam can no longer spuriously downgrade the rest of the word
- Ctrl+I jumps straight to the model view from either Composing or Conversion (`source_for_key` → `jump_to_source`); it is the only view with a dedicated key, since the rest are a step or two along the cycle and are opened rarely. Ctrl+R / Ctrl+T in the Conversion state cycle a candidate-source filter forward / backward (dedicated keys, both keysym cases — direction never depends on the Shift bit, which some environments fold into an uppercase keysym) — Tab stays next-candidate and Shift+Tab (`ISO_LEFT_TAB` on X11) prev-candidate for mozc-compatible muscle memory (`FILTER_CYCLE` in filter.rs: a pure rotation grouped by what the user is after — taught (learning), looked up (📚 covers both dictionaries as one stop, the user's own entries first and deduped by surface, since which book a word came from is already in each candidate's own annotation), guessed (model), rewritten; the model sits late because Ctrl+I reaches it in one press — the full list is not a stop, it is what Space already shows and Esc→Space returns to it; the rewriter view carries the plain kana at its tail (skipped in emoji mode — the picker shows emojis only), derived from the reading so mixed-list dedup can't hide them, and sits last so Ctrl+T reaches it in one press). Exactly one step per press: an empty source shows an empty window with 「候補なし」 in the aux, never skipped, so the position stays predictable; Ctrl+R while Composing starts the conversion already narrowed one step (`start_filtered_conversion`); the filter and the unfiltered list live in `InputState::Conversion` so they die with the state, and the aux header shows the active source's emoji (`[変換:📝]`). Inside the Conversion state every unbound non-Alt key is consumed as a no-op (mozc's TestSendKey does the same for Composition/Conversion), so chords like Ctrl+R never leak to the application mid-conversion — leaking them was the original bug that made browsers reload; Alt chords pass through before any binding matches (Alt+Return must not commit, Alt+Tab must not navigate) so desktop shortcuts keep working
- Data files (system dictionary `dict.bin`, user dictionaries `user_dicts/`, learning cache `learning.tsv`) live in the data directory: `~/.local/share/karukan-im/` on Linux, `~/Library/Application Support/com.karukan.karukan-im/` on macOS; a prebuilt `dict.tgz` is published on GitHub releases
- Learning cache is persisted as TSV (`learning.tsv` in the data directory); saved on deactivate and engine free, not on every commit
- Learning score uses recency-weighted formula (mozc-inspired): `recency * 10.0 + ln(1 + frequency)`; eviction removes lowest-score entries when over `max_entries` (default: 10,000)

## Training (karukan-jinen)

Model training is handled by the separate `karukan-jinen` Python project (not in this repository). It trains small language models (GPT-2 and Qwen3 based) for kana-kanji conversion using the jinen format, and outputs GGUF files for use with karukan-engine.
