//! Composing input handling (Empty and Composing states)

use super::filter::source_for_key;
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

        // Auto-suggest via chunked conversion. Skipped in alphabet mode
        // unless the buffer still contains kana (mode switched mid-word),
        // so live conversion stays alive on a mixed reading.
        let convert = !self.suppress_suggest
            && !full_reading.is_empty()
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
            // No useful model suggestion — still show learning, dictionary,
            // and rewriter variants (e.g. `「` → `『`, `【`, …).
            self.live.shown = false;
            let preedit = self.set_composing_state();
            let reading = full_reading;
            let mut all_candidates = self.lookup_learning_candidates(&reading);
            append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(&reading));
            append_candidates_dedup(&mut all_candidates, self.lookup_rewriter_variants(&reading));
            let aux = self.format_aux_composing();
            if all_candidates.is_empty() {
                self.shown_suggestions = CandidateList::default();
                return EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(preedit))
                    .with_action(EngineAction::HideCandidates)
                    .with_action(EngineAction::UpdateAuxText(aux));
            }
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(self.show_suggestions(all_candidates))
                .with_action(EngineAction::UpdateAuxText(aux));
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

    /// Build the auto-suggest result (preedit, candidates, aux), ordered
    /// learning → model → dictionary. The model candidates keep the list
    /// non-empty, so the candidate window — whose aux line shows the raw
    /// reading — stays on screen for the whole live conversion.
    fn suggest_result(&mut self, candidates: Vec<String>, reading: &str) -> EngineResult {
        let preedit = self.set_composing_state();
        let mut all_candidates = self.lookup_learning_candidates(reading);
        let model_candidates: Vec<Candidate> = candidates
            .into_iter()
            .map(|s| Candidate::with_reading(s, reading))
            .collect();
        append_candidates_dedup(&mut all_candidates, model_candidates);
        append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(reading));
        let aux = self.format_aux_suggest();
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(self.show_suggestions(all_candidates))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Show `candidates` in the composing suggestion window, remembering the
    /// list so Ctrl+digit can select from exactly what is on screen.
    fn show_suggestions(&mut self, candidates: Vec<Candidate>) -> EngineAction {
        self.shown_suggestions = self.settle_candidates(candidates);
        EngineAction::ShowCandidates(self.shown_suggestions.clone())
    }

    /// Process key in empty state
    pub(super) fn process_key_empty(&mut self, key: &KeyEvent, shift_active: bool) -> EngineResult {
        // Shift+Space on its own is the "I want a full-width space" gesture,
        // whatever the setting says. Committed directly, so no composition
        // opens for a second Space to convert.
        if shift_active && key.keysym == Keysym::SPACE && !key.modifiers.control_key {
            return EngineResult::consumed()
                .with_action(EngineAction::Commit("\u{3000}".to_string()));
        }

        // Bare Space from Empty: a full-width space is committed directly —
        // without entering Composing, where a second Space would open an
        // unwanted candidate window. A half-width one is passed through, so
        // the application keeps whatever it does with Space (scrolling a
        // page) when the IME has nothing to compose.
        if key.keysym == Keysym::SPACE && !key.modifiers.control_key && !key.modifiers.alt_key {
            return if self.space_char() == '\u{3000}' {
                EngineResult::consumed().with_action(EngineAction::Commit("\u{3000}".to_string()))
            } else {
                EngineResult::not_consumed()
            };
        }

        // `:` from Empty enters emoji shortcode mode. Accept both keysym
        // shapes a layout can emit for `:` — the `colon` keysym directly,
        // or `semicolon` with shift held.
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
        self.clear_composition();

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

    /// The space the Space key inputs: the configured one while typing
    /// kana, the ASCII one in direct input — the same edge the width rules
    /// stop at.
    pub(super) fn space_char(&self) -> char {
        match self.mode.current() {
            InputMode::Alphabet | InputMode::Emoji => ' ',
            InputMode::Hiragana | InputMode::Katakana => match self.config.space {
                SpaceStyle::Full => '\u{3000}',
                SpaceStyle::Half => ' ',
            },
        }
    }

    /// Insert the configured space after the active elements. Bare Space
    /// converts mid-composition, so Shift+Space is the only way to put a
    /// space into a composition — and the width wanted there is the
    /// everyday one, not the exception Shift+Space commits from Empty.
    pub(super) fn input_space(&mut self) -> EngineResult {
        let space = self.space_char();
        self.edit_with_chunk_breaks(|e| e.input_buf.push_direct(space));
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
                // Ctrl+J: start a new live-conversion chunk at the caret
                Keysym::KEY_J | Keysym::KEY_J_UPPER => return self.insert_chunk_break(),
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
                // Ctrl+R / Ctrl+T: start the conversion already narrowed
                // (first source / the cycle's tail), straight from typing —
                // no Space needed to reach the filtered view.
                Keysym::KEY_R | Keysym::KEY_R_UPPER => {
                    return self.start_filtered_conversion(FilterDirection::Backward);
                }
                Keysym::KEY_T | Keysym::KEY_T_UPPER => {
                    return self.start_filtered_conversion(FilterDirection::Forward);
                }
                _ => {}
            }
            // Ctrl+Y/U/I/O: jump straight to one source's view.
            if let Some(source) = source_for_key(key.keysym) {
                return self.jump_to_source(source);
            }
            // Ctrl+1..9: commit the numbered candidate from the suggestion
            // window. Bare digits stay plain text input, so numbers can be
            // typed mid-word without ever selecting a candidate.
            if let Some(digit) = key.keysym.digit_value() {
                return self.select_shown_candidate(digit);
            }
        }

        match key.keysym {
            Keysym::RETURN => self.commit_composing(),
            Keysym::ESCAPE => self.cancel_composing(),
            Keysym::BACKSPACE => self.backspace_composing(),
            Keysym::DELETE => self.delete_composing(),
            // Shift+Space: a space, since bare Space converts here.
            Keysym::SPACE if shift_active => self.input_space(),
            Keysym::SPACE if self.mode.current() == InputMode::Alphabet => {
                let space = self.space_char();
                self.input_char(space)
            }
            // Tab triggers conversion that bypasses the learning cache, so users
            // can escape stale or unwanted learned entries (mozc binds Tab to a
            // different conversion path — PredictAndConvert — in the same spirit).
            Keysym::TAB => self.start_conversion(LearningLookup::Skip),
            Keysym::SPACE | Keysym::DOWN => self.start_conversion(LearningLookup::Use),
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
        self.clear_composition();
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
        self.rewriter_variants(reading)
            .into_iter()
            .map(|(text, _desc)| text)
            .next()
    }

    /// Input a character during composing.
    /// In alphabet mode, inserts directly; otherwise goes through romaji conversion.
    pub(super) fn input_char(&mut self, ch: char) -> EngineResult {
        if matches!(self.mode.current(), InputMode::Alphabet | InputMode::Emoji) {
            self.edit_with_chunk_breaks(|e| e.input_buf.push_direct(ch));
            return self.refresh_input_state();
        }

        // PassThrough chars accumulate in the preedit alongside hiragana,
        // allowing users to compose `「」`, type `'word'`, and access symbol
        // variants from the candidate list.
        self.edit_with_chunk_breaks(|e| e.input_buf.push_romaji(ch, &e.converters.romaji));

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.refresh_input_state()
    }

    /// Resolve what committing the composition produces, as (reading, text).
    /// Settles pending romaji as a side effect. Emoji mode commits the first
    /// emoji candidate (falling back to the literal query), katakana mode
    /// commits katakana, live conversion commits the converted text.
    pub(super) fn resolve_composing_commit(&mut self) -> (String, String) {
        // Resolve the live text before settling: it needs the pending run
        let live_text = self.live_text_with_pending();
        self.settle_romaji();
        let reading = self.input_buf.reading();
        let text = if self.mode.current() == InputMode::Emoji {
            self.first_emoji_candidate(&reading)
                .unwrap_or_else(|| reading.clone())
        } else if self.mode.current() == InputMode::Katakana {
            karukan_engine::hiragana_to_katakana(&reading)
        } else if !live_text.is_empty() {
            live_text
        } else {
            reading.clone()
        };
        (reading, text)
    }

    /// Commit the current composition (Enter).
    pub(super) fn commit_composing(&mut self) -> EngineResult {
        let (reading, text) = self.resolve_composing_commit();

        if text.is_empty() {
            self.end_composition();
            return EngineResult::consumed()
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        self.record_learning(&reading, &text);
        self.end_composition();

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

        self.end_composition();

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
