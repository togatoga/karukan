//! Learning cache: what the user did with conversions, as decaying evidence.
//!
//! The unit is a stretch of reading with a surface. Each unit keeps one
//! value per kind of [`Evidence`], decaying with a half-life of
//! [`HALF_LIFE_SECS`] (`v ← v·2^(−Δt/H) + w`, so only the value and the
//! time it was last updated are stored), plus the same tally split by up to
//! [`MAX_CONTEXTS`] contexts: the last [`CONTEXT_CHARS`] chars of the text
//! before the unit. A unit's score is [`Evidence::WEIGHTS`] applied to its
//! tally, plus its tally under the context being looked up when there is
//! one.
//!
//! Persisted as TSV (see [`LearningCache::save`]); a file in the earlier
//! four-column format (`reading surface frequency last_access`) still
//! loads, each row as `confirmed` evidence.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// How long evidence takes to lose half its weight.
pub const HALF_LIFE_SECS: f64 = 30.0 * 86400.0;

/// Chars of preceding text a context is keyed by.
pub const CONTEXT_CHARS: usize = 2;

/// Contexts kept per unit; the weakest goes when another arrives.
pub const MAX_CONTEXTS: usize = 4;

/// First line of a file in the current format.
const HEADER: &str = "# karukan learning cache v2";

/// What happened to a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// Picked over what was shown as the default.
    Chosen,
    /// Kept as the default after the list was opened or the segment focused.
    Confirmed,
    /// Committed as shown, possibly unseen.
    Accepted,
    /// Shown as the default, then replaced by another candidate.
    Displaced,
}

impl Evidence {
    /// Every kind, in the order of a tally's values and of the TSV columns.
    pub const ALL: [Self; 4] = [
        Self::Chosen,
        Self::Confirmed,
        Self::Accepted,
        Self::Displaced,
    ];

    /// What one unit of each kind adds to a score, in [`Self::ALL`] order.
    pub const WEIGHTS: [f64; 4] = [1.0, 0.4, 0.1, -1.0];

    fn index(self) -> usize {
        self as usize
    }
}

/// Size limits for a [`LearningCache`].
///
/// Passed whole at construction ([`LearningCache::new`] /
/// [`LearningCache::load`]) so a caller can't apply one limit and forget the
/// other — there is no post-construction setter to miss.
#[derive(Debug, Clone, Copy)]
pub struct LearningConfig {
    /// Maximum number of units across all readings; the ones with the least
    /// evidence are evicted on save when over this limit.
    pub max_entries: usize,
    /// Maximum surface length (Unicode chars) that [`LearningCache::record`]
    /// accepts. Keeps whole-sentence live-conversion commits — one-off text
    /// that never matches again — out of the cache.
    pub max_surface_chars: usize,
}

impl LearningConfig {
    /// Default for [`max_entries`](Self::max_entries).
    pub const DEFAULT_MAX_ENTRIES: usize = 10_000;
    /// Default for [`max_surface_chars`](Self::max_surface_chars).
    pub const DEFAULT_MAX_SURFACE_CHARS: usize = 50;
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            max_surface_chars: Self::DEFAULT_MAX_SURFACE_CHARS,
        }
    }
}

/// How much of a value recorded at `updated` is left at `now`.
fn decay(updated: u64, now: u64) -> f64 {
    let elapsed = now.saturating_sub(updated) as f64;
    (-elapsed / HALF_LIFE_SECS).exp2()
}

/// One value per kind of evidence, all as of `updated`.
#[derive(Debug, Clone, Default, PartialEq)]
struct Tally {
    values: [f64; 4],
    updated: u64,
}

impl Tally {
    fn at(&self, now: u64) -> [f64; 4] {
        let factor = decay(self.updated, now);
        self.values.map(|v| v * factor)
    }

    fn add(&mut self, evidence: Evidence, weight: f64, now: u64) {
        self.values = self.at(now);
        self.values[evidence.index()] += weight;
        self.updated = self.updated.max(now);
    }

    fn score(&self, now: u64) -> f64 {
        self.at(now)
            .iter()
            .zip(Evidence::WEIGHTS)
            .map(|(v, w)| v * w)
            .sum()
    }

    /// How much evidence there is, whichever way it points.
    fn mass(&self, now: u64) -> f64 {
        self.at(now).iter().sum()
    }
}

/// A surface learned for a reading.
#[derive(Debug, Clone)]
struct Unit {
    surface: String,
    tally: Tally,
    /// The same evidence, split by the context it was recorded in.
    contexts: Vec<(String, Tally)>,
}

impl Unit {
    fn in_context(&self, context: Option<&str>) -> Option<&Tally> {
        let context = context?;
        self.contexts
            .iter()
            .find(|(c, _)| c == context)
            .map(|(_, t)| t)
    }

    fn score(&self, context: Option<&str>, now: u64) -> f64 {
        let in_context = self.in_context(context).map_or(0.0, |t| t.score(now));
        self.tally.score(now) + in_context
    }

    /// How many contexts it was picked in.
    fn chosen_contexts(&self) -> usize {
        self.contexts
            .iter()
            .filter(|(_, t)| t.values[Evidence::Chosen.index()] > 0.0)
            .count()
    }
}

/// A place in a reading where the user moved a segment edge to, as a
/// decaying count.
#[derive(Debug, Clone)]
struct Boundary {
    position: usize,
    value: f64,
    updated: u64,
}

/// In-memory cache of user learning data, keyed by reading (hiragana).
#[derive(Debug)]
pub struct LearningCache {
    entries: HashMap<String, Vec<Unit>>,
    boundaries: HashMap<String, Vec<Boundary>>,
    max_entries: usize,
    max_surface_chars: usize,
    dirty: bool,
}

/// The key a context is stored under: the tail of the preceding text (empty
/// at the start of the input, which is a context too), or none when that
/// tail holds a tab or a newline the TSV cannot carry.
fn context_key(preceding: &str) -> Option<String> {
    let count = preceding.chars().count();
    let key: String = preceding
        .chars()
        .skip(count.saturating_sub(CONTEXT_CHARS))
        .collect();
    (!key.chars().any(char::is_control)).then_some(key)
}

impl LearningCache {
    /// Create an empty cache with the given limits.
    pub fn new(config: LearningConfig) -> Self {
        Self {
            entries: HashMap::new(),
            boundaries: HashMap::new(),
            max_entries: config.max_entries,
            max_surface_chars: config.max_surface_chars,
            dirty: false,
        }
    }

    /// Record `weight` of `evidence` for `surface` under `reading`, after
    /// the text `preceding` (empty for none; only its tail is kept).
    ///
    /// Surfaces longer than `max_surface_chars` are skipped; see
    /// [`LearningConfig::max_surface_chars`] for why.
    pub fn record(
        &mut self,
        reading: &str,
        surface: &str,
        preceding: &str,
        evidence: Evidence,
        weight: f64,
    ) {
        if reading.is_empty()
            || surface.is_empty()
            || surface.chars().count() > self.max_surface_chars
        {
            return;
        }
        let now = now_unix();
        let unit = self.unit_mut(reading, surface);
        unit.tally.add(evidence, weight, now);
        if let Some(context) = context_key(preceding) {
            let at = match unit.contexts.iter().position(|(c, _)| *c == context) {
                Some(at) => at,
                None => {
                    if unit.contexts.len() >= MAX_CONTEXTS {
                        let weakest = (0..unit.contexts.len())
                            .min_by(|&a, &b| {
                                let mass = |i: usize| unit.contexts[i].1.mass(now);
                                mass(a).total_cmp(&mass(b))
                            })
                            .expect("full");
                        unit.contexts.remove(weakest);
                    }
                    unit.contexts.push((context, Tally::default()));
                    unit.contexts.len() - 1
                }
            };
            unit.contexts[at].1.add(evidence, weight, now);
        }
        self.dirty = true;
    }

    /// Record that the user put a segment edge at `position` (chars) of
    /// `reading`.
    pub fn record_boundary(&mut self, reading: &str, position: usize) {
        let now = now_unix();
        let boundaries = self.boundaries.entry(reading.to_string()).or_default();
        match boundaries.iter_mut().find(|b| b.position == position) {
            Some(b) => {
                b.value = b.value * decay(b.updated, now) + 1.0;
                b.updated = b.updated.max(now);
            }
            None => boundaries.push(Boundary {
                position,
                value: 1.0,
                updated: now,
            }),
        }
        self.dirty = true;
    }

    /// The segment edge the user has put in `reading` most, if any.
    pub fn preferred_boundary(&self, reading: &str) -> Option<usize> {
        let now = now_unix();
        self.boundaries
            .get(reading)?
            .iter()
            .max_by(|a, b| {
                (a.value * decay(a.updated, now)).total_cmp(&(b.value * decay(b.updated, now)))
            })
            .map(|b| b.position)
    }

    /// Remove every learned unit that would resurface `surface` for input
    /// `reading`: the exact-reading unit plus every longer reading with
    /// `reading` as a prefix (the [`prefix_lookup`](Self::prefix_lookup)
    /// fan-out) — an exact-only delete would leave a twin that pops back on
    /// the next conversion. Returns whether anything was removed; persisted
    /// at the next `save`.
    pub fn remove_suggestion(&mut self, reading: &str, surface: &str) -> bool {
        let mut removed = false;
        self.entries.retain(|r, units| {
            if !r.starts_with(reading) {
                return true;
            }
            let before = units.len();
            units.retain(|u| u.surface != surface);
            removed |= units.len() != before;
            !units.is_empty()
        });
        if removed {
            self.dirty = true;
        }
        removed
    }

    /// Record `accepted` for a unit that already exists, leaving the rest
    /// alone: committing what was shown keeps a learned unit fresh, and
    /// would otherwise make the model's own output a unit of its own.
    pub fn reinforce(&mut self, reading: &str, surface: &str, preceding: &str) {
        let exists = self
            .entries
            .get(reading)
            .is_some_and(|units| units.iter().any(|u| u.surface == surface));
        if exists {
            self.record(reading, surface, preceding, Evidence::Accepted, 1.0);
        }
    }

    /// Whether `surface` was ever picked or kept on purpose for `reading`,
    /// not only committed as shown.
    pub fn picked(&self, reading: &str, surface: &str) -> bool {
        let now = now_unix();
        self.entries
            .get(reading)
            .and_then(|units| units.iter().find(|u| u.surface == surface))
            .is_some_and(|u| {
                let v = u.tally.at(now);
                v[Evidence::Chosen.index()] + v[Evidence::Confirmed.index()] > 0.0
            })
    }

    /// What to show in place of `surface` for `reading` after `preceding`,
    /// when the history asks for it by more than `margin`:
    ///
    /// - a surface chosen in this very context, compared by what was
    ///   recorded in it alone;
    /// - else a surface chosen in two contexts or more, a preference of
    ///   spelling (子ども) that holds anywhere, compared by all of it;
    /// - else nothing: evidence from another context only orders lists.
    pub fn replacement(
        &self,
        reading: &str,
        surface: &str,
        preceding: &str,
        margin: f64,
    ) -> Option<String> {
        let units = self.entries.get(reading)?;
        let context = context_key(preceding);
        let context = context.as_deref();
        let now = now_unix();
        let current = units.iter().find(|u| u.surface == surface);
        let mut best: Option<(f64, &str)> = None;
        for unit in units.iter().filter(|u| u.surface != surface) {
            let chosen_here = unit
                .in_context(context)
                .is_some_and(|t| t.values[Evidence::Chosen.index()] > 0.0);
            let (score, against) = if chosen_here {
                let here = |u: &Unit| u.in_context(context).map_or(0.0, |t| t.score(now));
                (here(unit), current.map_or(0.0, here))
            } else if unit.chosen_contexts() >= 2 {
                let all = |u: &Unit| u.tally.score(now);
                (all(unit), current.map_or(0.0, all))
            } else {
                continue;
            };
            if score > against + margin && best.is_none_or(|(b, _)| score > b) {
                best = Some((score, &unit.surface));
            }
        }
        best.map(|(_, s)| s.to_string())
    }

    /// Whether anything is learned under `reading`.
    pub fn contains_reading(&self, reading: &str) -> bool {
        self.entries.contains_key(reading)
    }

    /// Score of `surface` under `reading` after `preceding`; 0 when nothing
    /// is learned for it.
    pub fn score(&self, reading: &str, surface: &str, preceding: &str) -> f64 {
        let context = context_key(preceding);
        let now = now_unix();
        self.entries
            .get(reading)
            .and_then(|units| units.iter().find(|u| u.surface == surface))
            .map_or(0.0, |u| u.score(context.as_deref(), now))
    }

    /// Every surface learned under `reading` with its score after
    /// `preceding`, best first.
    pub fn lookup(&self, reading: &str, preceding: &str) -> Vec<(String, f64)> {
        let context = context_key(preceding);
        let now = now_unix();
        let Some(units) = self.entries.get(reading) else {
            return Vec::new();
        };
        let mut scored: Vec<(String, f64)> = units
            .iter()
            .map(|u| (u.surface.clone(), u.score(context.as_deref(), now)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored
    }

    /// Score of each of `surfaces` summed over every reading it was
    /// learned under: how much the text itself is used, whatever it was
    /// typed as. Surfaces never learned are absent.
    pub fn surface_scores<'a>(
        &self,
        surfaces: impl IntoIterator<Item = &'a str>,
    ) -> HashMap<String, f64> {
        let wanted: HashSet<&str> = surfaces.into_iter().collect();
        let now = now_unix();
        let mut scores = HashMap::new();
        for unit in self.entries.values().flatten() {
            if wanted.contains(unit.surface.as_str()) {
                *scores.entry(unit.surface.clone()).or_insert(0.0) += unit.score(None, now);
            }
        }
        scores
    }

    /// `(reading, surface, score)` for every reading that starts with
    /// `prefix`, best first.
    pub fn prefix_lookup(&self, prefix: &str, preceding: &str) -> Vec<(String, String, f64)> {
        let context = context_key(preceding);
        let now = now_unix();
        let mut results: Vec<(String, String, f64)> = Vec::new();
        for (reading, units) in &self.entries {
            if reading.starts_with(prefix) {
                for unit in units {
                    results.push((
                        reading.clone(),
                        unit.surface.clone(),
                        unit.score(context.as_deref(), now),
                    ));
                }
            }
        }
        results.sort_by(|a, b| b.2.total_cmp(&a.2));
        results
    }

    /// Load a learning cache from a TSV file, in either format.
    ///
    /// Lines starting with `#` are comments; malformed lines are skipped.
    pub fn load(path: &Path, config: LearningConfig) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut cache = Self::new(config);
        let mut current = false;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            let line = line.trim_end_matches(['\r', '\n']);
            if i == 0 && line == HEADER {
                current = true;
                continue;
            }
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if current {
                cache.load_row(&parts);
            } else {
                cache.load_legacy_row(&parts);
            }
        }

        // Not dirty — just loaded from disk
        cache.dirty = false;
        Ok(cache)
    }

    /// `reading surface frequency last_access`: which kind of evidence the
    /// count was is unknown, so it is taken as `confirmed`.
    fn load_legacy_row(&mut self, parts: &[&str]) {
        let &[reading, surface, frequency, last_access, ..] = parts else {
            return;
        };
        let (Ok(frequency), Ok(last_access)) = (frequency.parse::<u32>(), last_access.parse())
        else {
            return;
        };
        let mut tally = Tally {
            values: [0.0; 4],
            updated: last_access,
        };
        tally.values[Evidence::Confirmed.index()] = frequency as f64;
        self.unit_mut(reading, surface).tally = tally;
    }

    fn load_row(&mut self, parts: &[&str]) {
        let tally = |fields: &[&str]| -> Option<Tally> {
            let [updated, values @ ..] = fields else {
                return None;
            };
            let values: Vec<f64> = values
                .iter()
                .map(|v| v.parse())
                .collect::<Result<_, _>>()
                .ok()?;
            Some(Tally {
                values: values.try_into().ok()?,
                updated: updated.parse().ok()?,
            })
        };
        match parts {
            ["U", reading, surface, rest @ ..] => {
                if let Some(t) = tally(rest) {
                    self.unit_mut(reading, surface).tally = t;
                }
            }
            ["C", reading, surface, context, rest @ ..] => {
                if let Some(t) = tally(rest) {
                    let unit = self.unit_mut(reading, surface);
                    if unit.contexts.len() < MAX_CONTEXTS {
                        unit.contexts.push((context.to_string(), t));
                    }
                }
            }
            ["B", reading, position, updated, value] => {
                if let (Ok(position), Ok(updated), Ok(value)) =
                    (position.parse(), updated.parse(), value.parse())
                {
                    self.boundaries
                        .entry(reading.to_string())
                        .or_default()
                        .push(Boundary {
                            position,
                            value,
                            updated,
                        });
                }
            }
            _ => {}
        }
    }

    /// The unit for `(reading, surface)`, created empty if new.
    fn unit_mut(&mut self, reading: &str, surface: &str) -> &mut Unit {
        let units = self.entries.entry(reading.to_string()).or_default();
        let at = match units.iter().position(|u| u.surface == surface) {
            Some(at) => at,
            None => {
                units.push(Unit {
                    surface: surface.to_string(),
                    tally: Tally::default(),
                    contexts: Vec::new(),
                });
                units.len() - 1
            }
        };
        &mut units[at]
    }

    /// Save the cache to a TSV file, evicting the units with the least
    /// evidence if over capacity.
    ///
    /// After the [`HEADER`] line, the first column says what a row is:
    ///
    /// - `U reading surface updated chosen confirmed accepted displaced`
    /// - `C reading surface context updated chosen confirmed accepted displaced`
    ///   (a unit's evidence in one context)
    /// - `B reading position updated value` (a segment edge the user moved)
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        self.evict();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        writeln!(writer, "{HEADER}")?;
        let values = |t: &Tally| {
            let v = t.values.map(|v| format!("{v:.4}"));
            format!("{}\t{}", t.updated, v.join("\t"))
        };

        // Sorted for deterministic output
        let mut readings: Vec<&String> = self.entries.keys().collect();
        readings.sort();
        for reading in readings {
            for unit in &self.entries[reading] {
                writeln!(
                    writer,
                    "U\t{reading}\t{}\t{}",
                    unit.surface,
                    values(&unit.tally)
                )?;
                for (context, tally) in &unit.contexts {
                    writeln!(
                        writer,
                        "C\t{reading}\t{}\t{context}\t{}",
                        unit.surface,
                        values(tally)
                    )?;
                }
            }
        }
        let mut readings: Vec<&String> = self.boundaries.keys().collect();
        readings.sort();
        for reading in readings {
            for b in &self.boundaries[reading] {
                writeln!(
                    writer,
                    "B\t{reading}\t{}\t{}\t{:.4}",
                    b.position, b.updated, b.value
                )?;
            }
        }

        writer.flush()?;
        self.dirty = false;
        Ok(())
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Total number of (reading, surface) units across all readings.
    pub fn entry_count(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    /// Drop the units with the least evidence until at most `max_entries`
    /// are left, and the weakest segment edges likewise. By how much
    /// evidence a unit holds rather than by its score: a unit the user
    /// keeps replacing scores low, and is worth keeping for that.
    fn evict(&mut self) {
        let now = now_unix();
        let total = self.entry_count();
        if total > self.max_entries {
            let mut all: Vec<(f64, &str, &str)> = self
                .entries
                .iter()
                .flat_map(|(r, units)| {
                    units
                        .iter()
                        .map(move |u| (u.tally.mass(now), r.as_str(), u.surface.as_str()))
                })
                .collect();
            all.sort_by(|a, b| a.0.total_cmp(&b.0));
            let doomed: HashSet<(String, String)> = all
                .iter()
                .take(total - self.max_entries)
                .map(|&(_, r, s)| (r.to_string(), s.to_string()))
                .collect();
            self.entries.retain(|r, units| {
                units.retain(|u| !doomed.contains(&(r.clone(), u.surface.clone())));
                !units.is_empty()
            });
        }

        let total: usize = self.boundaries.values().map(Vec::len).sum();
        if total > self.max_entries {
            let mut values: Vec<f64> = self
                .boundaries
                .values()
                .flatten()
                .map(|b| b.value * decay(b.updated, now))
                .collect();
            values.sort_by(f64::total_cmp);
            let floor = values[total - self.max_entries];
            self.boundaries.retain(|_, bs| {
                bs.retain(|b| b.value * decay(b.updated, now) >= floor);
                !bs.is_empty()
            });
        }
    }

    /// The unit `(reading, surface)` as it stands, whole — its tally and
    /// every context — for [`restore`](Self::restore) to put back after a
    /// `record` or `reinforce` changed it.
    pub fn snapshot(&self, reading: &str, surface: &str) -> UnitSnapshot {
        UnitSnapshot(
            self.entries
                .get(reading)
                .and_then(|units| units.iter().find(|u| u.surface == surface))
                .cloned(),
        )
    }

    /// Put `(reading, surface)` back to `snapshot`, taken by
    /// [`snapshot`](Self::snapshot) before a change: the unit as it was, or
    /// no unit at all when the change created it. Only that unit is
    /// touched, which is all one `record` or `reinforce` changes.
    pub fn restore(&mut self, reading: &str, surface: &str, snapshot: UnitSnapshot) {
        let units = self.entries.entry(reading.to_string()).or_default();
        let index = units.iter().position(|u| u.surface == surface);
        match (index, snapshot.0) {
            (Some(i), Some(unit)) => units[i] = unit,
            (Some(i), None) => {
                units.remove(i);
            }
            (None, Some(unit)) => units.push(unit),
            (None, None) => {}
        }
        if units.is_empty() {
            self.entries.remove(reading);
        }
        self.dirty = true;
    }
}

/// A unit as it stood, or its absence: what [`LearningCache::restore`]
/// puts back. Opaque, since a unit's insides are the cache's own.
#[derive(Debug, Clone)]
pub struct UnitSnapshot(Option<Unit>);

/// Current time as Unix timestamp in seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Config/cache with a custom entry limit and the default surface cap.
    fn config_with(max_entries: usize) -> LearningConfig {
        LearningConfig {
            max_entries,
            ..LearningConfig::default()
        }
    }

    fn cache_with(max_entries: usize) -> LearningCache {
        LearningCache::new(config_with(max_entries))
    }

    fn chose(cache: &mut LearningCache, reading: &str, surface: &str) {
        cache.record(reading, surface, "", Evidence::Chosen, 1.0);
    }

    const DAY: u64 = 86400;

    #[test]
    fn evidence_halves_every_half_life() {
        let mut tally = Tally::default();
        tally.add(Evidence::Chosen, 1.0, 1_000_000);
        let later = 1_000_000 + 30 * DAY;
        assert!((tally.at(later)[0] - 0.5).abs() < 1e-9);
        assert!((tally.at(later + 30 * DAY)[0] - 0.25).abs() < 1e-9);
        // Adding decays what was there first.
        tally.add(Evidence::Chosen, 1.0, later);
        assert!((tally.at(later)[0] - 1.5).abs() < 1e-9);
        // A clock that went backwards neither grows nor rewinds the value.
        assert!((tally.at(later - DAY)[0] - 1.5).abs() < 1e-9);
    }

    #[test]
    fn the_score_weighs_each_kind() {
        let mut tally = Tally::default();
        let now = 1_000_000;
        for (evidence, weight) in Evidence::ALL.into_iter().zip(Evidence::WEIGHTS) {
            let mut one = Tally::default();
            one.add(evidence, 1.0, now);
            assert!((one.score(now) - weight).abs() < 1e-9);
            tally.add(evidence, 1.0, now);
        }
        assert!((tally.score(now) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn displaced_lowers_a_surface_below_another() {
        let mut cache = cache_with(100);
        cache.record("きのう", "機能", "", Evidence::Accepted, 1.0);
        cache.record("きのう", "機能", "", Evidence::Displaced, 1.0);
        chose(&mut cache, "きのう", "昨日");
        let results = cache.lookup("きのう", "");
        assert_eq!(results[0].0, "昨日");
        assert!(results[1].1 < 0.0);
        assert_eq!(cache.score("きのう", "なし", ""), 0.0);
    }

    #[test]
    fn a_context_adds_its_own_evidence() {
        let mut cache = cache_with(100);
        cache.record("かいとう", "解答", "試験の", Evidence::Chosen, 1.0);
        cache.record("かいとう", "回答", "質問へ", Evidence::Chosen, 1.0);
        // The key is the tail of the preceding text.
        assert_eq!(cache.lookup("かいとう", "試験の")[0].0, "解答");
        assert_eq!(cache.lookup("かいとう", "の")[0].1, 1.0);
        assert_eq!(cache.lookup("かいとう", "質問へ")[0].0, "回答");
        assert!((cache.score("かいとう", "回答", "問へ") - 2.0).abs() < 1e-6);
        assert!((cache.score("かいとう", "回答", "") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_unit_keeps_its_strongest_contexts() {
        let mut cache = cache_with(100);
        for (context, times) in [("あa", 3), ("いi", 2), ("うu", 2), ("えe", 2)] {
            for _ in 0..times {
                cache.record("よみ", "読み", context, Evidence::Chosen, 1.0);
            }
        }
        cache.record("よみ", "読み", "おo", Evidence::Chosen, 1.0);
        let unit = &cache.entries["よみ"][0];
        assert_eq!(unit.contexts.len(), MAX_CONTEXTS);
        assert!(unit.contexts.iter().any(|(c, _)| c == "あa"));
        assert!(unit.contexts.iter().any(|(c, _)| c == "おo"));
    }

    #[test]
    fn surface_scores_sum_over_readings() {
        let mut cache = LearningCache::new(LearningConfig::default());
        chose(&mut cache, "こう", "高");
        chose(&mut cache, "こう", "高");
        chose(&mut cache, "たか", "高");
        chose(&mut cache, "かさ", "嵩");
        let scores = cache.surface_scores(["高", "嵩", "鷹"]);
        assert!(scores["高"] > scores["嵩"]);
        assert!(!scores.contains_key("鷹"));
    }

    #[test]
    fn test_prefix_lookup() {
        let mut cache = cache_with(100);
        chose(&mut cache, "きょう", "今日");
        chose(&mut cache, "きょうと", "京都");
        chose(&mut cache, "あした", "明日");

        let results = cache.prefix_lookup("きょう", "");
        let readings: Vec<&str> = results.iter().map(|(r, _, _)| r.as_str()).collect();
        assert_eq!(readings.len(), 2);
        assert!(readings.contains(&"きょう"));
        assert!(readings.contains(&"きょうと"));
        assert!(cache.prefix_lookup("い", "").is_empty());
    }

    #[test]
    fn saves_and_loads_every_row_kind() {
        let mut cache = cache_with(100);
        chose(&mut cache, "きょう", "今日");
        cache.record("きょう", "京", "東の", Evidence::Displaced, 0.5);
        cache.record("あした", "明日", "", Evidence::Accepted, 1.0);
        cache.record_boundary("きょうは", 3);

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();
        assert!(!cache.is_dirty());
        let content = std::fs::read_to_string(file.path()).unwrap();
        assert!(content.starts_with(HEADER));
        assert!(content.contains("U\tきょう\t今日\t"));
        assert!(content.contains("C\tきょう\t京\t東の\t"));
        assert!(content.contains("B\tきょうは\t3\t"));

        let loaded = LearningCache::load(file.path(), config_with(100)).unwrap();
        assert!(!loaded.is_dirty());
        assert_eq!(loaded.entry_count(), 3);
        for (reading, surface, context) in [
            ("きょう", "今日", ""),
            ("きょう", "京", "東の"),
            ("あした", "明日", ""),
        ] {
            let (a, b) = (
                cache.score(reading, surface, context),
                loaded.score(reading, surface, context),
            );
            assert!((a - b).abs() < 1e-3, "{surface}: {a} vs {b}");
        }
        assert_eq!(loaded.preferred_boundary("きょうは"), Some(3));
    }

    #[test]
    fn the_four_column_format_loads_as_confirmed() {
        let file = NamedTempFile::new().unwrap();
        let now = now_unix();
        std::fs::write(
            file.path(),
            format!(
                "# karukan learning cache v1\n\nきょう\t今日\t5\t{now}\nmalformed_line\n\
                 きょう\t京\tbad\t{now}\nあした\t明日\t1\t{}\n",
                now - 30 * DAY
            ),
        )
        .unwrap();

        let cache = LearningCache::load(file.path(), config_with(100)).unwrap();
        assert_eq!(cache.entry_count(), 2);
        // 5 confirmations, weighed 0.4 each, recorded just now.
        assert!((cache.score("きょう", "今日", "") - 2.0).abs() < 1e-3);
        // One a half-life ago counts half.
        assert!((cache.score("あした", "明日", "") - 0.2).abs() < 1e-3);
    }

    #[test]
    fn test_dirty_flag() {
        let mut cache = cache_with(100);
        assert!(!cache.is_dirty());

        chose(&mut cache, "きょう", "今日");
        assert!(cache.is_dirty());

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();
        assert!(!cache.is_dirty());
    }

    #[test]
    fn eviction_drops_the_least_evidence_first() {
        let mut cache = cache_with(3);
        for r in ["a", "b", "c", "d", "e"] {
            cache.record(r, r, "", Evidence::Accepted, 1.0);
        }
        // Replaced often: a low score, and much evidence.
        cache.record("a", "a", "", Evidence::Displaced, 3.0);
        chose(&mut cache, "c", "c");

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_reading("a"));
        assert!(cache.contains_reading("c"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = LearningCache::load(Path::new("/nonexistent/path"), config_with(100));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_suggestion_clears_exact_and_prefix_twins() {
        let mut cache = cache_with(100);
        chose(&mut cache, "あい", "藍");
        chose(&mut cache, "あいさ", "藍"); // same surface, longer reading (a twin)
        chose(&mut cache, "あい", "愛"); // different surface under the same reading
        chose(&mut cache, "うみ", "藍"); // same surface, unrelated reading

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();
        assert!(!cache.is_dirty());

        assert!(cache.remove_suggestion("あい", "藍"));
        assert!(cache.is_dirty(), "removal must mark the cache dirty");

        assert!(cache.lookup("あい", "").iter().all(|(s, _)| s != "藍"));
        assert!(!cache.contains_reading("あいさ"));
        assert!(cache.lookup("あい", "").iter().any(|(s, _)| s == "愛"));
        assert!(cache.lookup("うみ", "").iter().any(|(s, _)| s == "藍"));
    }

    #[test]
    fn test_remove_suggestion_nonexistent_is_noop() {
        let mut cache = cache_with(100);
        chose(&mut cache, "あい", "藍");

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();

        assert!(!cache.remove_suggestion("あい", "愛"));
        assert!(!cache.remove_suggestion("かき", "柿"));
        assert!(!cache.is_dirty(), "no-op removal must not mark dirty");
    }

    #[test]
    fn test_record_skips_long_surface() {
        let mut cache = LearningCache::new(LearningConfig {
            max_entries: 100,
            max_surface_chars: 5,
        });

        chose(&mut cache, "あ", &"漢".repeat(6));
        assert_eq!(cache.entry_count(), 0);
        assert!(!cache.is_dirty());

        // Boundary: exactly max_surface_chars is accepted.
        chose(&mut cache, "あ", &"漢".repeat(5));
        assert_eq!(cache.entry_count(), 1);

        // Only the surface is capped, not the reading.
        chose(&mut cache, &"あ".repeat(30), "短い");
        assert_eq!(cache.entry_count(), 2);
    }

    #[test]
    fn a_replacement_needs_the_context_or_two_of_them() {
        let mut cache = cache_with(100);
        cache.record("はし", "箸", "ごはんを", Evidence::Chosen, 1.0);
        // Chosen in this context.
        assert_eq!(
            cache.replacement("はし", "橋", "ごはんを", 0.3).as_deref(),
            Some("箸")
        );
        // Another context only orders lists.
        assert_eq!(cache.replacement("はし", "橋", "かかる", 0.3), None);
        assert!(cache.score("はし", "箸", "かかる") > 0.0);

        // Chosen in two contexts: a preference that holds anywhere.
        cache.record("こども", "子ども", "うちの", Evidence::Chosen, 1.0);
        assert_eq!(cache.replacement("こども", "子供", "あの", 0.3), None);
        cache.record("こども", "子ども", "", Evidence::Chosen, 1.0);
        assert_eq!(
            cache.replacement("こども", "子供", "あの", 0.3).as_deref(),
            Some("子ども")
        );
        // Never the surface itself.
        assert_eq!(cache.replacement("こども", "子ども", "あの", 0.3), None);
    }

    #[test]
    fn opposite_picks_cancel_out() {
        let mut cache = cache_with(100);
        cache.record("はし", "箸", "を", Evidence::Chosen, 1.0);
        cache.record("はし", "橋", "を", Evidence::Displaced, 1.0);
        assert_eq!(
            cache.replacement("はし", "橋", "を", 0.3).as_deref(),
            Some("箸")
        );
        cache.record("はし", "橋", "を", Evidence::Chosen, 1.0);
        cache.record("はし", "箸", "を", Evidence::Displaced, 1.0);
        // Even: the model's own text stands, either way round.
        assert_eq!(cache.replacement("はし", "橋", "を", 0.3), None);
        assert_eq!(cache.replacement("はし", "箸", "を", 0.3), None);
    }

    #[test]
    fn reinforcing_keeps_units_fresh_and_makes_none() {
        let mut cache = cache_with(100);
        cache.reinforce("きょう", "今日", "");
        assert_eq!(cache.entry_count(), 0);
        cache.record("きょう", "今日", "", Evidence::Confirmed, 1.0);
        cache.reinforce("きょう", "今日", "");
        assert!((cache.score("きょう", "今日", "") - 1.0).abs() < 1e-6);
        assert!(cache.picked("きょう", "今日"));
        cache.record("あした", "明日", "", Evidence::Accepted, 1.0);
        assert!(!cache.picked("あした", "明日"));
    }

    #[test]
    fn a_context_with_a_tab_is_none() {
        let mut cache = cache_with(100);
        cache.record("よみ", "読み", "a\t", Evidence::Chosen, 1.0);
        assert!(cache.entries["よみ"][0].contexts.is_empty());
    }
}
