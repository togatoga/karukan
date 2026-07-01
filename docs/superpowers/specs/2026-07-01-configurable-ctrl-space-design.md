# Ctrl+Space の全角スペース挙動を config.toml で切替可能にする

## 背景と問題

Karukan は Ctrl+Space を「全角スペース入力」として意図的に横取りしている。

- 未入力時 (Empty): `karukan-im/src/core/engine/input.rs` の `process_key_empty` で、Ctrl+Space を全角スペース (U+3000) の Composing セッション開始として消費する。
- 入力中 (Composing): 同ファイル `process_key_composing` で、Ctrl+Space を全角スペース挿入 (`input_fullwidth_space`) として消費する。

いずれも `EngineResult::consumed()` を返すため、フロントエンド (macOS Swift / Linux fcitx5 C++) はキーイベントを飲み込み、OS へ伝播しない。その結果、ユーザーが OS 側に割り当てた Ctrl+Space ショートカット (例: macOS の「次のウインドウを操作対象にする」) が発火せず、代わりに全角スペースが入力される。

現状 Karukan には GUI 設定画面が存在せず、設定は `config.toml` の手編集で行う (Linux fcitx5 のアドオンは `Configurable=True` と宣言しているが C++ 側に設定 API 実装がなく実質機能しない)。この挙動もハードコードされており変更できない。

## ゴール

`config.toml` の設定で、Ctrl+Space を全角スペース入力に使うか、OS へ素通しするかを切り替えられるようにする。

- 無効化 (`false`) 時は、未入力時・入力中の**両方**で Ctrl+Space を横取りせず OS へ渡す。
- デフォルトは `true` (既存挙動を維持し、後方互換を保つ)。
- Rust エンジン (`karukan-im`) のみの変更で macOS / Linux 両対応とする。

## 非ゴール

- 汎用キーバインド設定 (任意の機能に任意のキーを割り当てる仕組み) は作らない。今回は Ctrl+Space の ON/OFF ブール 1 個に絞る。
- GUI 設定画面は作らない。設定は `config.toml` 手編集のまま。
- フロントエンド (Swift / C++) のコードは変更しない。

## 設計

### 1. 設定項目

`config/default.toml` に新セクション `[keys]` を追加する。

```toml
[keys]
# Ctrl+Space を全角スペース入力に使う。
# false にすると Karukan は Ctrl+Space を横取りせず OS に渡す
# （macOS/Linux のウインドウ切替などのショートカットが効くようになる）。
ctrl_space_fullwidth = true
```

`[conversion]` ではなく新セクション `[keys]` にする理由: これは変換設定ではなくキー割り当ての挙動であり、意味的に分離する。今後キー系設定が増えた際の置き場所にもなる。

### 2. 設定の型定義と伝搬経路

既存の設定と全く同じ経路を辿る。

1. `karukan-im/src/config/settings.rs`
   - 新しい構造体 `KeysSettings { ctrl_space_fullwidth: bool }` を追加。
   - `Settings` に `pub keys: KeysSettings` フィールドを追加。
   - `merge_toml` + `parse_with_defaults` により、ユーザーが `[keys]` を書かなくても埋め込み `default.toml` の値で埋まる (既存の仕組みをそのまま利用)。
2. `karukan-im/src/core/engine/types.rs`
   - `EngineConfig` に `pub ctrl_space_fullwidth: bool` を追加。
   - `EngineConfig::from_settings` (`types.rs:96`) で `settings.keys.ctrl_space_fullwidth` を反映。
   - `impl Default for EngineConfig` (`types.rs:115`) で `true` を設定 (既存挙動維持)。

### 3. キー処理の分岐 (`karukan-im/src/core/engine/input.rs`)

- **Empty 状態** (`process_key_empty`, 現 `input.rs:122-131`)
  - `self.config.ctrl_space_fullwidth == false` かつ Ctrl+Space の場合、全角スペース分岐に入らず `EngineResult::not_consumed()` を返す。
  - (この分岐をスキップするだけでも、後続の bare-space 分岐は `!key.modifiers.control_key` 条件で該当しないため最終的に `not_consumed` に落ちるが、意図を明示するため明示的に early-return する。)
- **Composing 状態** (`process_key_composing`, 現 `input.rs:257-273` の Ctrl 分岐)
  - `self.config.ctrl_space_fullwidth == false` かつ Ctrl+Space の場合、**明示的に `EngineResult::not_consumed()` を返す**。
  - 注意: 単に全角スペース分岐をスキップすると、下の `match` で `Keysym::SPACE | Keysym::DOWN => self.start_conversion(false)` に該当し、Ctrl+Space が変換トリガーとして誤消費される。これを防ぐため明示的な early-return が必須。

### 4. フロントエンド

エンジンが `not_consumed` を返すと、既存のフロントは OS へキーを素通しする。

- macOS: `KarukanInputController.handle()` が `result.consumed == false` で `return false` する (`KarukanInputController.swift:70`)。
- Linux fcitx5: keyEvent が consumed でなければ `filterAndTransformKey` せず OS へ渡る。

いずれもコード変更は不要。

## 副作用と受容する挙動

Composing (変換中) に Ctrl+Space を押すと、未確定の preedit (下線テキスト) が残ったまま OS のショートカット (ウインドウ切替等) が発火する。「未入力時・入力中の両方で OS へ渡す」という決定に基づく想定内の挙動として受容する。

## テスト

`karukan-im` のエンジンテストに以下 4 ケースを追加する。

| flag | 状態 | 入力 | 期待 |
|---|---|---|---|
| `true` (default) | Empty | Ctrl+Space | 全角スペース (U+3000) で Composing 開始 (既存挙動) |
| `false` | Empty | Ctrl+Space | `not_consumed` (OS へ素通し) |
| `true` (default) | Composing | Ctrl+Space | 全角スペース挿入 (既存挙動) |
| `false` | Composing | Ctrl+Space | `not_consumed` (変換トリガーにならない) |

加えて、`settings.rs` に `[keys]` セクションの読み込み・デフォルト値のテストを追加する (既存の `test_partial_config` / `test_default_settings` に倣う)。

## 影響範囲まとめ

変更ファイル:

- `karukan-im/config/default.toml` — `[keys]` セクション追加
- `karukan-im/src/config/settings.rs` — `KeysSettings` 追加、`Settings` にフィールド追加、テスト追加
- `karukan-im/src/core/engine/types.rs` — `EngineConfig` にフィールド追加、`from_settings` / `Default` 反映
- `karukan-im/src/core/engine/input.rs` — Empty / Composing の Ctrl+Space 分岐に flag チェック追加
- `karukan-im/src/core/engine/tests/` (または `tests.rs`) — 4 ケース追加

変更しないもの: macOS Swift、Linux fcitx5 C++、その他フロントエンド。
