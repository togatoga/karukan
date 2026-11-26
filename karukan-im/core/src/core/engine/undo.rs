//! Taking back what the last commit taught (Mozc's Revert).
//!
//! A commit teaches the learning cache through its lessons, and a wrong
//! pick is learned with the rest. Mozc's answer is the key that follows: a
//! Backspace right after a commit rolls the commit's learning back and still
//! reaches the application, which deletes the character as it would without
//! the IME. Nothing else changes, so the gesture costs nothing to learn —
//! deleting what was just committed is what the user does anyway.
//!
//! One commit can touch several units — the whole reading, a stretch of it,
//! and the default it displaced — so `learn` snapshots every unit a lesson
//! touches before applying it (`LearningCache::snapshot`), and the revert
//! puts them back newest first (`LearningCache::restore`): a unit touched
//! twice returns to the state it had before the first touch. The window is
//! the next key only: `dispatch_key` takes the snapshots before handling any
//! key press, so a commit is revertible exactly until something else is
//! typed, and `reset` (a focus change, a caret the application moved), the
//! focus-out `commit()` and a mouse pick close it. Modifier presses and key
//! releases never reach that point, so holding Shift keeps it open.

use karukan_engine::learning::UnitSnapshot;
use tracing::debug;

use super::*;

/// The units the last commit changed, as they stood before it.
#[derive(Debug, Default)]
pub(super) struct LearningUndo {
    snapshots: Vec<(String, String, UnitSnapshot)>,
}

impl LearningUndo {
    pub(super) fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.snapshots.clear();
    }

    /// Save `(reading, surface)` as it stands in `cache`, before a change.
    pub(super) fn save(&mut self, cache: &LearningCache, reading: &str, surface: &str) {
        let snapshot = cache.snapshot(reading, surface);
        self.snapshots
            .push((reading.to_string(), surface.to_string(), snapshot));
    }
}

/// Backspace with nothing but Shift held: what deletes the character the
/// commit typed. Ctrl/Alt/Super chords are the application's own shortcuts
/// (delete a word, …), which delete more than the last character.
pub(super) fn is_plain_backspace(key: &KeyEvent) -> bool {
    key.keysym == Keysym::BACKSPACE
        && !key.modifiers.control_key
        && !key.modifiers.alt_key
        && !key.modifiers.super_key
}

impl InputMethodEngine {
    /// Put back every unit `undo` saved, newest first.
    pub(super) fn revert_learning(&mut self, undo: LearningUndo) {
        let Some(cache) = &mut self.learning else {
            return;
        };
        for (reading, surface, snapshot) in undo.snapshots.into_iter().rev() {
            cache.restore(&reading, &surface, snapshot);
        }
        debug!("reverted the last commit's learning");
    }
}
