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
model = "jinen-v2-small-q8"     # メインモデル（[models] のキーを指定）
light_model = "jinen-v2-xsmall-q8"  # 軽量モデル（ビームサーチ・長文用。[models] のキーを指定）
use_context = true              # Surrounding Textを変換に使用する
context_chars = 10              # 変換に使う前後テキストの最大文字数
persona = ""                    # 変換ペルソナ（例: "プログラミング"。[変換ペルソナ](persona.md) 参照）
beam_chars = 30                 # 別候補を出す範囲の文字数（Chunk単位で後ろからまとめる）
beam_width = 3                  # 別候補の本数
max_latency_ms = 100            # メインモデルの許容レイテンシ（ms）。超過時は軽量モデルに自動切替（0 = 無効）
timeout_ms = 500                # AI変換 1 回の制限時間（ms）。超えたら打ち切り、ひらがなと辞書で入力を続ける（0 = 無効。[PC が重いとき](#pc-が重いとき) 参照）
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

[date]                         # 日付・時刻の変換（[日付・時刻](date.md) 参照）
formats = ["{YEAR}/{MONTH}/{DATE}", "{WEEKDAY}曜日"]  # 共有フォーマット
phrases = [                    # フレーズ一覧（書くと既定一覧ごと置き換え）
    { reading = "きょう", offset_days = 0 },
    { reading = "いま", formats = ["{HOUR}:{MINUTE}"] },
]
```

> [!NOTE]
> 上記は主要な設定項目の抜粋です。全項目の正確な既定値と説明は [`config/default.toml`](../karukan-im/core/config/default.toml) を参照してください（各設定行に日本語コメント付き）。

## モデルの定義（[models]）

変換モデルは `[models]` テーブルで定義し、`model` / `light_model` はそのキーを参照します。値は 2 通りです。

- 文字列: ローカルの GGUF ファイルのパス。`tokenizer.json` は GGUF と同じディレクトリに置きます
- `{ repo = "...", filename = "..." }`: Hugging Face のリポジトリと GGUF ファイル名。初回起動時にバックグラウンドで自動ダウンロードされます。`tokenizer.json` は同じリポジトリから読み込みます

既定で以下の6モデルが定義済みです（ユーザーの `config.toml` の `[models]` はキー単位でマージされ、同じキーは上書き、既定のエントリはそのまま残ります）。

| モデルキー | ベースモデル | パラメータ数 | Accuracy@1 (NFKC) |
|---------|-----------|-----------|------:|
| [`jinen-v2-small-q8`](https://huggingface.co/togatogah/jinen-v2-small.gguf)（デフォルト） | Qwen3 | 109M | 86.5% |
| [`jinen-v2-xsmall-q8`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf)（軽量モデルのデフォルト） | Qwen3 | 36M | 79.0% |
| [`jinen-v2-small-q5`](https://huggingface.co/togatogah/jinen-v2-small.gguf) | Qwen3 | 109M | 86.0% |
| [`jinen-v2-xsmall-q5`](https://huggingface.co/togatogah/jinen-v2-xsmall.gguf) | Qwen3 | 36M | 79.0% |
| [`jinen-v1-small-q5`](https://huggingface.co/togatogah/jinen-v1-small.gguf) | GPT-2 | 90M | 76.5% |
| [`jinen-v1-xsmall-q5`](https://huggingface.co/togatogah/jinen-v1-xsmall.gguf) | GPT-2 | 26M | 71.0% |

`-q8` は Q8_0、`-q5` は Q5_K_M で量子化したものです。Q8_0 はファイルが大きい（small で 117MB と 81MB、xsmall で 39MB と 28MB）代わりに、複数のトークンをまとめて評価するときの計算が軽く、打鍵ごとの変換も Space での変換も速くなります。メモリやダウンロードの量を抑えたい場合は `-q5` を指定してください。

自作モデルを使うには `[models]` にエントリを追加して `model` から参照します。

```toml
[conversion]
model = "jinen-v2-small-q4"

[models]
jinen-v2-small-q4 = { repo = "togatogah/jinen-v2-small.gguf", filename = "jinen-v2-small-Q4_K_M.gguf" }
my-model = "/home/user/models/my-model.gguf"
```

既定のモデルを差し替えるには同じキーに書きます（`jinen-v2-small-q8 = "/path/to/model.gguf"`）。不正なエントリは起動時のログと、そのモデルを使うときのエラーに出ます。

モデルの変更（`model` / `light_model` / `strategy` / `n_threads` / `[models]`）も再起動なしで反映されます。新しいモデルは裏で読み込まれ（初めて使うモデルはダウンロードも）、読み込みが終わるまではそれまでのモデルが変換を続けます。読み込みに失敗した場合もそれまでのモデルのままです。

## 設定の反映

`config.toml` のすべての項目と [`romaji.tsv`](romaji-table.md) は、保存すると**次に打ち始めた言葉から**反映されます。fcitx5 の再起動も `killall KarukanIME` も要りません。入力中（未確定の文字がある間）は切り替わらず、その入力が確定かキャンセルで終わってから反映されます。

- `dict_path` を変えるとシステム辞書を読み直し、`[learning]` を変えると学習履歴をいったん保存して新しい上限で読み直します（`enabled = false` にすると保存して学習を止めます）
- 書きかけなどで `config.toml` が読めないときは、それまでの設定のまま動き続けます（ログに理由が出ます）。直して保存すれば反映されます
- `live_conversion` と `verbose` は、実行中に `Ctrl+Shift+L` / `Ctrl+Shift+V` で切り替えた状態を保ちます。設定ファイルのその値を書き換えたときだけ、書き換えた値になります
- ユーザー辞書（`user_dicts/`）や `dict.bin` の中身の差し替えは設定の変更ではないので、これまでどおり再起動（fcitx5 の再起動、macOS は `killall KarukanIME`）で反映されます

## Live Conversion

入力と同時にかな漢字変換の結果をプリエディットへリアルタイム表示します（Spaceを押さずに変換が進む）。`Ctrl+Shift+L` でON/OFFを切り替えられ、既定では `live_conversion = true` で有効です。

入力中の文が長くなっても変換時間が伸びないよう、変換は一定の長さごとの Chunk に区切って実行されます。Chunk の決まり方、表示のちらつきを止める手動区切り、`chunk_*` の調整方法は [Chunk](chunking.md) を参照してください。

## 記号・半角全角

`[symbol]` はキーが入力する記号を選びます（句読点 `、。` / `，．`、括弧 `「」` / `[]`、`/` キーの `・` / `/`、スペースの全角・半角）。`[width]` は出力される文字の幅を、かなと組で使う記号（`。、「」・`）・それ以外の記号・数字の3種類について `"half"` か `"full"` で指定します。効くのはかな入力中だけで、英字モード（Shift+英字）と絵文字モードでは打ったとおりの半角が出ます。

既定はかな入力が全角、数字だけ半角です（`(a)` → `（あ）`、`heya123` → `へや123`）。

既定はかな入力が全角、数字だけ半角です（`(a)` → `（あ）`、`heya123` → `へや123`）。切り替えたときに何がどう変わるか、「記号はすべて半角」のような設定例は [記号・半角全角](symbols.md) を参照してください。

## 日付・時刻

「きょう」「あした」「いま」などを変換すると日付・時刻の候補が出ます（`きょう` → `2026/09/06`・`令和8年9月6日`・`日曜日` など）。フレーズは `[date]` の `phrases` に読みと `offset_days`（今日±何日）で登録し、出力形式は `{YEAR}年{MONTH:bare}月` のようなフォーマットで指定します（`:bare` でゼロ埋めなし、`:kanji` で漢数字）。既定フレーズ・フォーマット一覧と設定例は [日付・時刻](date.md) を参照してください。

## 候補ウィンドウ

```toml
[display]
candidate_window = "always"     # "conversion" にすると Space で変換を始めるまで開かない
```

既定では入力中から候補ウィンドウが開き、学習・辞書・AI の候補を Ctrl+1〜9 で選べます。`"conversion"` にすると入力中は開かず、Space で変換を始めたときだけ開きます。補助テキストも候補ウィンドウの一部なので一緒に出なくなります。絵文字モードの候補は設定に関わらず表示されます。

## 詳細表示（verbose）

```toml
[display]
verbose = false                 # 補助テキストに詳細を出す（Ctrl+Shift+V で切替）
```

補助テキストには既定では、変換に必要な情報だけが表示されます。入力モード、入力の状態、編集中の Chunk の読みと文字数カウンタ、選択中の候補がどこから来たか、候補のページ番号です。入力中と変換中で同じ形式です（[Chunking](chunking.md) 参照）。

開発や設定の調整で内部の動きを確認したいときは、`Ctrl+Shift+V` で詳細表示に切り替えます。押した時点で表示が変わります（起動時から有効にするには `[display] verbose = true`）。詳細表示では次の情報が加わります。

| 項目 | 例 | 読み方 |
|------|-----|--------|
| ビームサーチの対象 | `🎯 うえ 2/30` | 変換中の読みの表示が入れ替わる。🎯 の後ろ（`うえ`）にだけ別候補が出る。それより前は表示中の変換のまま。`2/30` は対象の文字数と `beam_chars` の値 |
| 推論時間 | `推論: 41ms key: 45ms` | モデルの呼び出しにかかった時間と、その打鍵の処理全体にかかった時間。キャッシュに当たった場合、推論は `0ms` になる |
| モデル名 | `jinen-v2-small-q8` | 表示中の候補を出したモデル（`[models]` のキー） |
| モデルに渡した文脈 | `lctx: 昨日は` | 変換時に前方の文脈としてモデルへ渡した文字列（[ペルソナ](persona.md) は含まない）。ターミナルのインデントのような空白だけの文脈は捨てるので表示されない |
| ペルソナ | `⚡[あ]P:プログラミング` | モード表示に、モデルへ渡している `persona` の実効値が付く |

## 変換ペルソナ

`persona` によく書く文章と関連のあるキーワードを書くと、それに寄せた変換が出やすくなります（`persona = "プログラミング"` で `くろーど` → `Claude`、`らすと` → `Rust`）。書き方と詳細表示での確認方法は [変換ペルソナ](persona.md) を参照してください。

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

## PC が重いとき

ゲームやビルドなどで PC が重く、AI変換が打鍵に追いつかないときでも、入力そのものは止まりません。AI変換 1 回には制限時間（`timeout_ms`、既定 500ms）があり、超えた変換はその打鍵では打ち切られます。読みはひらがなのまま表示され、学習・辞書の候補はこれまでどおり出ます。Space を押せば辞書の候補から選べます。

時間切れになったモデルはしばらく休みます。

| 状況 | 動作 |
|------|------|
| メインモデルが時間切れ | メインモデルを休ませ、軽量モデルで変換を続ける |
| 軽量モデルも時間切れ | AI変換を休み、ひらがなと辞書の候補で入力を続ける。補助テキストのモード表示が `⚡[あ]` から `💤[あ]` に変わる |
| 休みが明けた | 次の変換で 1 回だけ試す。間に合えば元に戻り、また時間切れなら休みを倍にする |

休む時間は 2 秒から始まり、続けて時間切れになるたびに倍になります（最長 60 秒）。休んでいる間も、すでに変換済みの部分はそのまま表示されます。

Space での変換は打鍵ごとの変換より時間がかかるので、`timeout_ms` の 4 倍まで待ちます。

```toml
[conversion]
timeout_ms = 500   # 0 にすると制限なし（これまでの動作）
```

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
