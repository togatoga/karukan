//! LRU cache for model conversion results.
//!
//! Keyed by the computation itself — reading, left context, which model, and
//! beam width — not by the strategy that asked for it. Strategies that share
//! a computation therefore share its entry: `ParallelBeam`'s greedy half is
//! the same thing `MainModelOnly` computes, and its beam half is what
//! `LightModelBeam` computes, so switching strategies mid-word (the adaptive
//! latency downgrade) reuses whatever already ran.

use std::collections::HashMap;

/// Which model a conversion ran on. In `Light` strategy mode the light model
/// occupies the main slot, so this names the slot, not the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::core) enum ModelRole {
    Main,
    Light,
}

/// Everything that determines a model conversion's output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::core) struct ConversionCacheKey {
    /// Katakana reading fed to the model.
    pub katakana: String,
    /// Left context (lctx) fed to the model.
    pub lctx: String,
    /// Which model ran the conversion.
    pub model: ModelRole,
    /// Requested candidate count (1 = greedy).
    pub beam_width: usize,
}

struct Entry {
    candidates: Vec<String>,
    last_used: u64,
}

/// Bounded LRU map from [`ConversionCacheKey`] to conversion candidates.
pub(in crate::core) struct ConversionCache {
    entries: HashMap<ConversionCacheKey, Entry>,
    capacity: usize,
    clock: u64,
}

impl ConversionCache {
    /// Roughly one entry is inserted per keystroke (the growing tail chunk),
    /// so this covers a long editing session. Entries are a few hundred bytes
    /// (reading + lctx + candidates), so the cache stays in the low MBs.
    const DEFAULT_CAPACITY: usize = 4096;

    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            clock: 0,
        }
    }

    /// Cached candidates for `key`, refreshing its recency on hit.
    pub fn get(&mut self, key: &ConversionCacheKey) -> Option<Vec<String>> {
        self.clock += 1;
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.clock;
        Some(entry.candidates.clone())
    }

    /// Insert a result, evicting the least recently used entry when full.
    pub fn insert(&mut self, key: ConversionCacheKey, candidates: Vec<String>) {
        self.clock += 1;
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            // A linear scan over a few thousand entries is microseconds —
            // negligible next to a model call.
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            Entry {
                candidates,
                last_used: self.clock,
            },
        );
    }
}

impl Default for ConversionCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(katakana: &str, lctx: &str, model: ModelRole, beam_width: usize) -> ConversionCacheKey {
        ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: lctx.to_string(),
            model,
            beam_width,
        }
    }

    fn greedy(katakana: &str, lctx: &str) -> ConversionCacheKey {
        key(katakana, lctx, ModelRole::Main, 1)
    }

    #[test]
    fn hit_and_miss() {
        let mut cache = ConversionCache::new(4);
        let k = greedy("キョウ", "");
        assert_eq!(cache.get(&k), None);
        cache.insert(k.clone(), vec!["今日".to_string()]);
        assert_eq!(cache.get(&k), Some(vec!["今日".to_string()]));
    }

    #[test]
    fn every_key_field_distinguishes_entries() {
        let mut cache = ConversionCache::new(4);
        let k1 = greedy("キョウ", "");
        cache.insert(k1.clone(), vec!["今日".to_string()]);
        for other in [
            greedy("キョウ", "昨日と"),
            key("キョウ", "", ModelRole::Main, 3),
            key("キョウ", "", ModelRole::Light, 1),
        ] {
            assert_eq!(cache.get(&other), None);
        }
        assert_eq!(cache.get(&k1), Some(vec!["今日".to_string()]));
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = ConversionCache::new(2);
        let k1 = greedy("ア", "");
        let k2 = greedy("イ", "");
        let k3 = greedy("ウ", "");
        cache.insert(k1.clone(), vec!["亜".to_string()]);
        cache.insert(k2.clone(), vec!["伊".to_string()]);
        // Touch k1 so k2 becomes the eviction target.
        cache.get(&k1);
        cache.insert(k3.clone(), vec!["宇".to_string()]);
        assert_eq!(cache.get(&k2), None);
        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k3).is_some());
    }

    #[test]
    fn reinserting_existing_key_does_not_evict() {
        let mut cache = ConversionCache::new(2);
        let k1 = greedy("ア", "");
        let k2 = greedy("イ", "");
        cache.insert(k1.clone(), vec!["亜".to_string()]);
        cache.insert(k2.clone(), vec!["伊".to_string()]);
        cache.insert(k1.clone(), vec!["阿".to_string()]);
        assert_eq!(cache.get(&k1), Some(vec!["阿".to_string()]));
        assert!(cache.get(&k2).is_some());
    }
}
