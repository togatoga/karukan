//! Settings switched by typing their name. Converting the reading of a
//! setting (でばっぐもーど) offers a 【…をオン】/【…をオフ】 candidate;
//! choosing it flips the setting and inputs nothing. No key chord is
//! taken from the application for these.

use super::*;

/// A setting that has a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    /// The aux line's debug details (`[display] verbose`).
    Verbose,
}

impl Command {
    /// The setting `reading` names. でばっく is the common misspelling.
    pub(super) fn for_reading(reading: &str) -> Option<Self> {
        match reading {
            "でばっぐもーど" | "でばっくもーど" => Some(Self::Verbose),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Verbose => "デバッグモード",
        }
    }
}

impl InputMethodEngine {
    fn is_on(&self, command: Command) -> bool {
        match command {
            Command::Verbose => self.config.verbose,
        }
    }

    /// The candidate that flips the setting `reading` names, if any.
    pub(super) fn command_candidate(&self, reading: &str) -> Option<AnnotatedCandidate> {
        let command = Command::for_reading(reading)?;
        let to = if self.is_on(command) {
            "オフ"
        } else {
            "オン"
        };
        Some(
            AnnotatedCandidate::new(
                format!("【{}を{to}】", command.name()),
                CandidateSource::Command,
            )
            .with_description(Some("設定".to_string())),
        )
    }

    /// The setting the selected candidate flips, if it is a command.
    pub(super) fn selected_command(&self) -> Option<Command> {
        // Commands only ever sit in a conversion's own list.
        let selected = self
            .state
            .candidates()?
            .selected()
            .filter(|c| c.source == Some(CandidateSource::Command))?;
        Command::for_reading(selected.reading.as_deref()?)
    }

    /// The selected candidate is a command: flip its setting, end the
    /// composition without committing, and say what changed on the aux
    /// line. None when the selection is not a command.
    pub(super) fn run_selected_command(&mut self) -> Option<EngineResult> {
        let command = self.selected_command()?;
        match command {
            Command::Verbose => self.config.verbose = !self.config.verbose,
        }
        self.end_composition();
        let state = if self.is_on(command) {
            "オン"
        } else {
            "オフ"
        };
        // An empty list rather than HideCandidates: on macOS the candidate
        // panel is what draws the aux line.
        Some(
            EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::ShowCandidates(CandidateList::new(Vec::new())))
                .with_action(EngineAction::UpdateAuxText(format!(
                    "{}: {state}",
                    command.name()
                ))),
        )
    }
}
