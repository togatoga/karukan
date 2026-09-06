# karukan-engine

日本語入力エンジン：ローマ字からひらがなへの変換と、llama.cppによるニューラルかな漢字変換。

## Overview

karukan-engineは、karukanプロジェクトのコアライブラリです。以下の機能を提供します：

- **ニューラルかな漢字変換** — llama.cppによるGGUF形式のモデル（GPT-2 / Qwen3ベース）
- **辞書検索** — Double-array trieによる高速な前方一致・完全一致検索
- **変換学習** — ユーザーの変換履歴を記憶し、完全一致・前方一致（予測変換）で候補を優先表示。TSV形式で永続化

サーバー・CLIツールについては [karukan-cli](../karukan-cli/) を参照してください。

## Quick Start

```bash
# Build（リポジトリルートから実行）
cargo build -p karukan-engine --release

# テスト実行（ユニットテスト、モデルのダウンロード不要）
cargo test -p karukan-engine

# 統合テスト実行（モデルのダウンロードが必要）
cargo test -p karukan-engine -- --ignored
```

## Library Usage

### Romaji-to-Hiragana

```rust
use karukan_engine::RomajiConverter;

let converter = RomajiConverter::new();
// "nn" は常に「ん」なので、「こんにちは」には n が3つ必要:
// ko→こ, nn→ん, ni→に, chi→ち, ha→は
let result = converter.convert("konnnichiha");
assert_eq!(result.text, "こんにちは");
assert_eq!(result.pending, ""); // まだ確定していないローマ字末尾（`k` など）はこちらに残る
```

### Kana-Kanji Conversion

```rust
use karukan_engine::{Backend, KanaKanjiConverter, ModelSource};

// モデルの読み込み（初回使用時にHuggingFaceからダウンロード）
let source = ModelSource::Hf {
    repo: "togatogah/jinen-v2-small.gguf".to_string(),
    filename: "jinen-v2-small-Q5_K_M.gguf".to_string(),
};
let backend = Backend::from_source(&source)?;
let converter = KanaKanjiConverter::new(backend)?;

let candidates = converter.convert("かんじ", "", 3)?;
// => ["漢字", "感じ", "幹事"]
```

### Learning Cache

```rust
use karukan_engine::{LearningCache, LearningConfig};
use std::path::Path;

// 新規作成（デフォルト: 最大10,000エントリ）
let mut cache = LearningCache::new(LearningConfig::default());

// 変換結果を記録
cache.record("わせだだいがく", "早稲田大学");
cache.record("きょう", "今日");

// 完全一致検索（読みが一致する候補をスコア順に返す）
let results = cache.lookup("きょう");
// => [("今日", score)]

// 前方一致検索（予測変換: 読みが前方一致する候補を返す）
let results = cache.prefix_lookup("わせだ");
// => [("わせだだいがく", "早稲田大学", score)]

// TSVファイルに保存・読み込み
cache.save(Path::new("learning.tsv"))?;
let cache = LearningCache::load(Path::new("learning.tsv"), LearningConfig::default())?;
```

### Dictionary

```rust
use karukan_engine::Dictionary;

// バイナリ辞書の読み込み
let dict = Dictionary::load("dict.bin")?;

// 完全一致検索
if let Some(result) = dict.exact_match_search("きょう") {
    for candidate in result.candidates {
        println!("{} (score: {})", candidate.surface, candidate.score);
    }
}

// 前方一致検索
let results = dict.common_prefix_search("きょうと");
```

## Models

karukan-engine 自体はモデルの一覧を持ちません。`ModelSource`（HuggingFace の repo + filename、またはローカルの GGUF パス）を `Backend::from_source()` に渡すと自動的にダウンロード・読み込みされます。IME としての既定モデルは [`karukan-im/core/config/default.toml`](../karukan-im/core/config/default.toml) の `[models]` に定義されています。

| モデルキー | ベースモデル | パラメータ数 | 量子化 | Accuracy@1 (NFKC) | デフォルト |
|------------|-----------|-----------|--------------|------:|---------|
| [`jinen-v2-small-q5`](https://huggingface.co/togatogah/jinen-v2-small.gguf) | Qwen3 | 109M | Q5_K_M | 86.0% | Yes |
| [`jinen-v2-xsmall-q5`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf) | Qwen3 | 36M | Q5_K_M | 79.0% | |
| [`jinen-v1.1-beta-q5`](https://huggingface.co/togatogah/jinen-v1.1-beta.gguf) | Qwen3 | 109M | Q5_K_M | 86.0% | |
| [`jinen-v1-small-q5`](https://huggingface.co/togatogah/jinen-v1-small.gguf) | GPT-2 | 90M | Q5_K_M | 76.5% | |
| [`jinen-v1-xsmall-q5`](https://huggingface.co/togatogah/jinen-v1-xsmall.gguf) | GPT-2 | 26M | Q5_K_M | 71.0% | |

IMEで使うモデルは設定ファイルの `model` / `light_model` で切り替えられます（[docs/configuration.md](../docs/configuration.md) 参照）。

### jinen Format

モデルはPrivate Use Areaの特殊Unicodeトークンを使用するjinen形式でトレーニングされています。
この形式は[zenzai](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/main/Docs/zenzai.md)のかな漢字変換モデル「zenz」の第3世代（zenz-v3）フォーマットを参考にしています。
zenz-v3ではコンテキストを前置する `\uEE02<context>\uEE00<input_katakana>\uEE01<output></s>` 方式を推奨しており、jinen形式も同じトークン配置を採用しています。

| トークン | Unicode | 用途 |
|-------|---------|---------|
| INPUT_START | U+EE00 | カタカナ入力開始 |
| OUTPUT_START | U+EE01 | 漢字出力開始 |
| CONTEXT | U+EE02 | 左コンテキストマーカー |

プロンプト形式：`{CONTEXT}<context>{INPUT_START}<katakana>{OUTPUT_START}`

## License

MIT OR Apache-2.0
