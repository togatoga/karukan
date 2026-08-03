# karukan-im

Karukan の IME 本体。共有エンジンと各プラットフォームのフロントエンドがここにまとまっています。

| ディレクトリ | 説明 |
|---------|------|
| [core/](core/) | 共有IMEエンジン(crate: `karukan-im`) — ステートマシン、ローマ字変換、karukan-imserver(macOS向けJSON-RPCサーバー) |
| [fcitx5/](fcitx5/) | Linux向けフロントエンド(crate: `karukan-fcitx5`) — fcitx5アドオン + C FFI |
| [macos/](macos/) | macOS向けフロントエンド — Swift/InputMethodKit |

キーバインドは [docs/key-bindings.md](../docs/key-bindings.md) を参照してください。設定・辞書・学習キャッシュなどエンジンの機能説明は [core/README.md](core/README.md) を、インストール手順は各フロントエンドの README を参照してください。
