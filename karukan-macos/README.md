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

ただし**コード以外のメタデータ**(メニューアイコン、入力モード名、Info.plistの
入力モード定義)はmacOS側にキャッシュされるため、`killall KarukanIME` では反映され
ないことがあります。その場合は次のいずれかで反映されます:

```bash
# 入力メニューのアイコン・名前のキャッシュを更新(エージェントは自動再起動)
killall TextInputMenuAgent
```

- それでも反映されない場合: システム設定 → 入力ソースからKarukanを削除 → 再追加
- 最終手段: ログアウト → ログイン

## システム辞書のインストール

モデル推論だけでは語彙が限られるため、システム辞書の併用を強く推奨します。
システム辞書は.appに同梱されていないため、ビルド済みの辞書をダウンロードして配置してください:

```bash
curl -LO https://github.com/togatoga/karukan/releases/download/v0.1.0/dict.tgz
tar xzf dict.tgz
mkdir -p ~/Library/"Application Support"/com.karukan.karukan-im
cp dict.bin ~/Library/"Application Support"/com.karukan.karukan-im/
killall KarukanIME  # 起動中の場合は再起動して反映
```

辞書を自分でビルドする場合は [karukan-cli の README](../karukan-cli/README.md) を参照してください。

## キー操作

fcitx5版と同じキーバインドに加えて:

| キー | 動作 |
|------|------|
| かな (JIS) | 日本語モードへ切替 |
| 英数 (JIS) | 変換中テキストを確定して直接入力モードへ切替 |
| 右⌘ 単独タップ | 日本語入力(ひらがな)へ戻る。直接入力モード、Shift+英字で入った英字モードのどちらからでも有効。⌘C など他のキーと組み合わせた場合は発動しない(Linux版の右Superに相当) |

## 設定・データファイル

`directories`クレートのmacOS既定パスを使用します:

- 設定: `~/Library/Application Support/com.karukan.karukan-im/config.toml`
- システム辞書: `~/Library/Application Support/com.karukan.karukan-im/dict.bin`
- ユーザー辞書: `~/Library/Application Support/com.karukan.karukan-im/user_dicts/`
- 学習データ: `~/Library/Application Support/com.karukan.karukan-im/learning.tsv`

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
