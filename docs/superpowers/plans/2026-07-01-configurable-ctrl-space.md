# Configurable Ctrl+Space Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `config.toml` の設定で、Ctrl+Space を全角スペース入力に使うか OS へ素通しするかを切り替えられるようにする。

**Architecture:** 既存の設定伝搬経路（`config.toml` → `Settings` → `EngineConfig` → エンジンのキー処理）に新しいブール設定 `keys.ctrl_space_fullwidth` を1つ追加する。エンジンの `input.rs` で Empty / Composing 両状態の Ctrl+Space 分岐にこのフラグのチェックを入れ、false なら `not_consumed` を返す。`not_consumed` を返せばフロント（macOS Swift / Linux fcitx5）が自動で OS へキーを素通しするため、フロント側は無変更。

**Tech Stack:** Rust (Cargo workspace crate `karukan-im`)、TOML 設定、`cargo test`。

## Global Constraints

- 変更は `karukan-im` クレートのみ。フロントエンド（Swift / C++）は変更しない。
- デフォルト値は `true`（既存挙動を維持し後方互換を保つ）。
- 設定セクション名は `[keys]`、キー名は `ctrl_space_fullwidth`（bool）。
- Rust では可能な限り関数型スタイルを優先するが、既存の `match` によるキー処理パターンには合わせる。
- テストは各タスク内で TDD（Red → Green）で書く。コミットはこのリポジトリの規約に従い `git ai-commit` を使う（`git commit` を直接使わない）。

## File Structure

- `karukan-im/config/default.toml` — 埋め込みデフォルト設定。`[keys]` セクションを追加。
- `karukan-im/src/config/settings.rs` — `KeysSettings` 構造体と `Settings.keys` フィールド、読み込みテスト。
- `karukan-im/src/core/engine/types.rs` — `EngineConfig` に `ctrl_space_fullwidth` を追加、`from_settings` と `Default` に反映。
- `karukan-im/src/core/engine/input.rs` — `process_key_empty` と `process_key_composing` の Ctrl+Space 分岐にフラグチェックを追加。
- `karukan-im/src/core/engine/tests/basic.rs` — `EngineConfig` のデフォルト/マッピングと、Empty/Composing のキー挙動テスト。

すべて既存ファイルへの追記・修正。新規ファイルなし。

---

### Task 1: `KeysSettings` を設定に追加する

**Files:**
- Modify: `karukan-im/config/default.toml`
- Modify: `karukan-im/src/config/settings.rs`
- Test: `karukan-im/src/config/settings.rs`（同ファイル内の `#[cfg(test)] mod tests`）

**Interfaces:**
- Produces: `pub struct KeysSettings { pub ctrl_space_fullwidth: bool }`、`Settings.keys: KeysSettings`。Task 2 の `EngineConfig::from_settings` が `settings.keys.ctrl_space_fullwidth` を参照する。

備考：`Settings::default()` は埋め込み `default.toml` をパースして生成されるため、`[keys]` セクションを `default.toml` に追加してからでないと `Settings::default()` が panic する。したがって default.toml と settings.rs は同一タスクで変更する。ユーザー設定に `[keys]` が無い場合は `parse_with_defaults` の `merge_toml` がデフォルト値で埋めるので、serde の default 属性は不要（既存の `ConversionSettings` と同じ扱い）。

- [ ] **Step 1: 失敗するテストを書く**

`karukan-im/src/config/settings.rs` の `mod tests`（`test_partial_config` の隣、末尾付近）に追記：

```rust
    #[test]
    fn test_keys_default_ctrl_space_fullwidth() {
        let settings = Settings::default();
        assert!(settings.keys.ctrl_space_fullwidth);
    }

    #[test]
    fn test_keys_override_and_other_sections_keep_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[keys]
ctrl_space_fullwidth = false
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert!(!settings.keys.ctrl_space_fullwidth);
        // Sections the user did not specify still fall back to defaults.
        assert_eq!(settings.conversion.num_candidates, 9);
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p karukan-im test_keys_ 2>&1 | tail -20`
Expected: コンパイルエラー（`Settings` に `keys` フィールドが無い / `KeysSettings` 未定義）で FAIL。

- [ ] **Step 3: 最小実装**

`karukan-im/src/config/settings.rs` の `Settings` 構造体にフィールドを追加：

```rust
/// Configuration settings for the IME
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Conversion settings
    pub conversion: ConversionSettings,
    /// Learning cache settings
    pub learning: LearningSettings,
    /// Key-binding behavior settings
    pub keys: KeysSettings,
}
```

`LearningSettings` の定義の直後に `KeysSettings` を追加：

```rust
/// Key-binding behavior settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysSettings {
    /// Use Ctrl+Space to input a full-width space (U+3000).
    /// When false, Karukan does not intercept Ctrl+Space and lets it pass
    /// through to the OS (so window-switching shortcuts etc. still work).
    pub ctrl_space_fullwidth: bool,
}
```

`karukan-im/config/default.toml` の末尾（`[learning]` セクションの後）に追記：

```toml

[keys]
# Ctrl+Space を全角スペース入力に使う。
# false にすると Karukan は Ctrl+Space を横取りせず OS に渡す
# （macOS/Linux のウインドウ切替などのショートカットが効くようになる）。
ctrl_space_fullwidth = true
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p karukan-im test_keys_ 2>&1 | tail -20`
Expected: `test_keys_default_ctrl_space_fullwidth` と `test_keys_override_and_other_sections_keep_defaults` が PASS。

- [ ] **Step 5: 既存の設定テストが壊れていないことを確認**

Run: `cargo test -p karukan-im --lib config 2>&1 | tail -20`
Expected: 既存の `test_default_settings` / `test_partial_config` 等を含め全て PASS。

- [ ] **Step 6: コミット**

```bash
git add karukan-im/config/default.toml karukan-im/src/config/settings.rs
git ai-commit
```

---

### Task 2: `EngineConfig` にフラグを追加し設定から反映する

**Files:**
- Modify: `karukan-im/src/core/engine/types.rs`
- Test: `karukan-im/src/core/engine/tests/basic.rs`

**Interfaces:**
- Consumes: Task 1 の `Settings.keys.ctrl_space_fullwidth`。
- Produces: `EngineConfig.ctrl_space_fullwidth: bool`。Task 3 / Task 4 が `self.config.ctrl_space_fullwidth` として読む。`EngineConfig::default()` は `true` を返す。

- [ ] **Step 1: 失敗するテストを書く**

`karukan-im/src/core/engine/tests/basic.rs` の末尾に追記：

```rust
#[test]
fn ctrl_space_fullwidth_defaults_true_in_engine_config() {
    let config = EngineConfig::default();
    assert!(config.ctrl_space_fullwidth);
}

#[test]
fn ctrl_space_fullwidth_maps_from_settings() {
    let mut settings = crate::config::Settings::default();
    assert!(EngineConfig::from_settings(&settings).ctrl_space_fullwidth);
    settings.keys.ctrl_space_fullwidth = false;
    assert!(!EngineConfig::from_settings(&settings).ctrl_space_fullwidth);
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p karukan-im ctrl_space_fullwidth_ 2>&1 | tail -20`
Expected: コンパイルエラー（`EngineConfig` に `ctrl_space_fullwidth` フィールドが無い）で FAIL。

- [ ] **Step 3: 最小実装**

`karukan-im/src/core/engine/types.rs` の `EngineConfig` 構造体（`pub live_conversion: bool,` の後）にフィールドを追加：

```rust
    /// Whether Ctrl+Space inputs a full-width space (U+3000).
    /// When false, Ctrl+Space is not consumed and passes through to the OS.
    pub ctrl_space_fullwidth: bool,
```

`EngineConfig::from_settings`（`live_conversion: settings.conversion.live_conversion,` の後）に追加：

```rust
            ctrl_space_fullwidth: settings.keys.ctrl_space_fullwidth,
```

`impl Default for EngineConfig`（`live_conversion: false,` の後）に追加：

```rust
            ctrl_space_fullwidth: true,
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p karukan-im ctrl_space_fullwidth_ 2>&1 | tail -20`
Expected: 両テストが PASS。

- [ ] **Step 5: クレート全体がビルドできることを確認**

Run: `cargo build -p karukan-im 2>&1 | tail -20`
Expected: エラーなし（`with_config` を使う既存テストの構造体リテラルは `..EngineConfig::default()` を使っているため影響なし）。

- [ ] **Step 6: コミット**

```bash
git add karukan-im/src/core/engine/types.rs karukan-im/src/core/engine/tests/basic.rs
git ai-commit
```

---

### Task 3: Empty 状態の Ctrl+Space をフラグで分岐する

**Files:**
- Modify: `karukan-im/src/core/engine/input.rs`（`process_key_empty`）
- Test: `karukan-im/src/core/engine/tests/basic.rs`

**Interfaces:**
- Consumes: Task 2 の `self.config.ctrl_space_fullwidth`。
- Produces: なし（挙動の変更のみ）。

- [ ] **Step 1: 失敗するテストを書く**

`karukan-im/src/core/engine/tests/basic.rs` の末尾に追記。`press_ctrl` は `tests/mod.rs` で定義済み（`use super::*` で利用可能）：

```rust
#[test]
fn ctrl_space_inputs_fullwidth_space_in_empty_when_enabled() {
    // Default config has ctrl_space_fullwidth = true.
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "\u{3000}");
}

#[test]
fn ctrl_space_passes_through_in_empty_when_disabled() {
    let config = EngineConfig {
        ctrl_space_fullwidth: false,
        ..EngineConfig::default()
    };
    let mut engine = InputMethodEngine::with_config(config);
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(
        result.actions.is_empty(),
        "expected no actions, got {:?}",
        result.actions
    );
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p karukan-im ctrl_space_passes_through_in_empty 2>&1 | tail -20`
Expected: `ctrl_space_passes_through_in_empty_when_disabled` が FAIL（現状は無効化フラグを見ずに全角スペースを consume するため `result.consumed` が true になり assert 失敗）。`ctrl_space_inputs_fullwidth_space_in_empty_when_enabled` は既存挙動なので PASS。

- [ ] **Step 3: 最小実装**

`karukan-im/src/core/engine/input.rs` の `process_key_empty` 冒頭の Ctrl+Space ブロックを次に置き換える：

```rust
        // Ctrl+Space: start input with full-width space.
        // Gated on config: when `ctrl_space_fullwidth` is false, do not
        // intercept — return not_consumed so the key passes through to the
        // OS (e.g. window-switching shortcuts).
        if key.modifiers.control_key && key.keysym == Keysym::SPACE {
            if !self.config.ctrl_space_fullwidth {
                return EngineResult::not_consumed();
            }
            self.converters.romaji.reset();
            self.input_buf.clear();
            self.input_buf.insert("\u{3000}");
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
        }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p karukan-im ctrl_space 2>&1 | tail -20`
Expected: Task 2・Task 3 の Ctrl+Space 関連テストが全て PASS。

- [ ] **Step 5: コミット**

```bash
git add karukan-im/src/core/engine/input.rs karukan-im/src/core/engine/tests/basic.rs
git ai-commit
```

---

### Task 4: Composing 状態の Ctrl+Space をフラグで分岐する

**Files:**
- Modify: `karukan-im/src/core/engine/input.rs`（`process_key_composing`）
- Test: `karukan-im/src/core/engine/tests/basic.rs`

**Interfaces:**
- Consumes: Task 2 の `self.config.ctrl_space_fullwidth`。
- Produces: なし（挙動の変更のみ）。

注意：`process_key_composing` では、フラグ false のとき **明示的に `not_consumed` を返す**必要がある。単に全角スペース分岐をスキップすると、下の `match` の `Keysym::SPACE | Keysym::DOWN => self.start_conversion(false)` に該当し、Ctrl+Space が変換トリガーとして誤消費される。テストの `ctrl_space_passes_through_while_composing_when_disabled` がこのガードを検証する。

- [ ] **Step 1: 失敗するテストを書く**

`karukan-im/src/core/engine/tests/basic.rs` の末尾に追記：

```rust
#[test]
fn ctrl_space_inserts_fullwidth_space_while_composing_when_enabled() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a')); // preedit "あ", Composing
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(
        engine.input_buf.text.contains('\u{3000}'),
        "buffer should contain a full-width space, got {:?}",
        engine.input_buf.text
    );
}

#[test]
fn ctrl_space_passes_through_while_composing_when_disabled() {
    let config = EngineConfig {
        ctrl_space_fullwidth: false,
        ..EngineConfig::default()
    };
    let mut engine = InputMethodEngine::with_config(config);
    engine.process_key(&press('a')); // preedit "あ", Composing
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(!result.consumed);
    // Must NOT trigger conversion (the fall-through bug guard) and must
    // NOT insert a full-width space.
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
    assert!(!engine.input_buf.text.contains('\u{3000}'));
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p karukan-im ctrl_space_passes_through_while_composing 2>&1 | tail -20`
Expected: `ctrl_space_passes_through_while_composing_when_disabled` が FAIL（現状は全角スペースを挿入し `result.consumed` が true になる、または変換トリガーで Conversion 状態になり assert 失敗）。

- [ ] **Step 3: 最小実装**

`karukan-im/src/core/engine/input.rs` の `process_key_composing` の Ctrl 分岐内、`Keysym::SPACE` のアームを次に置き換える：

```rust
                // Ctrl+Space: insert full-width space (U+3000), unless
                // disabled in config — then pass through to the OS. Must
                // return explicitly here: falling through would let the
                // bare-Space arm below treat Ctrl+Space as the conversion
                // trigger.
                Keysym::SPACE => {
                    return if self.config.ctrl_space_fullwidth {
                        self.input_fullwidth_space()
                    } else {
                        EngineResult::not_consumed()
                    };
                }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test -p karukan-im ctrl_space 2>&1 | tail -20`
Expected: Ctrl+Space 関連テストが全て PASS。

- [ ] **Step 5: クレート全体のテスト・lint を確認**

Run: `cargo test -p karukan-im 2>&1 | tail -20`
Expected: 全テスト PASS。

Run: `cargo clippy -p karukan-im 2>&1 | tail -20`
Expected: 新しい警告なし。

Run: `cargo fmt --all -- --check 2>&1 | tail -5`
Expected: 差分なし（差分があれば `cargo fmt --all` を実行してから再コミット）。

- [ ] **Step 6: コミット**

```bash
git add karukan-im/src/core/engine/input.rs karukan-im/src/core/engine/tests/basic.rs
git ai-commit
```

---

## Manual Verification（実装完了後、任意）

実機での動作確認は以下（コード変更不要のフロント経由で確認できる）：

1. `~/Library/Application Support/com.karukan.karukan-im/config.toml`（macOS）または `~/.config/karukan-im/config.toml`（Linux）に以下を書く：
   ```toml
   [keys]
   ctrl_space_fullwidth = false
   ```
2. macOS は `killall KarukanIME`、Linux は fcitx5 を再起動して imserver / アドオンに設定を読み直させる。
3. Karukan 入力モードで、未入力時に Ctrl+Space を押し、全角スペースが入らず OS のウインドウ切替ショートカットが発火することを確認する。
4. `true` に戻す（または行を消す）と全角スペースが入る従来挙動に戻ることを確認する。

## Self-Review

- **Spec coverage:** 設定項目（Task 1）、伝搬経路（Task 2）、Empty 分岐（Task 3）、Composing 分岐 + 誤消費ガード（Task 4）、テスト4ケース + 設定テスト、フロント無変更、デフォルト true をすべてタスク化済み。副作用（Composing 中の preedit 残存）は spec に記載済みで実装上の対応不要。
- **Placeholder scan:** プレースホルダなし。全ステップに実コードと実コマンドを記載。
- **Type consistency:** `KeysSettings.ctrl_space_fullwidth`（Task 1）→ `EngineConfig.ctrl_space_fullwidth`（Task 2）→ `self.config.ctrl_space_fullwidth`（Task 3/4）で名称一致。`EngineConfig::default()` は Task 2 で `true` を設定し、Task 3/4 のテストが `InputMethodEngine::new()`（内部で `EngineConfig::default()` 使用）に依存する点も整合。
