# karukan-macos

macOS向けKarukan日本語入力(InputMethodKit + Swift)。

## アーキテクチャ

サーバー・クライアント構成です。IMEの状態機械・ローマ字変換・かな漢字変換はすべて
Rust側(`karukan-im`)にあり、Swift側はInputMethodKitとの橋渡しに徹します。

```
┌───────────────────────────────┐                            ┌─────────────────────────────┐
│ KarukanIME (Swift)            │  JSON-RPC 2.0 (改行区切り)  │ karukan-imserver (Rust)     │
│ ・IMKServer / InputController │ ◀────────────────────────▶ │ ・InputMethodEngine         │
│ ・NSEvent → XKB keysym変換    │    stdin/stdout パイプ      │ ・ローマ字変換・状態機械    │
│ ・preedit / 候補ウィンドウ描画 │                            │ ・llama.cpp推論・辞書・学習 │
│ ・子プロセス管理・自動再起動   │                            │                             │
└───────────────────────────────┘                            └─────────────────────────────┘
        どちらも Karukan.app バンドル内 (Contents/MacOS/)
```

- プロトコル定義: `karukan-im/src/server/protocol.rs`(Rust側が正)と
  `Sources/KarukanIME/EngineProtocol.swift`(Swift側ミラー)
- キーイベントはfcitx5版と同じXKB keysym表現に変換して送る
  (`Sources/KarukanIME/KeyCodeMap.swift`)
- エンジンプロセスはクラッシュ時に指数バックオフで自動再起動、
  スリープ復帰時にも再起動(macOSがスリープ中にパイプを破棄するため)

## ビルドとインストール

```bash
cd karukan-macos

# ビルド + .appバンドル組み立て + ~/Library/Input Methods へインストール
make install

# テスト (Swift + Rustサーバー統合テスト)
make test
```

インストール後:

1. **初回のみ**: ログアウト → ログイン(macOSが新しいIMEを認識するために必要)
2. システム設定 → キーボード → 入力ソース → 「+」→ 日本語 → **Karukan** を追加
3. 入力メニューからKarukanを選択

> **Note:** 初回の変換開始時にHugging Faceからモデルをダウンロードします。
> ダウンロード完了まで変換候補が出るまで時間がかかることがあります。

2回目以降の更新は `make install` だけでよく、`killall KarukanIME` で再読み込みされます
(次にテキストフィールドへフォーカスした時にmacOSが自動で再起動します)。

## キー操作

fcitx5版と同じキーバインドに加えて:

| キー | 動作 |
|------|------|
| かな (JIS) | 日本語モードへ切替 |
| 英数 (JIS) | 変換中テキストを確定して直接入力モードへ切替 |

## 設定・データファイル

`directories`クレートのmacOS既定パスを使用します:

- 設定: `~/Library/Application Support/com.karukan.karukan-im/config.toml`
- 学習データ・辞書: `~/Library/Application Support/com.karukan.karukan-im/`

## デバッグ

- ログ: `~/Library/Logs/KarukanIME/karukan-ime.log`(Swift側NSLogとRust側tracingの両方)
- サーバー単体デバッグ: JSON-RPCを直接流せます

  ```bash
  cargo run -p karukan-im --bin karukan-imserver
  {"jsonrpc":"2.0","id":1,"method":"process_key","params":{"keysym":107}}
  ```

- バンドルを組み立てずに開発中のサーバーを使う: `KARUKAN_IMSERVER=/path/to/karukan-imserver`

## 既知の制約

- ローマ字入力のみ対応(かな入力レイアウトは未対応)
- 候補ウィンドウはマウス操作不可(数字キー・矢印キーで選択)
