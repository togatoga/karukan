//! One karukan engine behind one IBus engine object, in IBus terms: X11
//! key events in, IBus signals out. Mirrors the fcitx5 frontend
//! (`karukan.cpp` and `src/ffi/`).

use karukan_im::config::Settings;
use karukan_im::core::candidate::{Candidate, CandidateList};
use karukan_im::core::engine::{EngineAction, EngineConfig, EngineResult, InputMethodEngine};
use karukan_im::core::keycode::{KeyEvent, KeyModifiers, Keysym};
use karukan_im::core::preedit::{AttributeType, Preedit};

use crate::ibus::{
    ATTR_TYPE_BACKGROUND, ATTR_TYPE_FOREGROUND, ATTR_TYPE_UNDERLINE, ATTR_UNDERLINE_DOUBLE,
    ATTR_UNDERLINE_SINGLE, Attribute, LookupTable, Text,
};

/// `IBusModifierType` (ibustypes.h).
const SHIFT_MASK: u32 = 1 << 0;
const CONTROL_MASK: u32 = 1 << 2;
const MOD1_MASK: u32 = 1 << 3;
const MOD4_MASK: u32 = 1 << 6;
const SUPER_MASK: u32 = 1 << 26;
const RELEASE_MASK: u32 = 1 << 30;

/// `IBUS_CAP_SURROUNDING_TEXT`.
const CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// Colors of the selected conversion in the preedit (0xRRGGBB).
const HIGHLIGHT_FOREGROUND: u32 = 0xffffff;
const HIGHLIGHT_BACKGROUND: u32 = 0x3584e4;

/// What the engine asks IBus to show, one item per D-Bus signal.
#[derive(Debug)]
pub enum Signal {
    Commit(Text),
    Preedit {
        text: Text,
        cursor: u32,
        visible: bool,
    },
    Aux(Text),
    HideAux,
    Lookup(LookupTable),
    HideLookup,
}

/// `ProcessKeyEvent`'s keyval (an X11 keysym, the representation the
/// fcitx5 frontend passes too) and modifier state, as a karukan key.
pub fn key_event(keyval: u32, state: u32) -> KeyEvent {
    let modifiers = KeyModifiers {
        shift_key: state & SHIFT_MASK != 0,
        control_key: state & CONTROL_MASK != 0,
        alt_key: state & MOD1_MASK != 0,
        super_key: state & (MOD4_MASK | SUPER_MASK) != 0,
    };
    KeyEvent::new(Keysym(keyval), modifiers, state & RELEASE_MASK == 0)
}

pub struct Session {
    pub engine: InputMethodEngine,
    /// `SetCapabilities`.
    pub capabilities: u32,
    /// The latest `SetSurroundingText`: text and cursor, in characters.
    surrounding: Option<(String, usize)>,
}

impl Session {
    /// A ready engine, set up as `karukan_engine_new` + `karukan_engine_init`
    /// do; the model loads in the background.
    pub fn new() -> Self {
        let settings = Settings::load_or_default();
        let mut engine = InputMethodEngine::with_config(EngineConfig::from_settings(&settings));
        if let Err(e) = engine.init_from_settings(&settings) {
            tracing::error!("Karukan init failed: {e:#}");
        }
        Self::with_engine(engine)
    }

    fn with_engine(engine: InputMethodEngine) -> Self {
        Self {
            engine,
            capabilities: 0,
            surrounding: None,
        }
    }

    /// Returns whether the key was consumed, and what to show.
    pub fn process_key(&mut self, keyval: u32, state: u32) -> (bool, Vec<Signal>) {
        let key = key_event(keyval, state);
        // The context is taken when a composition starts, as fcitx5 does.
        if key.is_press && self.engine.state().is_empty() {
            self.capture_surrounding_text();
        }
        let result = self.engine.process_key(&key);
        (result.consumed, signals(result))
    }

    /// A press of `keysym` alone, for the lookup table's buttons.
    pub fn press(&mut self, keysym: Keysym) -> Vec<Signal> {
        signals(self.engine.process_key(&KeyEvent::press(keysym)))
    }

    /// `CandidateClicked`: `index` is the row on the shown page.
    pub fn select_candidate(&mut self, index: u32) -> Vec<Signal> {
        signals(self.engine.select_candidate_on_page(index as usize))
    }

    pub fn set_surrounding_text(&mut self, text: String, cursor: u32) {
        self.surrounding = Some((text, cursor as usize));
    }

    fn capture_surrounding_text(&mut self) {
        let (text, cursor) = self.surrounding_context();
        let text = text.to_string();
        self.engine.set_surrounding_text_at(&text, cursor);
    }

    /// The editor text for the engine; empty without the capability, so
    /// a stale one never stays.
    fn surrounding_context(&self) -> (&str, usize) {
        match &self.surrounding {
            Some((text, cursor)) if self.capabilities & CAP_SURROUNDING_TEXT != 0 => {
                (text, *cursor)
            }
            _ => ("", 0),
        }
    }

    /// `Reset`: drop the composition, like the fcitx5 `reset`.
    pub fn reset(&mut self) -> Vec<Signal> {
        self.engine.reset();
        cleared()
    }

    /// `FocusOut` / `Disable`: commit what is pending, save the learning
    /// cache and forget the context, like the fcitx5 `deactivate`. The
    /// text itself is committed by IBus from the preedit (sent in
    /// `IBUS_ENGINE_PREEDIT_COMMIT` mode): by focus out the daemon has
    /// moved the engine off the context, so a `CommitText` from here
    /// would be lost. The engine commits only to learn it.
    pub fn deactivate(&mut self) -> Vec<Signal> {
        // Also clears the engine's context.
        self.engine.commit();
        self.engine.save_learning();
        self.surrounding = None;
        self.engine.reset();
        cleared()
    }
}

/// Everything hidden.
fn cleared() -> Vec<Signal> {
    vec![
        preedit(&Preedit::new()),
        Signal::HideAux,
        Signal::HideLookup,
    ]
}

/// The signals for one engine result. As in the fcitx5 frontend, a commit
/// clears the panel, and whatever the same result redraws is drawn after.
fn signals(result: EngineResult) -> Vec<Signal> {
    let mut commits = Vec::new();
    let mut preedit_update = None;
    let mut aux = None;
    let mut lookup = None;
    for action in result.actions {
        match action {
            EngineAction::Commit(text) => commits.push(text),
            EngineAction::UpdatePreedit(p) => preedit_update = Some(p),
            EngineAction::UpdateAuxText(text) => aux = Some(text),
            EngineAction::HideAuxText => aux = Some(String::new()),
            EngineAction::ShowCandidates(list) => lookup = Some(Some(list)),
            EngineAction::HideCandidates => lookup = Some(None),
        }
    }
    if !commits.is_empty() {
        preedit_update.get_or_insert_with(Preedit::new);
        aux.get_or_insert_with(String::new);
        lookup.get_or_insert(None);
    }
    let mut out: Vec<Signal> = commits
        .into_iter()
        .map(|text| Signal::Commit(Text::new(text)))
        .collect();
    out.extend(preedit_update.as_ref().map(preedit));
    out.extend(aux.map(|text| match text.is_empty() {
        true => Signal::HideAux,
        false => Signal::Aux(Text::new(text)),
    }));
    out.extend(lookup.map(|list| match list {
        Some(list) => Signal::Lookup(lookup_table(&list)),
        None => Signal::HideLookup,
    }));
    out
}

/// The preedit underlined whole, as the fcitx5 frontend draws it, with the
/// selected conversion in reverse colors. Positions are in characters, as
/// IBus counts them.
fn preedit(preedit: &Preedit) -> Signal {
    let len = preedit.text().chars().count();
    let mut attributes = vec![Attribute::new(
        ATTR_TYPE_UNDERLINE,
        ATTR_UNDERLINE_SINGLE,
        0,
        len,
    )];
    for a in preedit.attributes() {
        match a.attr_type {
            AttributeType::Underline => {}
            AttributeType::UnderlineDouble => attributes.push(Attribute::new(
                ATTR_TYPE_UNDERLINE,
                ATTR_UNDERLINE_DOUBLE,
                a.start,
                a.end,
            )),
            AttributeType::Highlight | AttributeType::Reverse => {
                attributes.push(Attribute::new(
                    ATTR_TYPE_FOREGROUND,
                    HIGHLIGHT_FOREGROUND,
                    a.start,
                    a.end,
                ));
                attributes.push(Attribute::new(
                    ATTR_TYPE_BACKGROUND,
                    HIGHLIGHT_BACKGROUND,
                    a.start,
                    a.end,
                ));
            }
        }
    }
    Signal::Preedit {
        text: Text::with_attributes(preedit.text(), attributes),
        cursor: preedit.caret() as u32,
        visible: len > 0,
    }
}

/// The shown page, the description beside each candidate in brackets
/// as the fcitx5 frontend shows it where it has no comment column.
fn lookup_table(list: &CandidateList) -> LookupTable {
    let candidates = list.page_candidates().iter().map(candidate_text).collect();
    LookupTable::new(list.page_size(), candidates, list.page_cursor())
}

fn candidate_text(candidate: &Candidate) -> Text {
    match candidate.description.as_deref().filter(|d| !d.is_empty()) {
        Some(description) => Text::new(format!("{} [{description}]", candidate.text)),
        None => Text::new(candidate.text.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_event_reads_x11_state() {
        let key = key_event(0x61, 0);
        assert_eq!(key.keysym, Keysym(0x61));
        assert!(key.is_press);
        assert!(!key.modifiers.any());

        // Shift+a arrives as the keysym A with the Shift bit.
        let key = key_event(0x41, SHIFT_MASK);
        assert_eq!(key.keysym, Keysym::KEY_A_UPPER);
        assert!(key.modifiers.shift_key);

        let key = key_event(0x72, CONTROL_MASK | MOD1_MASK);
        assert!(key.modifiers.control_key && key.modifiers.alt_key);
        assert!(!key.modifiers.shift_key && !key.modifiers.super_key);

        // Super shows as Mod4, the virtual Super bit, or both.
        assert!(key_event(0x61, MOD4_MASK).modifiers.super_key);
        assert!(key_event(0x61, SUPER_MASK).modifiers.super_key);

        let key = key_event(Keysym::RETURN.0, RELEASE_MASK);
        assert!(!key.is_press);
        assert_eq!(key.keysym, Keysym::RETURN);
    }

    fn session() -> Session {
        Session::with_engine(InputMethodEngine::new())
    }

    fn type_keys(session: &mut Session, keys: &str) -> Vec<Signal> {
        keys.chars()
            .flat_map(|c| session.process_key(c as u32, 0).1)
            .collect()
    }

    fn last_preedit(signals: &[Signal]) -> Option<&Text> {
        signals.iter().rev().find_map(|s| match s {
            Signal::Preedit { text, .. } => Some(text),
            _ => None,
        })
    }

    #[test]
    fn typing_shows_an_underlined_preedit() {
        let mut session = session();
        let signals = type_keys(&mut session, "kyou");
        let Some(Signal::Preedit { text, cursor, .. }) = signals
            .iter()
            .rev()
            .find(|s| matches!(s, Signal::Preedit { .. }))
        else {
            panic!("no preedit in {signals:?}")
        };
        assert_eq!(text.text(), "きょう");
        assert_eq!(*cursor, 3);
        let attrs: Vec<_> = text.attributes().collect();
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn enter_commits_and_clears() {
        let mut session = session();
        type_keys(&mut session, "kyou");
        let (consumed, signals) = session.process_key(Keysym::RETURN.0, 0);
        assert!(consumed);
        let Signal::Commit(text) = &signals[0] else {
            panic!("commit first: {signals:?}")
        };
        assert_eq!(text.text(), "きょう");
        assert_eq!(last_preedit(&signals).map(Text::text), Some(""));
        assert!(signals.iter().any(|s| matches!(s, Signal::HideLookup)));
        assert!(signals.iter().any(|s| matches!(s, Signal::HideAux)));
        // The release of Enter is not the engine's.
        assert!(!session.process_key(Keysym::RETURN.0, RELEASE_MASK).0);
    }

    #[test]
    fn space_highlights_the_conversion_and_shows_the_page() {
        let mut session = session();
        type_keys(&mut session, "kyou");
        let (_, signals) = session.process_key(Keysym::SPACE.0, 0);
        let text = last_preedit(&signals).expect("preedit");
        let kinds: Vec<_> = text.attributes().map(Attribute::kind).collect();
        assert_eq!(
            kinds,
            [
                ATTR_TYPE_UNDERLINE,
                ATTR_TYPE_FOREGROUND,
                ATTR_TYPE_BACKGROUND
            ]
        );
        let Some(Signal::Lookup(table)) = signals.iter().find(|s| matches!(s, Signal::Lookup(_)))
        else {
            panic!("no lookup table: {signals:?}")
        };
        assert!(table.candidates().count() <= CandidateList::DEFAULT_PAGE_SIZE);
        assert_eq!(table.labels().next(), Some("1"));
    }

    #[test]
    fn deactivate_learns_without_committing() {
        let mut session = session();
        type_keys(&mut session, "kyou");
        session.process_key(Keysym::SPACE.0, 0);
        let signals = session.deactivate();
        // IBus commits the preedit itself; a commit here would double it.
        assert!(signals.iter().all(|s| !matches!(s, Signal::Commit(_))));
        assert_eq!(last_preedit(&signals).map(Text::text), Some(""));
        assert!(session.engine.state().is_empty());
    }

    #[test]
    fn reset_drops_the_composition() {
        let mut session = session();
        type_keys(&mut session, "kyou");
        let signals = session.reset();
        assert!(signals.iter().all(|s| !matches!(s, Signal::Commit(_))));
        assert_eq!(last_preedit(&signals).map(Text::text), Some(""));
        assert!(session.engine.state().is_empty());
    }

    #[test]
    fn surrounding_text_needs_the_capability() {
        let mut session = session();
        session.set_surrounding_text("今日は".to_string(), 2);
        assert_eq!(session.surrounding_context(), ("", 0));
        session.capabilities = CAP_SURROUNDING_TEXT;
        assert_eq!(session.surrounding_context(), ("今日は", 2));
        type_keys(&mut session, "kyou");
        session.deactivate();
        assert_eq!(session.surrounding_context(), ("", 0));
    }
}
