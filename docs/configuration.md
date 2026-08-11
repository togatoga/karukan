# Configuration

設定ファイル: `~/.config/karukan-im/config.toml`（macOS: `~/Library/Application Support/com.karukan.karukan-im/config.toml`）

```toml
[conversion]
live_conversion = true          # ライブ変換を起動時に有効化（Ctrl+Shift+L で実行中も切替。既定ON）
chunk_chars = 30                # 一度にAI変換する Chunk の最大文字数（[Chunk](chunking.md) 参照）
chunk_symbols = 1               # Chunk に残せる記号（、。！？など）の数
chunk_digits = 0                # Chunk に残せる数字の桁数（0 = 数字はAI変換にかけない）
strategy = "adaptive"           # 変換ストラテジー（adaptive / light / main）
num_candidates = 9              # 変換候補数（Space押下時）
n_threads = 4                   # 推論スレッド数（0 = 全コア使用）
model = "jinen-v2-small-q5"     # メインモデル（モデルID or GGUFパス）
light_model = "jinen-v2-xsmall-q5"  # 軽量モデル（ビームサーチ・長文用）
use_context = true              # Surrounding Textを変換に使用する
context_chars = 10          # 変換に使う前後テキストの最大文字数
beam_chars = 30                 # 別候補を出す範囲の文字数（Chunk単位で後ろからまとめる）
beam_width = 3                  # 別候補の本数
max_latency_ms = 100            # メインモデルの許容レイテンシ（ms）。超過時は軽量モデルに自動切替（0 = 無効）
dict_path = "/path/to/dict.bin" # システム辞書パス（省略時はデータディレクトリの dict.bin。[Dictionary](dictionary.md) 参照）

[learning]
enabled = true                 # 変換学習の有効/無効
max_entries = 10000            # 学習エントリの最大数
max_surface_chars = 50         # 学習する変換結果の最大文字数
```

`model` / `light_model` に指定できるモデルIDは以下です（指定したモデルは初回利用時にHugging Faceから自動ダウンロードされます）。設定変更後はfcitx5の再起動（macOSは `killall KarukanIME`）で反映されます。

| モデルID | ベースモデル | パラメータ数 | Accuracy@1 (NFKC) |
|---------|-----------|-----------|------:|
| [`jinen-v2-small-q5`](https://huggingface.co/togatogah/jinen-v2-small.gguf)（デフォルト） | Qwen3 | 109M | 86.0% |
| [`jinen-v2-xsmall-q5`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf) | Qwen3 | 36M | 79.0% |
| [`jinen-v1.1-beta-q5`](https://huggingface.co/togatogah/jinen-v1.1-beta.gguf) | Qwen3 | 109M（beta） | 86.0% |
| [`jinen-v1-small-q5`](https://huggingface.co/togatogah/jinen-v1-small.gguf) | GPT-2 | 90M | 76.5% |
| [`jinen-v1-xsmall-q5`](https://huggingface.co/togatogah/jinen-v1-xsmall.gguf) | GPT-2 | 26M | 71.0% |

> [!NOTE]
> 上記は主要な設定項目の抜粋です。全項目の正確な既定値と説明は [`config/default.toml`](../karukan-im/core/config/default.toml) を参照してください（各設定行に日本語コメント付き）。

## Live Conversion

入力と同時にかな漢字変換の結果をプリエディットへリアルタイム表示します（Spaceを押さずに変換が進む）。`Ctrl+Shift+L` でON/OFFを切り替えられ、既定では `live_conversion = true` で有効です。

打っている途中の文が長くなっても待ち時間が伸びないよう、変換は一定の長さごとの Chunk に区切って行われます。Chunk の決まり方、表示のちらつきを止める手動区切り、`chunk_*` の調整方法は [Chunk](chunking.md) を参照してください。

## 詳細表示（verbose）

```toml
[display]
verbose = false                 # 補助テキストに詳細を出す（Ctrl+Shift+V で切替）
```


補助テキストは既定では静かな表示で、変換に必要な情報だけが出ます。状態、読み、選択中の候補がどこから来たか、ページ番号などです。

開発や調整で内部の様子を見たいときは `Ctrl+Shift+V` で詳細表示に切り替えられます。押した時点で表示が切り替わります（起動時から有効にするなら `[display] verbose = true`）。詳細表示では次が加わります。

| 項目 | 例 | 読み方 |
|------|-----|--------|
| ビームサーチの対象 | `🎯 うえ 2/30` | 🎯 の後ろ（`うえ`）だけが別候補を持つ。それより前は表示中の変換のまま。`2/30` は対象の文字数と `beam_chars` |
| 推論時間 | `推論: 41ms key: 45ms` | モデル呼び出しにかかった時間 / その打鍵の処理全体。キャッシュに当たれば推論は `0ms` |
| モデル名 | `jinen-v2-small-q5` | その変換を実際に担当したモデル |
| モデルに渡した文脈 | `lctx: 昨日は` | 変換時に前方の文脈として渡した文字列 |

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
