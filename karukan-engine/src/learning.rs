//! Learning cache: what the user did with conversions, as decaying evidence.
//!
//! The unit is a stretch of reading with a surface. Each unit keeps one
//! value per kind of [`Evidence`], decaying with a half-life of
//! [`HALF_LIFE_SECS`] (`v ← v·2^(−Δt/H) + w`, so only the value and the
//! time it was last updated are stored), plus the same tally split by up to
//! [`MAX_CONTEXTS`] contexts: up to [`CONTEXT_CHARS`] chars of what was typed
//! before the unit and after it. A unit's score is [`Evidence::WEIGHTS`]
//! applied to its tally, plus its tally in the context that matches the one
//! being looked up best. A context a surface was chosen in is a rule
//! ([`LearningCache::rules`]): what was typed around it says where it
//! applies again.
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

/// Chars kept on each side of a context.
pub const CONTEXT_CHARS: usize = 4;

/// Contexts kept per unit; the weakest goes when another arrives.
pub const MAX_CONTEXTS: usize = 8;

/// First line of a file in the current format.
const HEADER: &str = "# karukan learning cache v3";

/// First line of a file in the format before, whose contexts are the text
/// before a unit only.
const HEADER_V2: &str = "# karukan learning cache v2";

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

/// Where a unit was recorded: up to [`CONTEXT_CHARS`] chars of what was
/// typed before it and after it.
#[derive(Debug, Clone)]
struct Context {
    left: String,
    right: String,
    tally: Tally,
}

/// How many chars of `left`/`right`, what surrounds a place, agree with a
/// context's `before`/`after`: before from its end, after from its start.
pub fn context_match(before: &str, after: &str, left: &str, right: &str) -> (usize, usize) {
    let l = before
        .chars()
        .rev()
        .zip(left.chars().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let r = after
        .chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count();
    (l, r)
}

/// A surface learned for a reading.
#[derive(Debug, Clone)]
struct Unit {
    surface: String,
    tally: Tally,
    /// The same evidence, split by the context it was recorded in.
    contexts: Vec<Context>,
}

impl Unit {
    /// The context that agrees with `left`/`right` on the most chars, at
    /// least one.
    fn closest(&self, left: &str, right: &str) -> Option<&Context> {
        self.contexts
            .iter()
            .map(|c| {
                let (l, r) = context_match(&c.left, &c.right, left, right);
                (l + r, c)
            })
            .filter(|(n, _)| *n > 0)
            .max_by_key(|(n, _)| *n)
            .map(|(_, c)| c)
    }

    fn score(&self, left: &str, right: &str, now: u64) -> f64 {
        let in_context = self
            .closest(left, right)
            .map_or(0.0, |c| c.tally.score(now));
        self.tally.score(now) + in_context
    }
}

/// A context a surface was chosen in: where it may stand in for what the
/// model writes ([`LearningCache::rules`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub surface: String,
    /// What was typed before and after it.
    pub left: String,
    pub right: String,
    /// The score of what was recorded in this context.
    pub score: f64,
}

/// A unit as it stood, for putting it back ([`LearningCache::snapshot`]):
/// every kind of evidence and every context, or no unit at all.
#[derive(Debug, Clone)]
pub struct LearningSnapshot(Option<Unit>);

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

/// The context keys for what was typed around a unit: the end of `left`
/// and the start of `right`, or none when they hold a tab or a newline the
/// TSV cannot carry.
fn context_keys(left: &str, right: &str) -> Option<(String, String)> {
    let count = left.chars().count();
    let left: String = left
        .chars()
        .skip(count.saturating_sub(CONTEXT_CHARS))
        .collect();
    let right: String = right.chars().take(CONTEXT_CHARS).collect();
    (!left.chars().chain(right.chars()).any(char::is_control)).then_some((left, right))
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

    /// Record `weight` of `evidence` for `surface` under `reading`, typed
    /// between `left` and `right` (empty for none; only the chars next to
    /// the unit are kept).
    ///
    /// Surfaces longer than `max_surface_chars` are skipped; see
    /// [`LearningConfig::max_surface_chars`] for why.
    pub fn record(
        &mut self,
        reading: &str,
        surface: &str,
        left: &str,
        right: &str,
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
        if let Some((left, right)) = context_keys(left, right) {
            let at = match unit
                .contexts
                .iter()
                .position(|c| c.left == left && c.right == right)
            {
                Some(at) => at,
                None => {
                    if unit.contexts.len() >= MAX_CONTEXTS {
                        let weakest = (0..unit.contexts.len())
                            .min_by(|&a, &b| {
                                let mass = |i: usize| unit.contexts[i].tally.mass(now);
                                mass(a).total_cmp(&mass(b))
                            })
                            .expect("full");
                        unit.contexts.remove(weakest);
                    }
                    unit.contexts.push(Context {
                        left,
                        right,
                        tally: Tally::default(),
                    });
                    unit.contexts.len() - 1
                }
            };
            unit.contexts[at].tally.add(evidence, weight, now);
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
    pub fn reinforce(&mut self, reading: &str, surface: &str, left: &str, right: &str) {
        let exists = self
            .entries
            .get(reading)
            .is_some_and(|units| units.iter().any(|u| u.surface == surface));
        if exists {
            self.record(reading, surface, left, right, Evidence::Accepted, 1.0);
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

    /// The contexts `reading` was chosen in, each with its surface and the
    /// score of what was recorded there: where a surface may stand in for
    /// what the model writes, when what is typed around agrees.
    pub fn rules(&self, reading: &str) -> Vec<Rule> {
        let now = now_unix();
        let Some(units) = self.entries.get(reading) else {
            return Vec::new();
        };
        units
            .iter()
            .flat_map(|u| {
                u.contexts
                    .iter()
                    .filter(move |c| c.tally.at(now)[Evidence::Chosen.index()] > 0.0)
                    .map(move |c| Rule {
                        surface: u.surface.clone(),
                        left: c.left.clone(),
                        right: c.right.clone(),
                        score: c.tally.score(now),
                    })
            })
            .collect()
    }

    /// The score of `surface` for `reading` in its context closest to
    /// `left`/`right`; 0 when none agrees on a char.
    pub fn context_score(&self, reading: &str, surface: &str, left: &str, right: &str) -> f64 {
        let now = now_unix();
        self.entries
            .get(reading)
            .and_then(|units| units.iter().find(|u| u.surface == surface))
            .and_then(|u| u.closest(left, right))
            .map_or(0.0, |c| c.tally.score(now))
    }

    /// `(reading, surface)` as it stands now, for [`restore`](Self::restore)
    /// to take back what is recorded after it.
    pub fn snapshot(&self, reading: &str, surface: &str) -> LearningSnapshot {
        LearningSnapshot(
            self.entries
                .get(reading)
                .and_then(|units| units.iter().find(|u| u.surface == surface))
                .cloned(),
        )
    }

    /// Put `(reading, surface)` back as `snapshot` had it: removed when it
    /// did not exist then. Nothing else is touched.
    pub fn restore(&mut self, reading: &str, surface: &str, snapshot: LearningSnapshot) {
        let units = self.entries.entry(reading.to_string()).or_default();
        let at = units.iter().position(|u| u.surface == surface);
        match (at, snapshot.0) {
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

    /// Whether anything is learned under `reading`.
    pub fn contains_reading(&self, reading: &str) -> bool {
        self.entries.contains_key(reading)
    }

    /// Score of `surface` under `reading` typed between `left` and `right`;
    /// 0 when nothing is learned for it.
    pub fn score(&self, reading: &str, surface: &str, left: &str, right: &str) -> f64 {
        let now = now_unix();
        self.entries
            .get(reading)
            .and_then(|units| units.iter().find(|u| u.surface == surface))
            .map_or(0.0, |u| u.score(left, right, now))
    }

    /// Every surface learned under `reading` with its score between `left`
    /// and `right`, best first.
    pub fn lookup(&self, reading: &str, left: &str, right: &str) -> Vec<(String, f64)> {
        let now = now_unix();
        let Some(units) = self.entries.get(reading) else {
            return Vec::new();
        };
        let mut scored: Vec<(String, f64)> = units
            .iter()
            .map(|u| (u.surface.clone(), u.score(left, right, now)))
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
                *scores.entry(unit.surface.clone()).or_insert(0.0) += unit.score("", "", now);
            }
        }
        scores
    }

    /// `(reading, surface, score)` for every reading that starts with
    /// `prefix`, typed after `left`, best first.
    pub fn prefix_lookup(&self, prefix: &str, left: &str) -> Vec<(String, String, f64)> {
        let now = now_unix();
        let mut results: Vec<(String, String, f64)> = Vec::new();
        for (reading, units) in &self.entries {
            if reading.starts_with(prefix) {
                for unit in units {
                    results.push((
                        reading.clone(),
                        unit.surface.clone(),
                        unit.score(left, "", now),
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
        let mut version = 1;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            let line = line.trim_end_matches(['\r', '\n']);
            if i == 0 && (line == HEADER || line == HEADER_V2) {
                version = if line == HEADER { 3 } else { 2 };
                continue;
            }
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if version > 1 {
                cache.load_row(&parts, version);
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

    /// A row of the current format, or of v2, whose contexts have no right
    /// side.
    fn load_row(&mut self, parts: &[&str], version: u8) {
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
            ["C", reading, surface, left, rest @ ..] => {
                let (right, rest) = match rest {
                    [right, rest @ ..] if version >= 3 => (*right, rest),
                    _ => ("", rest),
                };
                if let Some(tally) = tally(rest) {
                    let unit = self.unit_mut(reading, surface);
                    if unit.contexts.len() < MAX_CONTEXTS {
                        unit.contexts.push(Context {
                            left: left.to_string(),
                            right: right.to_string(),
                            tally,
                        });
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
    /// - `C reading surface left right updated chosen confirmed accepted displaced`
    ///   (a unit's evidence in one context: what was typed before and after)
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
                for c in &unit.contexts {
                    writeln!(
                        writer,
                        "C\t{reading}\t{}\t{}\t{}\t{}",
                        unit.surface,
                        c.left,
                        c.right,
                        values(&c.tally)
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
}

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
        cache.record(reading, surface, "", "", Evidence::Chosen, 1.0);
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
        cache.record("きのう", "機能", "", "", Evidence::Accepted, 1.0);
        cache.record("きのう", "機能", "", "", Evidence::Displaced, 1.0);
        chose(&mut cache, "きのう", "昨日");
        let results = cache.lookup("きのう", "", "");
        assert_eq!(results[0].0, "昨日");
        assert!(results[1].1 < 0.0);
        assert_eq!(cache.score("きのう", "なし", "", ""), 0.0);
    }

    #[test]
    fn a_context_adds_its_own_evidence() {
        let mut cache = cache_with(100);
        cache.record("かいとう", "解答", "試験の", "", Evidence::Chosen, 1.0);
        cache.record("かいとう", "回答", "質問へ", "", Evidence::Chosen, 1.0);
        // The key is the tail of the preceding text.
        assert_eq!(cache.lookup("かいとう", "試験の", "")[0].0, "解答");
        // Closest by the chars that agree: one is enough to count.
        assert_eq!(cache.lookup("かいとう", "の", "")[0].1, 2.0);
        assert_eq!(cache.lookup("かいとう", "質問へ", "")[0].0, "回答");
        assert!((cache.score("かいとう", "回答", "問へ", "") - 2.0).abs() < 1e-6);
        assert!((cache.score("かいとう", "回答", "", "") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_unit_keeps_its_strongest_contexts() {
        let mut cache = cache_with(100);
        let contexts = ["あa", "いi", "うu", "えe", "かk", "きk", "くk", "けk"];
        for (n, context) in contexts.into_iter().enumerate() {
            let times = if n == 0 { 3 } else { 2 };
            for _ in 0..times {
                cache.record("よみ", "読み", context, "", Evidence::Chosen, 1.0);
            }
        }
        cache.record("よみ", "読み", "おo", "", Evidence::Chosen, 1.0);
        let unit = &cache.entries["よみ"][0];
        assert_eq!(unit.contexts.len(), MAX_CONTEXTS);
        assert!(unit.contexts.iter().any(|c| c.left == "あa"));
        assert!(unit.contexts.iter().any(|c| c.left == "おo"));
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
        cache.record("きょう", "京", "東の", "とも", Evidence::Displaced, 0.5);
        cache.record("あした", "明日", "", "", Evidence::Accepted, 1.0);
        cache.record_boundary("きょうは", 3);

        let file = NamedTempFile::new().unwrap();
        cache.save(file.path()).unwrap();
        assert!(!cache.is_dirty());
        let content = std::fs::read_to_string(file.path()).unwrap();
        assert!(content.starts_with(HEADER));
        assert!(content.contains("U\tきょう\t今日\t"));
        assert!(content.contains("C\tきょう\t京\t東の\tとも\t"));
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
                cache.score(reading, surface, context, ""),
                loaded.score(reading, surface, context, ""),
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
        assert!((cache.score("きょう", "今日", "", "") - 2.0).abs() < 1e-3);
        // One a half-life ago counts half.
        assert!((cache.score("あした", "明日", "", "") - 0.2).abs() < 1e-3);
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
            cache.record(r, r, "", "", Evidence::Accepted, 1.0);
        }
        // Replaced often: a low score, and much evidence.
        cache.record("a", "a", "", "", Evidence::Displaced, 3.0);
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

        assert!(cache.lookup("あい", "", "").iter().all(|(s, _)| s != "藍"));
        assert!(!cache.contains_reading("あいさ"));
        assert!(cache.lookup("あい", "", "").iter().any(|(s, _)| s == "愛"));
        assert!(cache.lookup("うみ", "", "").iter().any(|(s, _)| s == "藍"));
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
    fn a_rule_is_a_context_a_surface_was_chosen_in() {
        let mut cache = cache_with(100);
        cache.record("ぶ", "分", "しあいは", "がわるい", Evidence::Chosen, 1.0);
        cache.record("ぶ", "部", "しあいは", "がわるい", Evidence::Displaced, 1.0);
        cache.record("ぶ", "部", "", "かつ", Evidence::Confirmed, 1.0);
        let rules = cache.rules("ぶ");
        assert_eq!(
            rules,
            [Rule {
                surface: "分".into(),
                left: "しあいは".into(),
                right: "がわるい".into(),
                score: 1.0,
            }]
        );
        // The other side, in the closest context: by what follows here.
        assert_eq!(cache.context_score("ぶ", "部", "このは", "がわる"), -1.0);
        assert_eq!(cache.context_score("ぶ", "部", "", "かつて"), 0.4);
        assert_eq!(cache.context_score("ぶ", "部", "あ", "い"), 0.0);
    }

    #[test]
    fn contexts_match_from_the_unit_outwards() {
        assert_eq!(
            context_match("しあいは", "がわるい", "このしょうぶは", "がわ"),
            (1, 2)
        );
        assert_eq!(context_match("ごはんを", "", "ごはんを", "で"), (4, 0));
        assert_eq!(context_match("", "", "なにか", "ある"), (0, 0));
    }

    #[test]
    fn opposite_picks_cancel_out() {
        let mut cache = cache_with(100);
        cache.record("はし", "箸", "ごはんを", "", Evidence::Chosen, 1.0);
        cache.record("はし", "橋", "ごはんを", "", Evidence::Displaced, 1.0);
        assert_eq!(cache.rules("はし")[0].score, 1.0);
        cache.record("はし", "橋", "ごはんを", "", Evidence::Chosen, 1.0);
        cache.record("はし", "箸", "ごはんを", "", Evidence::Displaced, 1.0);
        // Even: neither outscores the other there.
        let rules = cache.rules("はし");
        assert!(rules.iter().all(|r| r.score.abs() < 1e-9));
    }

    #[test]
    fn a_v2_file_loads_its_contexts_as_the_left_side() {
        let file = NamedTempFile::new().unwrap();
        let now = now_unix();
        std::fs::write(
            file.path(),
            format!(
                "{HEADER_V2}\nU\tはし\t箸\t{now}\t1\t0\t0\t0\n\
                 C\tはし\t箸\tんを\t{now}\t1\t0\t0\t0\n"
            ),
        )
        .unwrap();
        let cache = LearningCache::load(file.path(), config_with(100)).unwrap();
        let rules = cache.rules("はし");
        assert_eq!(rules.len(), 1);
        assert_eq!(
            (rules[0].left.as_str(), rules[0].right.as_str()),
            ("んを", "")
        );
    }

    #[test]
    fn reinforcing_keeps_units_fresh_and_makes_none() {
        let mut cache = cache_with(100);
        cache.reinforce("きょう", "今日", "", "");
        assert_eq!(cache.entry_count(), 0);
        cache.record("きょう", "今日", "", "", Evidence::Confirmed, 1.0);
        cache.reinforce("きょう", "今日", "", "");
        assert!((cache.score("きょう", "今日", "", "") - 0.5).abs() < 1e-6);
        assert!(cache.picked("きょう", "今日"));
        cache.record("あした", "明日", "", "", Evidence::Accepted, 1.0);
        assert!(!cache.picked("あした", "明日"));
    }

    #[test]
    fn a_snapshot_puts_back_every_kind_and_context() {
        let mut cache = cache_with(100);
        cache.record("はし", "箸", "ごはんを", "", Evidence::Chosen, 1.0);
        let before = cache.snapshot("はし", "箸");
        let missing = cache.snapshot("はし", "橋");
        cache.record("はし", "箸", "かかる", "", Evidence::Displaced, 1.0);
        cache.record("はし", "橋", "かかる", "", Evidence::Chosen, 1.0);
        cache.restore("はし", "橋", missing);
        cache.restore("はし", "箸", before);
        assert!((cache.score("はし", "箸", "ごはんを", "") - 2.0).abs() < 1e-6);
        assert!((cache.score("はし", "箸", "かかる", "") - 1.0).abs() < 1e-6);
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn a_context_with_a_tab_is_none() {
        let mut cache = cache_with(100);
        cache.record("よみ", "読み", "a\t", "", Evidence::Chosen, 1.0);
        assert!(cache.entries["よみ"][0].contexts.is_empty());
    }
}
