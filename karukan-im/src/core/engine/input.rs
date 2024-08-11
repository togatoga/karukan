//! Composing input handling (Empty and Composing states)

use karukan_engine::ConversionEvent;

use super::*;

/// Append candidates to `target`, skipping duplicates and updating indices.
fn append_candidates_dedup(target: &mut Vec<Candidate>, source: Vec<Candidate>) {
    for mut c in source {
        if !target.iter().any(|existing| existing.text == c.text) {
            c.index = target.len();
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
            // No useful AI suggestion — still show learning + dictionary candidates
            self.live.text.clear();
            let preedit = self.set_composing_state();
            let reading = self.input_buf.text.clone();
            let mut all_candidates = self.lookup_learning_candidates(&reading);
            append_candidates_dedup(&mut all_candidates, self.lookup_dict_candidates(&reading));
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
            let event = self.converters.romaji.push(ch);
            let romaji_buffer = self.converters.romaji.buffer().to_string();

            // Check for PassThrough FIRST: the converter adds PassThrough chars
            // to its output, so we must check the event before checking output emptiness.
            // Exception: digits should enter Composing so users can type "20世紀" etc.
            if let ConversionEvent::PassThrough(c) = event
                && !c.is_ascii_digit()
            {
                self.converters.romaji.reset();
                return EngineResult::consumed().with_action(EngineAction::Commit(c.to_string()));
            }
            // For digits, fall through to enter Composing normally

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
            Keysym::SPACE | Keysym::DOWN | Keysym::TAB => self.start_conversion(),
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

    /// Input a character during composing.
    /// In alphabet mode, inserts directly; otherwise goes through romaji conversion.
    pub(super) fn input_char(&mut self, ch: char) -> EngineResult {
        if self.input_mode == InputMode::Alphabet {
            self.input_buf.insert(&ch.to_string());
            return self.refresh_input_state();
        }

        let prev_output_len = self.converters.romaji.output().chars().count();
        let event = self.converters.romaji.push(ch);
        let curr_output_len = self.converters.romaji.output().chars().count();
        let romaji_buffer = self.converters.romaji.buffer().to_string();

        // Track whether composed_hiragana was empty before processing.
        // Used to decide if PassThrough chars should auto-commit (standalone punctuation)
        // or stay in preedit (punctuation after hiragana).
        let composed_was_empty = self.input_buf.text.is_empty();

        // Consume ALL new converter output into composed_hiragana at cursor position.
        // This must happen BEFORE PassThrough handling because the converter may
        // recursively pass through multiple chars (e.g., "thx" → output="th", buffer="x",
        // event=PassThrough('h')), and we need to capture all of them via delta detection.
        // PassThrough chars are already included in the converter output, so we must NOT
        // also insert them separately.
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

        // Handle pass-through: only auto-commit when there was no previous hiragana
        // (standalone punctuation). If hiragana was already in the preedit, keep the
        // passthrough chars in composed_hiragana (already inserted by delta detection).
        // Exception: digits always stay in preedit to allow "20世紀" style input.
        if let ConversionEvent::PassThrough(c) = event
            && !c.is_ascii_digit()
            && composed_was_empty
            && romaji_buffer.is_empty()
        {
            let text = self.input_buf.text.clone();
            self.input_buf.clear();
            self.state = InputState::Empty;
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideAuxText)
                .with_action(EngineAction::Commit(text));
        }

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
        let text = if self.input_mode == InputMode::Katakana {
            // Katakana mode always commits katakana, ignoring live conversion
            Self::hiragana_to_katakana(&reading)
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

        // Record live conversion result in learning cache
        self.record_learning(&reading, &text);

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;

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

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
    }
}
