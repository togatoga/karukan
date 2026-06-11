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

## インストール

### 初回インストール

```bash
cd karukan-macos

# ビルド + .appバンドル組み立て + ~/Library/Input Methods へインストール
make install
```

その後、以下の手順で利用できます:

1. macOSからログアウトし、再ログイン(macOSが新しいIMEを認識するために必要)
2. 「システム設定」>「キーボード」>「入力ソース」を編集 >「+」ボタン>「日本語」> **Karukan** を追加
3. メニューバーの入力メニューからKarukanを選択

> **Note:** `make install` 実行時に変換モデル(`models.toml` に定義された全モデル)を
> Hugging Faceからダウンロードしてキャッシュ(`~/.cache/huggingface/`)に配置します。
> オフラインなどでダウンロードに失敗した場合は初回の変換開始時に自動でダウンロード
> されますが、完了まで変換候補が出るのに時間がかかることがあります。

### 開発版の更新(2回目以降)

ログアウトは不要で、この2コマンドだけで反映されます:

```bash
make install
killall KarukanIME  # 次にテキストフィールドへフォーカスした時に自動で再起動
```

ただし**コード以外のメタデータ**(メニューアイコン、入力モード名、Info.plistの
入力モード定義)はmacOS側にキャッシュされるため、`killall KarukanIME` では反映され
ないことがあります。その場合は次のいずれかで反映されます:

```bash
# 入力メニューのアイコン・名前のキャッシュを更新(エージェントは自動再起動)
killall TextInputMenuAgent
```

- それでも反映されない場合: システム設定 → 入力ソースからKarukanを削除 → 再追加
- 最終手段: ログアウト → ログイン

### テスト

```bash
# Swift + Rustサーバー統合テスト
make test
```

## システム辞書のインストール

モデル推論だけでは語彙が限られるため、システム辞書の併用を強く推奨します。
`make install` 実行時に、辞書が未インストールであればGitHubリリースのビルド済み辞書
(`dict.tgz`)を自動でダウンロードして配置します。既に
`~/Library/Application Support/com.karukan.karukan-im/dict.bin` がある場合は
何もしません(自前ビルドの辞書が上書きされることはありません)。

手動でインストールする場合:

```bash
curl -LO https://github.com/togatoga/karukan/releases/latest/download/dict.tgz
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

## 参考プロジェクト

- [mac-akaza](https://github.com/akaza-im/mac-akaza)
