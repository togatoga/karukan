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

    /// The selected candidate is a command: flip its setting, end the
    /// composition without committing, and say what changed on the aux
    /// line. None when the selection is not a command.
    pub(super) fn run_selected_command(&mut self) -> Option<EngineResult> {
        // Commands only ever sit in a conversion's own list.
        let reading = self
            .state
            .candidates()?
            .selected()
            .filter(|c| c.source == Some(CandidateSource::Command))?
            .reading
            .clone()?;
        let command = Command::for_reading(&reading)?;
        match command {
            Command::Verbose => self.config.verbose = !self.config.verbose,
        }
        self.end_composition();
        let state = if self.is_on(command) {
            "オン"
        } else {
            "オフ"
        };
        Some(
            EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::UpdateAuxText(format!(
                    "{}: {state}",
                    command.name()
                ))),
        )
    }
}
