# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

karukan is a Japanese Input Method system for Linux and macOS, consisting of four Rust crates and a Swift package:

- **karukan-engine**: Core library — romaji-to-hiragana conversion, neural kana-kanji conversion via llama.cpp, system dictionary, learning cache, candidate rewriter (width/case/symbol variants)
- **karukan-cli**: CLI tools and server — dictionary builder, Sudachi converter, dict viewer, AJIMEE-Bench, HTTP API server
- **karukan-im**: Shared IME engine state machine (Empty → Composing → Conversion) and `karukan-imserver` stdio JSON-RPC server (macOS binary bundled in karukan-macos)
- **karukan-fcitx5**: fcitx5 Linux frontend — C FFI (`src/ffi/`) and C++ addon (`fcitx5-addon/`) that wrap karukan-im
- **karukan-macos**: Swift/InputMethodKit frontend that spawns `karukan-imserver` as a bundled child process

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
cd karukan-fcitx5/fcitx5-addon

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
cd karukan-macos

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
  - `converter.rs` — FSM converter
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

### karukan-im (`karukan-im/src/`)

- `core/engine/` — IMEEngine state machine (Empty → Composing → Conversion)
  - `mod.rs` — Main InputMethodEngine struct and core processing logic
  - `types.rs` — EngineConfig, EngineResult, EngineAction, Converters, ConversionStrategy
  - `input.rs` — Key input handling for Composing state
  - `input_buffer.rs` — Input buffer (hiragana text + cursor position)
  - `conversion.rs` — Conversion mode handling (candidate building, commit)
  - `chunk.rs` — Live-conversion chunking: the Japanese/non-Japanese split (`is_japanese`, `group_chunks`), incremental re-chunk diff (`ChunkPlan`), and `chunked_auto_suggest`
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

### karukan-fcitx5 (`karukan-fcitx5/`)

Linux fcitx5 frontend. Wraps karukan-im via C FFI and exposes the engine to the C++ addon.

- `src/ffi/mod.rs` — `KarukanEngine` opaque struct, action dispatch, cache structs, FFI macros
- `src/ffi/lifecycle.rs` — `karukan_engine_new/init/free`
- `src/ffi/input.rs` — `karukan_engine_process_key/reset/set_surrounding_text`
- `src/ffi/query.rs` — All getter functions (preedit, commit, candidates, aux, timing)
- `include/karukan.h` — C header for the fcitx5 C++ addon
- `fcitx5-addon/src/karukan.cpp` — C++ fcitx5 wrapper

### karukan-macos (`karukan-macos/Sources/KarukanIME/`)

Swift/InputMethodKit frontend. All IME state lives in karukan-imserver (spawned as a bundled child process); Swift only adapts IMK events and renders UI.

- `main.swift` — IMKServer startup, engine process spawn, wake-from-sleep restart, SIGPIPE handling
- `KarukanInputController.swift` — IMKInputController; translates keys, applies engine actions (preedit/candidates/commit), JIS かな key and right-Command tap return to hiragana (exit katakana mode)
- `KeyCodeMap.swift` — NSEvent → XKB keysym translation (same keysym representation as fcitx5), RightCommandTapDetector
- `resources/*.tiff` — template menu icon (か), regenerated via `swift scripts/generate_icons.swift`; `resources/{ja,en}.lproj/InfoPlist.strings` localize the input mode name shown in the input menu
- `EngineProcess.swift` — child process lifecycle: crash restart with exponential backoff, EOF-based clean shutdown (lets the server save its learning cache)
- `EngineClient.swift` — JSON-RPC transport (sync for process_key, async for fire-and-forget)
- `EngineProtocol.swift` — Swift mirror of `karukan-im/src/server/protocol.rs` (keep in sync; protocol_version guards breaking changes)
- `CandidateWindowController.swift` — custom NSPanel candidate window (engine pre-paginates)

## macOS Input Mode Design

`karukan-macos` registers **only the Japanese input mode** (`dev.togatoga.inputmethod.Karukan.Japanese`) in `Info.plist`. There is no Roman/英数 mode inside Karukan — if the user wants to type in Latin script they switch to the OS-level English input source (e.g. via Karabiner). Do not add a Roman mode back; it is intentionally absent.

The engine-internal `InputMode::Alphabet` (entered via Shift+letter on Linux/fcitx5) is a separate Rust engine concept unrelated to this macOS input mode registration. Do not conflate the two.

## Key Design Patterns

- IMEEngine uses a state machine: Empty → Composing → Conversion
- `input_buf: InputBuffer` in IMEEngine is the source of truth for hiragana text (`.text` field holds the composed hiragana, `.cursor_pos` tracks cursor position)
- RomajiConverter accumulates output; consumed into input_buf via delta tracking
- Models use jinen format with special Unicode tokens (U+EE00–U+EE02) from the Private Use Area; model input is katakana (hiragana is converted to katakana before inference)
- Model registry defined in `karukan-engine/models.toml`; default models use Q5_K_M quantization
- Live conversion (auto-suggest) splits the composing buffer into internal chunks of at most `composing_chunk_len` reading chars (default 40, configurable) so each model call stays bounded for long input. Chunking (`group_chunks`) starts a new chunk whenever the current one is full OR the character group changes between Japanese and non-Japanese (`is_japanese`: hiragana, katakana incl. `ー`, and kanji are Japanese; ASCII/full-width digits, letters, symbols, and all punctuation are non-Japanese). A non-Japanese run (digits/symbols/alphabet) is passed through to the preedit verbatim and never sent to the neural converter (which otherwise tends to drop digits mid-run, e.g. `123456`); a Japanese run is converted by the model. Because punctuation is non-Japanese, it forms its own chunk and naturally separates clauses (`今日は。明日` → `今日は`/`。`/`明日`), so there is no separate punctuation rule. A katakana word like `スーパーマーケット` is entirely Japanese, so it stays one chunk. `chunked_auto_suggest` re-chunks incrementally: it diffs the new buffer against the previous chunking by common character prefix/suffix and reconverts only the changed span (`ChunkPlan` decides which leading/trailing chunks to reuse). Each chunk's left context (lctx) is the editor surrounding text plus the converted text of the preceding chunks, truncated to `max_context_length`. Chunks are internal — the user sees one continuous preedit, and the aux text shows the current chunk's lctx as its single `lctx:`
- Learning cache records user-selected conversions and boosts them on subsequent conversions; candidate priority: Learning → User Dictionary → Model → System Dictionary → Fallback → Rewriter
- Data files (system dictionary `dict.bin`, user dictionaries `user_dicts/`, learning cache `learning.tsv`) live in the data directory: `~/.local/share/karukan-im/` on Linux, `~/Library/Application Support/com.karukan.karukan-im/` on macOS; a prebuilt `dict.tgz` is published on GitHub releases
- Learning cache is persisted as TSV (`learning.tsv` in the data directory); saved on deactivate and engine free, not on every commit
- Learning score uses recency-weighted formula (mozc-inspired): `recency * 10.0 + ln(1 + frequency)`; eviction removes lowest-score entries when over `max_entries` (default: 10,000)

## Training (karukan-jinen)

Model training is handled by the separate `karukan-jinen` Python project (not in this repository). It trains GPT-2 based models for kana-kanji conversion using the jinen format, and outputs GGUF files for use with karukan-engine.
