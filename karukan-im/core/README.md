# karukan-im

fcitx5（Linux）および macOS Swift フロントエンドで共有される日本語IMEエンジン。ローマ字→ひらがな変換、ニューラルかな漢字変換（GPT-2/Qwen3ベース）、学習キャッシュ、システム辞書を提供します。

フロントエンドのインストール手順:
- Linux (fcitx5): [karukan-fcitx5](../fcitx5/README.md)
- macOS: [karukan-macos](../macos/README.md)

## Features

- ニューラルかな漢字変換（llama.cppによるGGUF推論）
- 変換学習（ユーザーの変換履歴を記憶し、完全一致・前方一致で候補を優先表示）
- 日本語・英数字の混合入力（Shift切り替え）
- Surrounding Textによる文脈を考慮した変換
- システム辞書・ユーザー辞書による候補補完

> [!NOTE]
> モデル推論だけでは語彙が限られるため、システム辞書の併用を強く推奨します。システム辞書はIMEに同梱されていないため、別途インストールが必要です。詳しくは [docs/dictionary.md](../../docs/dictionary.md) を参照してください。

## Documentation

ユーザー向けドキュメントは [docs/](../../docs/) にまとまっています:

- [キーバインド一覧](../../docs/key-bindings.md) — 共通キーバインドと Linux / macOS 固有キー
- [設定](../../docs/configuration.md) — config.toml の設定項目、ライブ変換、変換ストラテジー、学習キャッシュ
- [辞書](../../docs/dictionary.md) — システム辞書のインストール、ユーザー辞書、候補の優先順位

設定項目の正確な既定値と説明は [`config/default.toml`](config/default.toml) を参照してください（各設定行に日本語コメント付き）。

## Development

- エンジン本体: `src/core/engine/` — Empty → Composing → Conversion のステートマシン
- macOS向けJSON-RPCサーバー: `src/server/` + `src/bin/karukan-imserver.rs`

```bash
cargo build -p karukan-im --release
cargo test -p karukan-im
```
