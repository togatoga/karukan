//! The overload guard: what keeps typing possible when the model cannot
//! keep up.
//!
//! A conversion is a keystroke waiting to be shown. On a machine busy with
//! something else — a game holding every fast core — one can take seconds,
//! and every keystroke typed meanwhile queues up behind it. So a conversion
//! is given a time budget, and a model that overruns it is rested: the
//! engine stops calling it for a while and carries on with what is left —
//! the light model while only the main one rests, then the dictionaries and
//! plain kana. A rest ends with one more attempt; if that overruns too, the
//! next rest is twice as long.

use std::time::{Duration, Instant};

use super::cache::ModelRole;

/// How long a model rests after its first overrun.
const FIRST_REST: Duration = Duration::from_secs(2);

/// Where the doubling stops.
const LONGEST_REST: Duration = Duration::from_secs(60);

/// A beam search may take this many times the budget of a live conversion:
/// Space is pressed once and waited for, a live conversion is paid on every
/// keystroke.
const BEAM_BUDGET_FACTOR: u32 = 4;

/// One model's rest: until when, and how long it was.
#[derive(Debug, Clone, Copy, Default)]
struct Rest {
    /// The model is not called before this instant. Stays set once passed —
    /// the next call is the attempt that decides whether the rest is over.
    until: Option<Instant>,
    /// Length of the last rest, which the next overrun doubles. Zero while
    /// the model has been finishing in time.
    length: Duration,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OverloadGuard {
    /// Time one live conversion may take; `None` turns the guard off.
    budget: Option<Duration>,
    main: Rest,
    light: Rest,
}

impl OverloadGuard {
    /// A guard giving each live conversion `timeout_ms` (0 = no limit).
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            budget: (timeout_ms > 0).then(|| Duration::from_millis(timeout_ms)),
            ..Self::default()
        }
    }

    /// When a conversion for `num_candidates` started at `now` has to be
    /// done. `None` when the guard is off.
    pub fn deadline(&self, now: Instant, num_candidates: usize) -> Option<Instant> {
        let budget = self.budget?;
        let factor = if num_candidates > 1 {
            BEAM_BUDGET_FACTOR
        } else {
            1
        };
        Some(now + budget * factor)
    }

    /// Whether `model` is resting at `now` and must not be called.
    pub fn is_resting(&self, model: ModelRole, now: Instant) -> bool {
        self.rest(model).until.is_some_and(|until| now < until)
    }

    /// `model` overran its budget in a call that ended at `now`: rest it,
    /// twice as long as the last time unless it finished in time since.
    pub fn overran(&mut self, model: ModelRole, now: Instant) {
        let rest = self.rest_mut(model);
        rest.length = if rest.length.is_zero() {
            FIRST_REST
        } else {
            (rest.length * 2).min(LONGEST_REST)
        };
        rest.until = Some(now + rest.length);
    }

    /// `model` finished a call in time: whatever slowed it down is over.
    pub fn finished_in_time(&mut self, model: ModelRole) {
        *self.rest_mut(model) = Rest::default();
    }

    fn rest(&self, model: ModelRole) -> &Rest {
        match model {
            ModelRole::Main => &self.main,
            ModelRole::Light => &self.light,
        }
    }

    fn rest_mut(&mut self, model: ModelRole) -> &mut Rest {
        match model {
            ModelRole::Main => &mut self.main,
            ModelRole::Light => &mut self.light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: ModelRole = ModelRole::Main;
    const LIGHT: ModelRole = ModelRole::Light;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_model_that_keeps_up_is_never_rested() {
        let mut guard = OverloadGuard::new(500);
        let now = Instant::now();
        assert!(!guard.is_resting(MAIN, now));
        guard.finished_in_time(MAIN);
        assert!(!guard.is_resting(MAIN, now));
    }

    #[test]
    fn an_overrun_rests_the_model_and_the_rest_ends() {
        let mut guard = OverloadGuard::new(500);
        let now = Instant::now();
        guard.overran(MAIN, now);
        assert!(guard.is_resting(MAIN, now));
        assert!(guard.is_resting(MAIN, now + FIRST_REST - Duration::from_millis(1)));
        assert!(!guard.is_resting(MAIN, now + FIRST_REST));
    }

    #[test]
    fn each_model_rests_on_its_own() {
        let mut guard = OverloadGuard::new(500);
        let now = Instant::now();
        guard.overran(MAIN, now);
        assert!(!guard.is_resting(LIGHT, now));
        guard.overran(LIGHT, now);
        guard.finished_in_time(MAIN);
        assert!(!guard.is_resting(MAIN, now));
        assert!(guard.is_resting(LIGHT, now));
    }

    #[test]
    fn overruns_in_a_row_double_the_rest_up_to_the_longest() {
        let mut guard = OverloadGuard::new(500);
        let mut now = Instant::now();
        let mut lengths = Vec::new();
        for _ in 0..8 {
            guard.overran(MAIN, now);
            let until = guard.main.until.expect("resting");
            lengths.push(until - now);
            // The attempt that ends the rest overruns again.
            now = until;
        }
        assert_eq!(
            lengths,
            [2, 4, 8, 16, 32, 60, 60, 60].map(secs),
            "2s doubling to the 60s cap"
        );
    }

    #[test]
    fn finishing_in_time_starts_the_doubling_over() {
        let mut guard = OverloadGuard::new(500);
        let now = Instant::now();
        guard.overran(MAIN, now);
        guard.overran(MAIN, now);
        guard.finished_in_time(MAIN);
        guard.overran(MAIN, now);
        assert_eq!(guard.main.until, Some(now + FIRST_REST));
    }

    #[test]
    fn a_beam_search_gets_a_longer_budget_than_a_live_conversion() {
        let guard = OverloadGuard::new(500);
        let now = Instant::now();
        assert_eq!(
            guard.deadline(now, 1),
            Some(now + Duration::from_millis(500))
        );
        assert_eq!(
            guard.deadline(now, 9),
            Some(now + Duration::from_millis(2000))
        );
    }

    #[test]
    fn a_zero_timeout_turns_the_guard_off() {
        let guard = OverloadGuard::new(0);
        assert_eq!(guard.deadline(Instant::now(), 1), None);
        assert_eq!(guard.deadline(Instant::now(), 9), None);
    }
}
