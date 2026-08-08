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
        let full_reading = self.input_buf.reading();

        // Alphabet mode with active live conversion but no kana left to convert:
        // preserve the existing conversion display without re-running the model.
        // (When the buffer still contains kana we fall through and reconvert below,
        // so a mixed reading like `きょうはABC` keeps live-converting.)
        if self.mode.current() == InputMode::Alphabet
            && !self.live_text().is_empty()
            && !karukan_engine::contains_kana(&full_reading)
        {
            let preedit = self.set_composing_state();
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        // Run auto-suggest via chunked conversion. Normally skipped in alphabet
        // mode (raw latin has no hiragana to convert), but if the buffer still
        // contains kana — e.g. the user typed hiragana, switched to alphabet mode,
        // and kept typing — keep converting the mixed reading so live conversion
        // stays alive. `chunked_auto_suggest` splits long input into
        // bounded-length chunks so per-keystroke latency stays flat; for input
        // within one chunk this is identical to a whole-buffer call.
        let convert = !full_reading.is_empty()
            && (self.mode.current() != InputMode::Alphabet
                || karukan_engine::contains_kana(&full_reading));
        let candidates = if convert {
            let reading = full_reading.clone();
            self.chunked_auto_suggest()
                .map(|converted| (vec![converted], reading))
        } else {
            self.chunks.clear();
            None
        };

        let Some((candidates, reading)) = candidates else {
            // No useful AI suggestion — still show learning + dictionary + rule-based
            // rewriter variants. The rewriter path produces mozc-style symbol variants
            // (e.g. `「` → `『`, `【`, ...) for symbol-only inputs where the model is skipped.
            self.live.shown = false;
            let preedit = self.set_composing_state();
            let reading = full_reading;
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

        // Live conversion mode: show converted text in preedit. The displayed
        // text is derived from the chunks (`live_text`), which
        // `chunked_auto_suggest` just rebuilt — candidates[0] is that same
        // concatenation.
        if self.live.enabled && self.mode.current() != InputMode::Katakana {
            self.live.shown = true;
            return self.suggest_result(candidates, &reading);
        }

        // Normal auto-suggest: show hiragana preedit
        self.live.shown = false;
        self.suggest_result(candidates, &reading)
    }

    /// Build the auto-suggest result shared by live conversion and normal
    /// auto-suggest: composing preedit, candidate list, and aux text.
    ///
    /// Candidate ordering is learning → model → dictionary. Including the
    /// model candidates guarantees the list is never empty, so the candidate
    /// window — whose aux line is where frontends show the raw reading once
    /// the preedit displays converted text — stays on screen for the whole
    /// live conversion.
    fn suggest_result(&mut self, candidates: Vec<String>, reading: &str) -> EngineResult {
        let preedit = self.set_composing_state();
        let mut all_candidates = self.lookup_learning_candidates(reading);
        let model_candidates: Vec<Candidate> = candidates
            .into_iter()
            .map(|s| Candidate::with_reading(s, reading))
            .collect();
        append_candidates_dedup(&mut all_candidates, model_candidates);
        append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(reading));
        let aux = self.format_aux_suggest(reading);
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(CandidateList::new(
                all_candidates,
            )))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Process key in empty state
    pub(super) fn process_key_empty(&mut self, key: &KeyEvent, shift_active: bool) -> EngineResult {
        // Ctrl+Space: start input with full-width space.
        // Gated on config: when `ctrl_space_fullwidth` is false, do not
        // intercept — return not_consumed so the key passes through to the
        // OS (e.g. window-switching shortcuts).
        if key.modifiers.control_key && key.keysym == Keysym::SPACE {
            if !self.config.ctrl_space_fullwidth {
                return EngineResult::not_consumed();
            }
            self.input_buf.clear();
            self.input_buf.push_direct('\u{3000}');
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
            return if self.mode.current() == InputMode::Hiragana {
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
            && self.mode.current() != InputMode::Alphabet
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

            if is_shift_alpha {
                // Shift-alphabet is a temporary per-word mode, not a sticky
                // toggle: ModeState remembers the mode to restore when this
                // word is committed, so the next word returns to kana (#37).
                self.mode.enter_temporary(InputMode::Alphabet);
            }
            let ch = if self.mode.current() == InputMode::Alphabet && is_shift_alpha {
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
        self.input_buf.clear();

        if self.mode.current() == InputMode::Alphabet {
            self.input_buf.push_direct(ch);
        } else {
            // PassThrough chars (no romaji rule, e.g. `'`, `;`, `<`, `(`) used to
            // auto-commit immediately, but that prevented users from composing
            // sequences like `「」` or getting symbol variants. Treat them like
            // digits — let them enter Composing and accumulate in the preedit.
            self.input_buf.push_romaji(ch, &self.converters.romaji);
        }

        let preedit = self.set_composing_state();

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Insert a full-width space (U+3000) after the active elements
    pub(super) fn input_fullwidth_space(&mut self) -> EngineResult {
        self.input_buf.push_direct('\u{3000}');
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
            Keysym::SPACE if self.mode.current() == InputMode::Alphabet => self.input_char(' '),
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

                    if is_shift_alpha && self.mode.current() != InputMode::Alphabet {
                        // Bake katakana before switching so preedit doesn't
                        // revert; in kana mode the live romaji stays live so
                        // typing next to it can still combine
                        if self.mode.current() == InputMode::Katakana {
                            self.settle_romaji();
                            self.bake_katakana();
                        }
                        // Shift-alphabet is a temporary per-word mode:
                        // ModeState remembers the mode to restore on
                        // commit/cancel, so the next word returns to the
                        // prior mode (issue #37).
                        self.mode.enter_temporary(InputMode::Alphabet);
                        self.live.shown = false;
                    }
                    let ch = if self.mode.current() == InputMode::Alphabet && is_shift_alpha {
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
    /// Resets any leftover state, switches the input mode to
    /// [`InputMode::Emoji`], seeds the buffer with `:`, and refreshes
    /// the candidate list so the user sees emoji suggestions appear
    /// the moment they press `:`.
    pub(super) fn start_emoji_mode(&mut self) -> EngineResult {
        self.input_buf.clear();
        self.live.shown = false;
        // Remember where the user was so commit/cancel/erase-to-empty
        // can drop them back into the same mode (e.g. Katakana stays
        // Katakana). ModeState guards against clobbering the saved mode
        // on re-entry just in case start_emoji_mode is ever called while
        // already in Emoji mode.
        self.mode.enter_temporary(InputMode::Emoji);
        self.input_buf.push_direct(':');
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
        if matches!(self.mode.current(), InputMode::Alphabet | InputMode::Emoji) {
            self.input_buf.push_direct(ch);
            return self.refresh_input_state();
        }

        // PassThrough chars accumulate in the preedit alongside hiragana,
        // allowing users to compose `「」`, type `'word'`, and access symbol
        // variants from the candidate list.
        self.input_buf.push_romaji(ch, &self.converters.romaji);

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.refresh_input_state()
    }

    /// Commit the current hiragana input (or katakana if in katakana mode)
    /// In live conversion mode, commits the converted text instead of hiragana.
    pub(super) fn commit_composing(&mut self) -> EngineResult {
        // Resolve the live text before settling: it needs the pending run
        let live_text = self.live_text_with_pending();

        // Settle any pending romaji
        self.settle_romaji();

        let reading = self.input_buf.reading();
        let text = if self.mode.current() == InputMode::Emoji {
            // Emoji mode: Enter should select the first emoji candidate the
            // EmojiRewriter would surface, not commit the literal `:smile`.
            // Falls back to the literal buffer when nothing matches (e.g.
            // `:xyz`) so the user still sees what they typed.
            self.first_emoji_candidate(&reading)
                .unwrap_or_else(|| reading.clone())
        } else if self.mode.current() == InputMode::Katakana {
            // Katakana mode always commits katakana, ignoring live conversion
            karukan_engine::hiragana_to_katakana(&reading)
        } else if !live_text.is_empty() {
            // Live conversion active: commit converted text
            live_text
        } else {
            reading.clone()
        };

        if text.is_empty() {
            self.state = InputState::Empty;
            self.input_buf.clear();
            self.live.shown = false;
            self.chunks.clear();
            return EngineResult::consumed()
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        // Record live conversion result in learning cache.
        // Skip the learning record for emoji mode — the buffer holds
        // a Slack-style query like `:smile`, not a hiragana reading,
        // so storing it would corrupt the kana-keyed learning cache.
        if self.mode.current() != InputMode::Emoji {
            self.record_learning(&reading, &text);
        }

        self.input_buf.clear();
        self.live.shown = false;
        self.chunks.clear();
        self.state = InputState::Empty;
        // Temporary modes (Emoji, Alphabet) end with the composition:
        // committing the word returns to the prior mode, so the next word
        // is converted again (#37).
        self.mode.exit_temporary();

        // HideCandidates is required here: the auto-suggest/live-conversion
        // window may be open while Composing, and the macOS frontend's
        // NSPanel only closes on an explicit hide (fcitx5 resets its panel
        // on commit implicitly, which masked this on Linux).
        EngineResult::consumed()
            .with_action(EngineAction::Commit(text))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
    }

    /// Cancel the current input
    /// In live conversion mode: first Escape clears live conversion and shows hiragana,
    /// second Escape cancels input entirely.
    pub(super) fn cancel_composing(&mut self) -> EngineResult {
        // If live conversion is active, first Escape returns to hiragana display
        if !self.live_text().is_empty() {
            self.live.shown = false;
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
        let emoji_literal = if self.mode.current() == InputMode::Emoji {
            Some(self.input_buf.reading()).filter(|r| !r.is_empty())
        } else {
            None
        };

        self.input_buf.clear();
        self.live.shown = false;
        self.chunks.clear();
        self.state = InputState::Empty;
        // Temporary modes (Emoji, Alphabet) are per-session: cancelling
        // returns the user to whatever mode they were in before, so their
        // next word doesn't unexpectedly stay in ASCII-passthrough mode
        // (#37).
        self.mode.exit_temporary();

        if let Some(literal) = emoji_literal {
            EngineResult::consumed()
                .with_action(EngineAction::Commit(literal))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText)
        } else {
            EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText)
        }
    }
}
