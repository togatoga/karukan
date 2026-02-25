# karukan-cli

karukan-engineを利用したCLIツール群。かな漢字変換サーバー、辞書ビルド、辞書ビューア、ベンチマーク評価ツールを提供します。

## Binaries

| バイナリ | 概要 |
|---------|------|
| `karukan-dict` | 辞書のビルド（JSON/Mozc TSV → バイナリ）とビューア（Web UI + CLI検索） |
| `sudachi-dict` | Sudachi CSVからJSON辞書を生成 |
| `karukan-server` | かな漢字変換HTTPサーバー（Web UI付き） |
| `ajimee-bench` | AJIMEE-Bench評価ツール |

## Build

```bash
# リポジトリルートから実行
cargo build -p karukan-cli --release
```

## karukan-dict

辞書のビルドと検索を行うツール。`build` と `view` の2つのサブコマンドがあります。

### build — 辞書ビルド

JSON または Mozc TSV 形式の入力ファイルからバイナリ辞書を生成します。

```bash
# JSON形式（拡張子で自動判定）
cargo run --release --bin karukan-dict -- build input.json -o dict.bin

# Mozc TSV形式
cargo run --release --bin karukan-dict -- build mozc.tsv -o dict.bin

# フォーマットを明示指定
cargo run --release --bin karukan-dict -- build input.txt --format json -o dict.bin
```

| オプション | デフォルト | 説明 |
|-----------|----------|------|
| `input` (必須) | — | 入力ファイル（JSON or Mozc TSV） |
| `-o, --output` | `dict.bin` | 出力バイナリ辞書ファイル |
| `-f, --format` | 自動判定 | 入力形式: `json` or `mozc` |

**入力形式:**
- `json`: `[{reading, candidates: [{surface, score}]}]` の配列
- `mozc`: Mozc/Google IME TSV（`reading\tword\tPOS\tcomment`）

### view — 辞書ビューア

辞書の内容を検索・閲覧します。CLIモードとWebモードの2つの動作モードがあります。

```bash
# Webモード（ブラウザで辞書を検索）
cargo run --release --bin karukan-dict -- view dict.bin
# → http://127.0.0.1:8080

# CLI検索（完全一致）
cargo run --release --bin karukan-dict -- view dict.bin --query きょう

# CLI検索（前方一致）
cargo run --release --bin karukan-dict -- view dict.bin --query きょう --prefix

# CLI検索（表層形で検索）
cargo run --release --bin karukan-dict -- view dict.bin --query 今日 --surface

# 全エントリのダンプ
cargo run --release --bin karukan-dict -- view dict.bin --all
```

| オプション | デフォルト | 説明 |
|-----------|----------|------|
| `dicts` (必須) | — | 辞書ファイル（複数指定可、KRKN or Mozc TSV） |
| `--port` | `8080` | Webモードのポート |
| `--host` | `127.0.0.1` | Webモードのバインドアドレス |
| `-q, --query` | — | CLI検索クエリ |
| `-s, --surface` | off | 表層形で検索 |
| `-p, --prefix` | off | 前方一致検索 |
| `-a, --all` | off | 全エントリをダンプ |

## sudachi-dict

Sudachi辞書CSVファイルからJSON辞書を生成します。デフォルトではSudachiの正規コストをそのまま使用し、`--model-scores` を指定するとjinenモデルのNLLスコアリングで候補を順序付けします。

入力となるSudachi辞書CSVは[SudachiDict](http://sudachi.s3-website-ap-northeast-1.amazonaws.com/sudachidict-raw/)からダウンロードできます。

```bash
# 基本的な使い方（Sudachiコストを使用）
cargo run --release --bin sudachi-dict -- input.csv -o scored.json

# モデルスコアリングを使用
cargo run --release --bin sudachi-dict -- input.csv --model-scores -o scored.json

# モデルとスレッド数を指定
cargo run --release --bin sudachi-dict -- input.csv --model-scores --model jinen-v1-small-q5 --threads 8
```

| オプション | デフォルト | 説明 |
|-----------|----------|------|
| `csv_files` (必須) | — | 入力Sudachi CSVファイル（複数指定可） |
| `-o, --output` | `scored.json` | 出力JSONファイル |
| `--model-scores` | off | モデルNLLスコアリングを使用（デフォルトはSudachiコスト） |
| `--model` | `jinen-v1-xsmall-q5` | モデルバリアントIDまたはGGUFファイルパス |
| `--tokenizer-json` | — | tokenizer.jsonパス（`--model` がGGUFパス時に必要） |
| `--threads` | CPUコア数 / 2 | 並列スコアリングスレッド数 |
| `--n-ctx` | `256` | モデルのコンテキストウィンドウサイズ |

出力JSONは `karukan-dict build` の入力として使用できます。

## karukan-server

ニューラルかな漢字変換を提供するHTTPサーバー。起動時にHuggingFaceからGGUFモデルを自動ダウンロードします。

### 起動

```bash
cargo run --release --bin karukan-server

# オプション
cargo run --release --bin karukan-server -- --port 8080 --host 0.0.0.0 --verbose --debug
```

| オプション | デフォルト | 説明 |
|-----------|----------|------|
| `-p, --port` | `3000` | 待ち受けポート |
| `--host` | `127.0.0.1` | バインドアドレス |
| `-v, --verbose` | off | デバッグレベルのログ出力 |
| `--debug` | off | `/api/tokenize` エンドポイントを有効化 |

### API エンドポイント

| メソッド | パス | 説明 |
|---------|------|------|
| POST | `/api/convert` | ローマ字→ひらがな変換 |
| POST | `/api/reset` | ローマ字変換器をリセット |
| POST | `/api/kanji/convert` | かな漢字変換（ビームサーチ対応） |
| GET | `/api/models` | 利用可能なモデル一覧 |
| GET | `/health` | ヘルスチェック |
| POST | `/api/tokenize` | トークナイズ（`--debug` 時のみ） |

`static/` ディレクトリからWeb UIを配信します。

## ajimee-bench

[AJIMEE-Bench](https://github.com/Ajimee-Bench/AJIMEE-Bench)によるかな漢字変換の精度評価ツール。Exact Match Rate と Character Error Rate (CER) を計算します。

```bash
# 基本的な使い方
cargo run --release --bin ajimee-bench -- evaluation_items.json

# モデルを指定して実行
cargo run --release --bin ajimee-bench -- evaluation_items.json --model jinen-v1-small-q5

# 結果をJSONに保存（サマリーのみ表示）
cargo run --release --bin ajimee-bench -- evaluation_items.json --output results.json --quiet
```

| オプション | デフォルト | 説明 |
|-----------|----------|------|
| `bench_path` (必須) | — | evaluation_items.json のパス |
| `--model` | `jinen-v1-xsmall-q5` | モデルバリアントID |
| `--gguf` | — | GGUFファイルパス（`--model` を上書き） |
| `--tokenizer-json` | — | tokenizer.jsonパス（`--gguf` 使用時に必要） |
| `--output` | — | 詳細結果の出力先JSONファイル |
| `--no-context` | off | 左コンテキストを使用しない |
| `--quiet` | off | サマリーのみ表示 |
| `--n-ctx` | `512` | コンテキストウィンドウサイズ |

## License

MIT OR Apache-2.0
