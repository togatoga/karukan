# Contributing to Karukan

Thank you for your interest in contributing to Karukan, a neural Kana-Kanji conversion Japanese input method!

## Rust Workspace Setup

Karukan is structured as a Rust workspace. Ensure you have the latest Rust toolchain installed:
```bash
rustup update stable
```

### Building & Testing
To build all packages in the workspace:
```bash
cargo build --workspace
```

To run the unit tests:
```bash
cargo test --workspace
```

## Code Formatting
All code should be formatted using `rustfmt` before committing:
```bash
cargo fmt --all -- --check
```

Ensure all your changes pass the Cargo tests and formatting checks.
