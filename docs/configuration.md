# Configuration

設定ファイル: `~/.config/karukan-im/config.toml`（macOS: `~/Library/Application Support/com.karukan.karukan-im/config.toml`）

```toml
[conversion]
live_conversion = true          # ライブ変換を起動時に有効化（Ctrl+Shift+L で実行中も切替。既定ON）
chunk_chars = 30                # 一度にAI変換する Chunk の最大文字数（[Chunk](chunking.md) 参照）
chunk_symbols = 1               # Chunk に残せる記号（、。！？など）の数
chunk_digits = 0                # Chunk に残せる数字の桁数（0 = 数字はAI変換にかけない）
chunk_alphabets = 0             # Chunk に残せる英字の数（0 = 英字はAI変換にかけない）
strategy = "adaptive"           # 変換ストラテジー（adaptive / light / main）
num_candidates = 9              # 変換候補数（Space押下時）
n_threads = 4                   # 推論スレッド数（0 = 全コア使用）
model = "jinen-v2-small-q5"     # メインモデル（[models] のキーを指定）
light_model = "jinen-v2-xsmall-q5"  # 軽量モデル（ビームサーチ・長文用。[models] のキーを指定）
use_context = true              # Surrounding Textを変換に使用する
context_chars = 10              # 変換に使う前後テキストの最大文字数
beam_chars = 30                 # 別候補を出す範囲の文字数（Chunk単位で後ろからまとめる）
beam_width = 3                  # 別候補の本数
max_latency_ms = 100            # メインモデルの許容レイテンシ（ms）。超過時は軽量モデルに自動切替（0 = 無効）
dict_path = "/path/to/dict.bin" # システム辞書パス（省略時はデータディレクトリの dict.bin。[Dictionary](dictionary.md) 参照）

[learning]
enabled = true                 # 変換学習の有効/無効
max_entries = 10000            # 学習エントリの最大数
max_surface_chars = 50         # 学習する変換結果の最大文字数

[symbol]                       # どの記号を打つか（[記号・半角全角](symbols.md) 参照）
punctuation = "、。"            # 「,」「.」キーが入力する句読点（"，．" / "、．" / "，。"）
bracket = "「」"                # 「[」「]」キーが入力する括弧（"[]"）
slash = "・"                    # 「/」キーが入力する記号（"/"）
space = "half"                 # かな入力中のスペースキーが入力する空白（"full" / "half"）

[width]                        # かな入力中の出力幅（"half" / "full"）
kana_symbol = "full"             # 。、「」・
ascii_symbol = "full"          # ?! ,. (){}[] @ : ~ ほかの記号
digit = "half"                 # 0-9
```

> [!NOTE]
> 上記は主要な設定項目の抜粋です。全項目の正確な既定値と説明は [`config/default.toml`](../karukan-im/core/config/default.toml) を参照してください（各設定行に日本語コメント付き）。

## モデルの定義（[models]）

変換モデルは `[models]` テーブルで定義し、`model` / `light_model` はそのキーを参照します。各エントリは次のどちらか一方を指定します。

- `repo` + `filename`: Hugging Face のリポジトリとGGUFファイル名。初回起動時にバックグラウンドで自動ダウンロードされます。`tokenizer.json` は同じリポジトリから読み込みます
- `path`: ローカルのGGUFファイルのパス。`tokenizer.json` はGGUFと同じディレクトリに置きます

既定で以下の5モデルが定義済みです（ユーザーの `config.toml` の `[models]` はキー単位でマージされ、同じキーは上書き、既定のエントリはそのまま残ります）。

| モデルキー | ベースモデル | パラメータ数 | Accuracy@1 (NFKC) |
|---------|-----------|-----------|------:|
| [`jinen-v2-small-q5`](https://huggingface.co/togatogah/jinen-v2-small.gguf)（デフォルト） | Qwen3 | 109M | 86.0% |
| [`jinen-v2-xsmall-q5`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf) | Qwen3 | 36M | 79.0% |
| [`jinen-v1.1-beta-q5`](https://huggingface.co/togatogah/jinen-v1.1-beta.gguf) | Qwen3 | 109M（beta） | 86.0% |
| [`jinen-v1-small-q5`](https://huggingface.co/togatogah/jinen-v1-small.gguf) | GPT-2 | 90M | 76.5% |
| [`jinen-v1-xsmall-q5`](https://huggingface.co/togatogah/jinen-v1-xsmall.gguf) | GPT-2 | 26M | 71.0% |

自作モデルを使うには `[models]` にエントリを追加して `model` から参照します。

```toml
[conversion]
model = "my-model"

# ローカルのGGUFを使う（tokenizer.json を同じディレクトリに置く）
[models.my-model]
path = "/home/user/models/my-model.gguf"

# Hugging Faceのリポジトリから使う
[models.my-hf-model]
repo = "owner/my-model.gguf"
filename = "my-model-Q5_K_M.gguf"
```

設定変更後はfcitx5の再起動（macOSは `killall KarukanIME`）で反映されます。

## Live Conversion

入力と同時にかな漢字変換の結果をプリエディットへリアルタイム表示します（Spaceを押さずに変換が進む）。`Ctrl+Shift+L` でON/OFFを切り替えられ、既定では `live_conversion = true` で有効です。

入力中の文が長くなっても変換時間が伸びないよう、変換は一定の長さごとの Chunk に区切って実行されます。Chunk の決まり方、表示のちらつきを止める手動区切り、`chunk_*` の調整方法は [Chunk](chunking.md) を参照してください。

## 記号・半角全角

`[symbol]` はキーが入力する記号を選びます（句読点 `、。` / `，．`、括弧 `「」` / `[]`、`/` キーの `・` / `/`、スペースの全角・半角）。`[width]` は出力される文字の幅を、かなと組で使う記号（`。、「」・`）・それ以外の記号・数字の3種類について `"half"` か `"full"` で指定します。効くのはかな入力中だけで、英字モード（Shift+英字）と絵文字モードでは打ったとおりの半角が出ます。

既定はかな入力が全角、数字だけ半角です（`(a)` → `（あ）`、`heya123` → `へや123`）。

既定はかな入力が全角、数字だけ半角です（`(a)` → `（あ）`、`heya123` → `へや123`）。切り替えたときに何がどう変わるか、「記号はすべて半角」のような設定例は [記号・半角全角](symbols.md) を参照してください。

## 詳細表示（verbose）

```toml
[display]
verbose = false                 # 補助テキストに詳細を出す（Ctrl+Shift+V で切替）
```

補助テキストには既定では、変換に必要な情報だけが表示されます。入力の状態、読み、選択中の候補がどこから来たか、候補のページ番号です。

開発や設定の調整で内部の動きを確認したいときは、`Ctrl+Shift+V` で詳細表示に切り替えます。押した時点で表示が変わります（起動時から有効にするには `[display] verbose = true`）。詳細表示では次の情報が加わります。

| 項目 | 例 | 読み方 |
|------|-----|--------|
| ビームサーチの対象 | `🎯 うえ 2/30` | 🎯 の後ろ（`うえ`）にだけ別候補が出る。それより前は表示中の変換のまま。`2/30` は対象の文字数と `beam_chars` の値 |
| 推論時間 | `推論: 41ms key: 45ms` | モデルの呼び出しにかかった時間と、その打鍵の処理全体にかかった時間。キャッシュに当たった場合、推論は `0ms` になる |
| モデル名 | `jinen-v2-small-Q5_K_M` | その変換を実行したモデル（GGUFファイル名） |
| モデルに渡した文脈 | `lctx: 昨日は` | 変換時に前方の文脈としてモデルへ渡した文字列 |

## Conversion Strategy

`strategy` で変換時のモデル使い分けを制御できます。

| 値 | 説明 | 読み込むモデル |
|---|---|---|
| `adaptive` | デフォルト。レイテンシに応じてメイン・軽量モデルを動的に切り替え | メイン + 軽量 |
| `light` | 軽量モデルのみ使用。メモリ消費が少なく、低スペックPCにおすすめ | 軽量のみ |
| `main` | メインモデルのみ使用（ビームサーチなし） | メインのみ |

低スペックのPC（メモリが少ない、CPUが遅い等）では `strategy = "light"` を設定すると、軽量モデル1つだけで動作するためメモリ使用量が削減され、レスポンスも安定します。

```toml
[conversion]
strategy = "light"
```

## Performance Tuning

CPU高負荷時（Rustビルド中など）にかな漢字変換が遅くなる場合は、`n_threads` を小さくするとレスポンスが改善します。

## Learning Cache

ユーザーが選択した変換結果を記憶し、次回以降の変換で優先表示します。

- 保存先: `~/.local/share/karukan-im/learning.tsv`（macOS: `~/Library/Application Support/com.karukan.karukan-im/learning.tsv`）
- 完全一致と前方一致（予測変換）の両方に対応
  - 例: 「早稲田大学」を一度変換すると、次回「わせだ」と入力した時点で候補に表示
- 学習候補は変換時・入力中（auto-suggest）の両方で最大3件表示
- スコアはrecency（最終使用日時）重視 + 頻度補正
- 50文字（`max_surface_chars`）を超える変換結果は学習しない
- 変換中に学習候補（📝）を選択して `Ctrl+Backspace`（macOSでは Ctrl+delete。`Ctrl+Delete` でも可）を押すと、そのエントリを学習履歴から削除できる。学習候補の選択中はフッターに「Ctrl+Backspaceで履歴から削除」と表示される
- IME切り替え・ウィンドウ切り替え時に自動保存（commit のたびには保存しない）
- `[learning] enabled = false` で無効化可能
- 学習履歴をすべて削除するには: `rm ~/.local/share/karukan-im/learning.tsv`
