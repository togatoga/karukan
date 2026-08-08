# Configuration

設定ファイル: `~/.config/karukan-im/config.toml`（macOS: `~/Library/Application Support/com.karukan.karukan-im/config.toml`）

```toml
[conversion]
live_conversion = true          # ライブ変換を起動時に有効化（Ctrl+Shift+L で実行中も切替。既定ON）
composing_chunk_len = 30        # ライブ変換で1回のモデル変換が扱う読みの最大文字数（= 1キーあたりレイテンシの上限）
strategy = "adaptive"           # 変換ストラテジー（adaptive / light / main）
num_candidates = 9              # 変換候補数（Space押下時）
n_threads = 4                   # 推論スレッド数（0 = 全コア使用）
model = "jinen-v2-small-q5"     # メインモデル（モデルID or GGUFパス）
light_model = "jinen-v2-xsmall-q5"  # 軽量モデル（ビームサーチ・長文用）
use_context = true              # Surrounding Textを変換に使用する
max_context_length = 10         # コンテキストの最大文字数
persona = ""                    # 変換ペルソナ（例: "プログラミング"。[Conversion Persona](#conversion-persona) 参照）
short_input_threshold = 10      # ビームサーチを使うトークン数の上限
beam_width = 3                  # ビーム幅
max_latency_ms = 100            # メインモデルの許容レイテンシ（ms）。超過時は軽量モデルに自動切替（0 = 無効）
dict_path = "/path/to/dict.bin" # システム辞書パス（省略時はデータディレクトリの dict.bin。[Dictionary](dictionary.md) 参照）

[learning]
enabled = true                 # 変換学習の有効/無効
max_entries = 10000            # 学習エントリの最大数
max_surface_chars = 50         # 学習する変換結果の最大文字数
```

`model` / `light_model` に指定できるモデルIDは以下です（指定したモデルは初回利用時にHugging Faceから自動ダウンロードされます）。モデルの変更はfcitx5の再起動（macOSは `killall KarukanIME`）で反映されます。それ以外のチューニング設定は再起動なしで反映されます（[Hot Reload](#hot-reload) 参照）。

| モデルID | ベースモデル | パラメータ数 | Accuracy@1 (NFKC) |
|---------|-----------|-----------|------:|
| [`jinen-v2-small-q5`](https://huggingface.co/togatogah/jinen-v2-small.gguf)（デフォルト） | Qwen3 | 109M | 86.0% |
| [`jinen-v2-xsmall-q5`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf) | Qwen3 | 36M | 79.0% |
| [`jinen-v1.1-beta-q5`](https://huggingface.co/togatogah/jinen-v1.1-beta.gguf) | Qwen3 | 109M（beta） | 86.0% |
| [`jinen-v1-small-q5`](https://huggingface.co/togatogah/jinen-v1-small.gguf) | GPT-2 | 90M | 76.5% |
| [`jinen-v1-xsmall-q5`](https://huggingface.co/togatogah/jinen-v1-xsmall.gguf) | GPT-2 | 26M | 71.0% |

> [!NOTE]
> 上記は主要な設定項目の抜粋です。全項目の正確な既定値と説明は [`config/default.toml`](../karukan-im/core/config/default.toml) を参照してください（各設定行に日本語コメント付き）。

## Hot Reload

config.toml は保存後、**次のフォーカス切替**（ウィンドウを移る・IMEを入れ直す）で自動的に再読み込みされます。mtime を見て変化したときだけ再読込するので、変更が無ければ stat 1回で済みます。fcitx5 では `fcitx5-remote -r` や `busctl --user call org.fcitx.Fcitx5 /controller org.fcitx.Fcitx.Controller1 ReloadAddonConfig s karukan` でも即時反映できます。

チューニング設定（`persona`・`composing_chunk_len`・`beam_width`・`strategy`・`max_context_length`・`live_conversion` など）は再起動なしで反映されます。**モデル（`model` / `light_model`）・辞書（`dict_path`）・スレッド数（`n_threads`）・`[learning]` の変更は再起動が必要**です（変更を検知するとログに出ます）。`live_conversion` は設定値が変わったときだけ適用されるので、`Ctrl+Shift+L` での実行中トグルが無関係な設定変更で巻き戻ることはありません。

## Live Conversion

入力と同時にかな漢字変換の結果をプリエディットへリアルタイム表示します（Spaceを押さずに変換が進む）。`Ctrl+Shift+L` でON/OFFを切り替えられ、既定では `live_conversion = true` で有効です。

長文入力でも1キーあたりのレイテンシを一定に保つため、変換中のバッファを内部で最大 `composing_chunk_len` 文字（既定30）のチャンクに分割し、編集した箇所のチャンクだけを再変換します。チャンクは内部的な分割で、ユーザーには連続した1つのプリエディットとして見えます。記号・数字の連続は日本語とは別チャンクに分けてそのまま通すため、`123456` のような並びが変換で崩れることはありません。

## Conversion Persona

`persona` によく書く話題のキーワード（例: `プログラミング`。英語表記を優先したければ `programming` のような英単語）を設定すると、モデルへ渡す左コンテキストの先頭にそのまま連結され（`{persona}{文脈}`）、それに合わせた変換が出やすくなります。ライブ変換・Space変換の両方に適用され、適用中は aux のモード表示に実効値が表示されます（例: `⚡[あ]P:プログラミング`）。末尾25文字まで使用（10〜20文字推奨）。空（既定）で無効です。

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
