//! Taking back what the last commit taught.
//!
//! A wrong pick is learned with the rest of a commit. The key that follows
//! takes it back: a Backspace right after a commit restores every unit the
//! commit touched and still reaches the application, which deletes the
//! character as it would without the IME. Deleting what was just committed
//! is what the user does anyway, so there is nothing new to learn.
//!
//! Every lesson a commit records saves its unit first
//! (`LearningCache::snapshot`), and the revert puts each back newest first,
//! so a unit touched twice in one commit returns to its first state. The
//! window is the next key only: `dispatch_key` takes the snapshots before
//! handling any key press, and `reset` (a focus change, a caret the
//! application moved) and a focus-out commit close it too. Modifier presses
//! and key releases never reach that point, so holding Shift keeps it open.

use karukan_engine::LearningSnapshot;
use tracing::debug;

use super::*;

/// The units the last commit changed, as they stood before it.
#[derive(Debug, Default)]
pub(super) struct LearningUndo {
    snapshots: Vec<(String, String, LearningSnapshot)>,
}

impl LearningUndo {
    pub(super) fn push(&mut self, reading: &str, surface: &str, snapshot: LearningSnapshot) {
        self.snapshots
            .push((reading.to_string(), surface.to_string(), snapshot));
    }

    pub(super) fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.snapshots.clear();
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
        let Some(mut cache) = self.learning_cache() else {
            return;
        };
        for (reading, surface, snapshot) in undo.snapshots.into_iter().rev() {
            cache.restore(&reading, &surface, snapshot);
        }
        debug!("reverted the last commit's learning");
    }
}
