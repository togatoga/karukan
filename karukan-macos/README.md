# karukan-macos

macOS IMKit 向け日本語入力メソッド

## ビルド

```bash
cd karukan-macos
scripts/build.sh
```

ビルド成果物は `karukan-macos/build/Karukan.app` に生成されます。

## インストール

```bash
scripts/install.sh
```

`~/Library/Input Methods/Karukan.app` にコピーされます。

> **重要:** macOS は `~/Library/Input Methods/` と `/Library/Input Methods/` のみを入力メソッドとして認識します。`/Applications/` に配置しても動作しません。

## 有効化

1. ログアウトしてログインし直す（または再起動）
2. System Settings > Keyboard > Input Sources > Edit > +
3. Japanese の下にある「Karukan」を追加

ログアウトせずに試す場合:

```bash
killall KarukanInputMethod 2>/dev/null
open ~/Library/Input\ Methods/Karukan.app
```

## 必要環境

- macOS 14.0 以上
- Rust toolchain
- Xcode Command Line Tools（swiftc が必要）
