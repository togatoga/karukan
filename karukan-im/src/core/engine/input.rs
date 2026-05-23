//! Composing input handling (Empty and Composing states)

use super::*;

/// Append candidates to `target`, skipping duplicates by text.
fn append_candidates_dedup(target: &mut Vec<Candidate>, source: Vec<Candidate>) {
    for c in source {
        if !target.iter().any(|existing| existing.text == c.text) {
            target.push(c);
        }
    }
}

impl InputMethodEngine {
    /// Refresh the input state: rebuild preedit and run auto-suggest for candidates.
    pub(super) fn refresh_input_state(&mut self) -> EngineResult {
        // Alphabet mode with active live conversion: preserve the conversion display
        if self.input_mode == InputMode::Alphabet && !self.live.text.is_empty() {
            let preedit = self.set_composing_state();
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        // Run auto-suggest (skip in alphabet mode — no hiragana to convert)
        let candidates =
            if self.input_mode != InputMode::Alphabet && !self.input_buf.text.is_empty() {
                let reading = self.input_buf.text.clone();
                let result = self.run_auto_suggest(&reading, 1);
                if !result.is_empty() && result[0] != self.input_buf.text {
                    Some((result, reading))
                } else {
                    None
                }
            } else {
                None
            };

        let Some((candidates, reading)) = candidates else {
            // No useful AI suggestion — still show learning + dictionary + rule-based
            // rewriter variants. The rewriter path produces mozc-style symbol variants
            // (e.g. `「` → `『`, `【`, ...) for symbol-only inputs where the model is skipped.
            self.live.text.clear();
            let preedit = self.set_composing_state();
            let reading = self.input_buf.text.clone();
            let mut all_candidates = self.lookup_learning_candidates(&reading);
            append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(&reading));
            append_candidates_dedup(&mut all_candidates, self.lookup_rewriter_variants(&reading));
            if all_candidates.is_empty() {
                return EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(preedit))
                    .with_action(EngineAction::HideCandidates)
                    .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
            }
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::ShowCandidates(CandidateList::new(
                    all_candidates,
                )))
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
        };

        // Live conversion mode: show converted text in preedit
        if self.live.enabled && self.input_mode != InputMode::Katakana {
            self.live.text = candidates[0].clone();
            let preedit = self.set_composing_state();
            let mut result =
                EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));

            // Learning candidates first, then dictionary candidates
            let mut all_candidates = self.lookup_learning_candidates(&reading);
            append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(&reading));
            if all_candidates.is_empty() {
                result = result.with_action(EngineAction::HideCandidates);
            } else {
                result = result.with_action(EngineAction::ShowCandidates(CandidateList::new(
                    all_candidates,
                )));
            }
            let aux = self.format_aux_suggest(&self.input_buf.text.clone());
            return result.with_action(EngineAction::UpdateAuxText(aux));
        }

        // Normal auto-suggest: show hiragana preedit + learning/model/dict candidates
        self.live.text.clear();
        let preedit = self.set_composing_state();
        // Learning candidates first (highest priority)
        let mut all_candidates = self.lookup_learning_candidates(&reading);
        // Then model inference candidates
        let model_candidates: Vec<Candidate> = candidates
            .into_iter()
            .map(|s| Candidate::with_reading(s, &reading))
            .collect();
        append_candidates_dedup(&mut all_candidates, model_candidates);
        // Then dictionary candidates
        append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(&reading));
        let aux = self.format_aux_suggest(&self.input_buf.text.clone());
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(CandidateList::new(
                all_candidates,
            )))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Process key in empty state
    pub(super) fn process_key_empty(&mut self, key: &KeyEvent, shift_active: bool) -> EngineResult {
        // Ctrl+Space: start input with full-width space
        if key.modifiers.control_key && key.keysym == Keysym::SPACE {
            self.converters.romaji.reset();
            self.input_buf.clear();
            self.input_buf.insert("\u{3000}");
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
        }

        // Bare Space from Empty state:
        //
        // * Hiragana mode → commit a full-width `　` directly, matching
        //   the Japanese-IME convention. We deliberately do NOT enter
        //   Composing here: if we did, the next Space the user typed
        //   would be interpreted by `process_key_composing` as the
        //   conversion trigger and an unwanted candidate window would
        //   appear after two spaces in a row.
        // * Any other mode → return `not_consumed` so the OS delivers
        //   a normal half-width ASCII space to the application. The
        //   user is either typing ASCII (Alphabet) or in an edge mode
        //   (Katakana / Emoji) where injecting `　` would be wrong.
        //
        // The full-width space gesture from Empty in any mode is
        // `Ctrl+Space` (above), which seeds a Composing session.
        if key.keysym == Keysym::SPACE && !key.modifiers.control_key && !key.modifiers.alt_key {
            return if self.input_mode == InputMode::Hiragana {
                EngineResult::consumed().with_action(EngineAction::Commit("\u{3000}".to_string()))
            } else {
                EngineResult::not_consumed()
            };
        }

        // `:` from Empty state enters emoji shortcode mode — `:pien` stays
        // as `:pien` literally (no romaji conversion) while emoji candidates
        // are surfaced via the rewriter. The mode auto-exits back to Hiragana
        // on Escape or commit, so the user's next word lands in kana mode
        // again without an explicit toggle.
        //
        // Two keysym shapes can produce `:` depending on how fcitx5
        // resolves the layout: (a) the X11 `colon` keysym (0x003A)
        // arriving directly, or (b) the `semicolon` keysym (0x003B)
        // with shift held. Accept both so we don't depend on which
        // shape the upstream stack happens to emit.
        let typed_colon =
            key.to_char() == Some(':') || (shift_active && key.keysym == Keysym(b';' as u32));
        if typed_colon
            && !key.modifiers.control_key
            && !key.modifiers.alt_key
            && self.input_mode != InputMode::Alphabet
        {
            return self.start_emoji_mode();
        }

        // Only handle printable characters without modifiers (except shift)
        if let Some(ch) = key.to_char()
            && !key.modifiers.control_key
            && !key.modifiers.alt_key
        {
            // Detect Shift+letter: shift modifier with alphabetic, OR uppercase keysym.
            // fcitx5 may resolve Shift into the keysym (sending 'A' instead of 'a'+shift),
            // so we must also check for uppercase to handle both cases.
            let is_shift_alpha =
                ch.is_ascii_uppercase() || (shift_active && ch.is_ascii_alphabetic());

            if is_shift_alpha && self.input_mode != InputMode::Alphabet {
                self.input_mode = InputMode::Alphabet;
            }
            let ch = if self.input_mode == InputMode::Alphabet && is_shift_alpha {
                ch.to_ascii_uppercase()
            } else {
                ch
            };
            return self.start_input(ch);
        }
        EngineResult::not_consumed()
    }

    /// Start input with a character (first character of a new input session).
    /// In alphabet mode, inserts directly; otherwise goes through romaji conversion.
    pub(super) fn start_input(&mut self, ch: char) -> EngineResult {
        self.converters.romaji.reset();
        self.input_buf.clear();

        if self.input_mode == InputMode::Alphabet {
            self.input_buf.insert(&ch.to_string());
        } else {
            let prev_output_len = 0;
            let _event = self.converters.romaji.push(ch);
            let romaji_buffer = self.converters.romaji.buffer().to_string();

            // PassThrough chars (no romaji rule, e.g. `'`, `;`, `<`, `(`) used to
            // auto-commit immediately, but that prevented users from composing
            // sequences like `「」` or getting symbol variants. Treat them like
            // digits — let them enter Composing and accumulate in the preedit.

            if self.converters.romaji.output().is_empty() && romaji_buffer.is_empty() {
                return EngineResult::not_consumed();
            }

            // Consume new converter output into composed_hiragana
            let new_output_len = self.converters.romaji.output().chars().count();
            if new_output_len > prev_output_len {
                let new_chars: String = self
                    .converters
                    .romaji
                    .output()
                    .chars()
                    .skip(prev_output_len)
                    .collect();
                self.input_buf.insert(&new_chars);
            }
        }

        let preedit = self.set_composing_state();

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Insert a full-width space (U+3000) at cursor position
    pub(super) fn input_fullwidth_space(&mut self) -> EngineResult {
        self.input_buf.insert("\u{3000}");
        self.refresh_input_state()
    }

    /// Process key in hiragana input state
    pub(super) fn process_key_composing(
        &mut self,
        key: &KeyEvent,
        shift_active: bool,
    ) -> EngineResult {
        // Handle Ctrl+key shortcuts
        if key.modifiers.control_key {
            match key.keysym {
                // Ctrl+Space: insert full-width space (U+3000)
                Keysym::SPACE => return self.input_fullwidth_space(),
                // Ctrl+K: enter katakana mode
                Keysym::KEY_K | Keysym::KEY_K_UPPER => return self.enter_katakana_mode(),
                // Ctrl+A: move to beginning (Emacs-style Home)
                Keysym::KEY_A | Keysym::KEY_A_UPPER => return self.move_caret_home(),
                // Ctrl+B: move left (Emacs-style Left)
                Keysym::KEY_B | Keysym::KEY_B_UPPER => return self.move_caret_left(),
                // Ctrl+E: move to end (Emacs-style End)
                Keysym::KEY_E | Keysym::KEY_E_UPPER => return self.move_caret_end(),
                // Ctrl+F: move right (Emacs-style Right)
                Keysym::KEY_F | Keysym::KEY_F_UPPER => return self.move_caret_right(),
                _ => {}
            }
        }

        match key.keysym {
            Keysym::RETURN => self.commit_composing(),
            Keysym::ESCAPE => self.cancel_composing(),
            Keysym::BACKSPACE => self.backspace_composing(),
            Keysym::DELETE => self.delete_composing(),
            Keysym::SPACE if self.input_mode == InputMode::Alphabet => self.input_char(' '),
            // Tab triggers conversion that bypasses the learning cache, so users
            // can escape stale or unwanted learned entries (mozc binds Tab to a
            // different conversion path — PredictAndConvert — in the same spirit).
            Keysym::TAB => self.start_conversion(true),
            Keysym::SPACE | Keysym::DOWN => self.start_conversion(false),
            Keysym::LEFT => self.move_caret_left(),
            Keysym::RIGHT => self.move_caret_right(),
            Keysym::HOME => self.move_caret_home(),
            Keysym::END => self.move_caret_end(),
            _ => {
                if let Some(ch) = key.to_char()
                    && !key.modifiers.control_key
                    && !key.modifiers.alt_key
                {
                    // Detect Shift+letter: shift modifier with alphabetic, OR uppercase keysym.
                    // fcitx5 may resolve Shift into the keysym (sending 'A' instead of 'a'+shift).
                    let is_shift_alpha =
                        ch.is_ascii_uppercase() || (shift_active && ch.is_ascii_alphabetic());

                    if is_shift_alpha && self.input_mode != InputMode::Alphabet {
                        // Bake katakana before switching so preedit doesn't revert
                        if self.input_mode == InputMode::Katakana {
                            self.bake_katakana();
                        }
                        self.input_mode = InputMode::Alphabet;
                        self.flush_romaji_to_composed();
                        self.live.text.clear();
                    }
                    let ch = if self.input_mode == InputMode::Alphabet && is_shift_alpha {
                        ch.to_ascii_uppercase()
                    } else {
                        ch
                    };
                    return self.input_char(ch);
                }
                EngineResult::not_consumed()
            }
        }
    }

    /// Begin a new emoji-shortcode composing session.
    ///
    /// Resets any leftover state, switches `input_mode` to
    /// [`InputMode::Emoji`], seeds the buffer with `:`, and refreshes
    /// the candidate list so the user sees emoji suggestions appear
    /// the moment they press `:`.
    pub(super) fn start_emoji_mode(&mut self) -> EngineResult {
        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.input_mode = InputMode::Emoji;
        self.input_buf.insert(":");
        self.refresh_input_state()
    }

    /// First emoji candidate the rewriter would surface for `reading`,
    /// or `None` if none match. Used by Enter in emoji mode so committing
    /// `:smile` produces 😄 directly rather than the literal `:smile`.
    fn first_emoji_candidate(&self, reading: &str) -> Option<String> {
        self.converters
            .rewriters
            .rewrite_all(&[reading.to_string()])
            .into_iter()
            .map(|(text, _desc)| text)
            .next()
    }

    /// Input a character during composing.
    /// In alphabet mode, inserts directly; otherwise goes through romaji conversion.
    pub(super) fn input_char(&mut self, ch: char) -> EngineResult {
        if matches!(self.input_mode, InputMode::Alphabet | InputMode::Emoji) {
            self.input_buf.insert(&ch.to_string());
            return self.refresh_input_state();
        }

        let prev_output_len = self.converters.romaji.output().chars().count();
        let _event = self.converters.romaji.push(ch);
        let curr_output_len = self.converters.romaji.output().chars().count();

        // Consume ALL new converter output into composed_hiragana at cursor position.
        // The converter may recursively pass through multiple chars (e.g., "thx" →
        // output="th", buffer="x"), so capture all of them via delta detection.
        // PassThrough chars are already included in the converter output.
        if curr_output_len > prev_output_len {
            let new_chars: String = self
                .converters
                .romaji
                .output()
                .chars()
                .skip(prev_output_len)
                .collect();
            self.input_buf.insert(&new_chars);
        }

        // PassThrough chars no longer auto-commit. They accumulate in the preedit
        // alongside hiragana, allowing users to compose `「」`, type `'word'`,
        // and access symbol variants from the candidate list.

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.refresh_input_state()
    }

    /// Commit the current hiragana input (or katakana if in katakana mode)
    /// In live conversion mode, commits the converted text instead of hiragana.
    pub(super) fn commit_composing(&mut self) -> EngineResult {
        // Flush any pending romaji into composed_hiragana
        self.flush_romaji_to_composed();

        let reading = self.input_buf.text.clone();
        let text = if self.input_mode == InputMode::Emoji {
            // Emoji mode: Enter should select the first emoji candidate the
            // EmojiRewriter would surface, not commit the literal `:smile`.
            // Falls back to the literal buffer when nothing matches (e.g.
            // `:xyz`) so the user still sees what they typed.
            self.first_emoji_candidate(&reading)
                .unwrap_or_else(|| reading.clone())
        } else if self.input_mode == InputMode::Katakana {
            // Katakana mode always commits katakana, ignoring live conversion
            karukan_engine::hiragana_to_katakana(&reading)
        } else if !self.live.text.is_empty() {
            // Live conversion active: commit converted text
            self.live.text.clone()
        } else {
            reading.clone()
        };

        if text.is_empty() {
            self.state = InputState::Empty;
            self.input_buf.clear();
            self.live.text.clear();
            return EngineResult::consumed().with_action(EngineAction::HideAuxText);
        }

        // Record live conversion result in learning cache.
        // Skip the learning record for emoji mode — the buffer holds
        // a Slack-style query like `:smile`, not a hiragana reading,
        // so storing it would corrupt the kana-keyed learning cache.
        if self.input_mode != InputMode::Emoji {
            self.record_learning(&reading, &text);
        }

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;
        if self.input_mode == InputMode::Emoji {
            self.input_mode = InputMode::Hiragana;
        }

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::Commit(text))
            .with_action(EngineAction::HideAuxText)
    }

    /// Cancel the current input
    /// In live conversion mode: first Escape clears live conversion and shows hiragana,
    /// second Escape cancels input entirely.
    pub(super) fn cancel_composing(&mut self) -> EngineResult {
        // If live conversion is active, first Escape returns to hiragana display
        if !self.live.text.is_empty() {
            self.live.text.clear();
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
        }

        // Emoji mode: Escape closes the picker but commits the literal
        // buffer (the typed `:smile` or `:xyz`) — Slack-style escape.
        // The user is saying "abandon the emoji lookup but keep what I
        // typed as plain text". Without this, Escape would silently
        // discard the typed characters which is surprising when the
        // user just wanted to dismiss the candidate list.
        let emoji_literal =
            if self.input_mode == InputMode::Emoji && !self.input_buf.text.is_empty() {
                Some(self.input_buf.text.clone())
            } else {
                None
            };

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;
        // Emoji mode is per-session: leaving it returns the user to the
        // default Hiragana behavior so their next word doesn't unexpectedly
        // stay in ASCII-passthrough mode.
        if self.input_mode == InputMode::Emoji {
            self.input_mode = InputMode::Hiragana;
        }

        let mut result = EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText);
        if let Some(literal) = emoji_literal {
            result = result.with_action(EngineAction::Commit(literal));
        }
        result
    }
}
