# karukan-engine

日本語入力エンジン：ローマ字からひらがなへの変換と、llama.cppによるニューラルかな漢字変換。

## Overview

karukan-engineは、karukanプロジェクトのコアライブラリです。以下の機能を提供します：

- **ローマ字→ひらがな変換** — Trieベースの有限状態マシンによる200以上の変換ルール（Google IME互換）
- **ニューラルかな漢字変換** — llama.cppによるGGUF形式のGPT-2モデル
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

let mut converter = RomajiConverter::new();
// "nn" は常に「ん」なので、「こんにちは」には n が3つ必要:
// ko→こ, nn→ん, ni→に, chi→ち, ha→は
for ch in "konnnichiha".chars() {
    converter.push(ch);
}
assert_eq!(converter.output(), "こんにちは");
```

### Kana-Kanji Conversion

```rust
use karukan_engine::{Backend, KanaKanjiConverter};

// モデルの読み込み（初回使用時にHuggingFaceからダウンロード）
let backend = Backend::from_variant_id("jinen-v1-small-q5")?;
let converter = KanaKanjiConverter::new(backend)?;

let candidates = converter.convert("かんじ", "", 3)?;
// => ["漢字", "感じ", "幹事"]
```

### Learning Cache

```rust
use karukan_engine::LearningCache;
use std::path::Path;

// 新規作成（最大エントリ数: 10,000）
let mut cache = LearningCache::new(10_000);

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
let cache = LearningCache::load(Path::new("learning.tsv"), 10_000)?;
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

モデルは`models.toml`で定義されています。`Backend::from_variant_id()`で指定すると自動的にダウンロードされます。

| バリアントID | パラメータ数 | 量子化 | デフォルト |
|------------|-----------|--------------|---------|
| `jinen-v1-xsmall-q5` | 26M | Q5_K_M | |
| `jinen-v1-small-q5` | 90M | Q5_K_M | Yes |

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
