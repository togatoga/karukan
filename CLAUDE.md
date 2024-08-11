# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

karukan is a Linux Japanese Input Method system consisting of three Rust crates:

- **karukan-engine**: Core library — romaji-to-hiragana conversion, neural kana-kanji conversion via llama.cpp, system dictionary, learning cache
- **karukan-cli**: CLI tools and server — dictionary builder, Sudachi converter, dict viewer, AJIMEE-Bench, HTTP API server
- **karukan-im**: fcitx5 IME addon using karukan-engine for Japanese input on Linux

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

# Build and install fcitx5 addon
cd karukan-im/fcitx5-addon

# Option A: System install (sudo required, no FCITX_ADDON_DIRS needed)
cmake -B build -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build -j
sudo cmake --install build

# Option B: User-local install (no sudo, requires FCITX_ADDON_DIRS)
cmake -B build -DCMAKE_INSTALL_PREFIX=$HOME/.local
cmake --build build -j
cmake --install build
```

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
  - `rules.rs` — 200+ conversion rules (Google IME compatible)
  - `converter.rs` — FSM converter
- `kanji/` — Kana-kanji conversion via llama.cpp
  - `backend.rs` — Backend + KanaKanjiConverter
  - `llamacpp.rs` — GGUF inference
  - `hf_download.rs` — HuggingFace model download
  - `model_config.rs` — models.toml registry
  - `error.rs` — KanjiError type
- `dict.rs` — Double-array trie system dictionary
- `learning.rs` — Learning cache (user conversion history, TSV persistence, recency+frequency scoring)
- `kana.rs` — Hiragana/katakana utilities

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
  - `conversion.rs` — Conversion mode handling
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
- `config/settings.rs` — User settings (`~/.config/karukan-im/config.toml`)
- `ffi.rs` — C FFI for fcitx5 C++ addon
- `fcitx5-addon/src/karukan.cpp` — C++ fcitx5 wrapper

## Key Design Patterns

- IMEEngine uses a state machine: Empty → Composing → Conversion
- `input_buf: InputBuffer` in IMEEngine is the source of truth for hiragana text (`.text` field holds the composed hiragana, `.cursor_pos` tracks cursor position)
- RomajiConverter accumulates output; consumed into input_buf via delta tracking
- Models use jinen format with special Unicode tokens (U+EE00–U+EE02) from the Private Use Area; model input is katakana (hiragana is converted to katakana before inference)
- Model registry defined in `karukan-engine/models.toml`; default models use Q5_K_M quantization
- Learning cache records user-selected conversions and boosts them on subsequent conversions; candidate priority: Learning → User Dictionary → Model → System Dictionary → Fallback
- Learning cache is persisted as TSV (`~/.local/share/karukan-im/learning.tsv`); saved on deactivate and engine free, not on every commit
- Learning score uses recency-weighted formula (mozc-inspired): `recency * 10.0 + ln(1 + frequency)`; eviction removes lowest-score entries when over `max_entries` (default: 10,000)

## Training (karukan-jinen)

Model training is handled by the separate `karukan-jinen` Python project (not in this repository). It trains GPT-2 based models for kana-kanji conversion using the jinen format, and outputs GGUF files for use with karukan-engine.
