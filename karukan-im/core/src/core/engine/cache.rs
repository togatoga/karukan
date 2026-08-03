//! LRU cache for model conversion results.
//!
//! Keyed by everything that determines a conversion's output: the katakana
//! reading, the left context (lctx), and the [`ConversionStrategy`] (which
//! carries the beam width). Live conversion re-runs every chunk on each
//! keystroke and only cache misses reach the model, so unchanged chunks —
//! and re-typed or backspaced-over text — come back instantly.

use std::collections::HashMap;

use super::ConversionStrategy;

/// Everything that determines a model conversion's output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::core) struct ConversionCacheKey {
    /// Katakana reading fed to the model.
    pub katakana: String,
    /// Left context (lctx) fed to the model.
    pub lctx: String,
    /// Model dispatch and beam width used for the conversion.
    pub strategy: ConversionStrategy,
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

    fn key(katakana: &str, lctx: &str, strategy: ConversionStrategy) -> ConversionCacheKey {
        ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: lctx.to_string(),
            strategy,
        }
    }

    #[test]
    fn hit_and_miss() {
        let mut cache = ConversionCache::new(4);
        let k = key("キョウ", "", ConversionStrategy::MainModelOnly);
        assert_eq!(cache.get(&k), None);
        cache.insert(k.clone(), vec!["今日".to_string()]);
        assert_eq!(cache.get(&k), Some(vec!["今日".to_string()]));
    }

    #[test]
    fn distinct_lctx_and_strategy_are_distinct_keys() {
        let mut cache = ConversionCache::new(4);
        let k1 = key("キョウ", "", ConversionStrategy::MainModelOnly);
        let k2 = key("キョウ", "昨日と", ConversionStrategy::MainModelOnly);
        let k3 = key(
            "キョウ",
            "",
            ConversionStrategy::MainModelBeam { beam_width: 3 },
        );
        cache.insert(k1.clone(), vec!["今日".to_string()]);
        assert_eq!(cache.get(&k2), None);
        assert_eq!(cache.get(&k3), None);
        assert_eq!(cache.get(&k1), Some(vec!["今日".to_string()]));
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = ConversionCache::new(2);
        let k1 = key("ア", "", ConversionStrategy::MainModelOnly);
        let k2 = key("イ", "", ConversionStrategy::MainModelOnly);
        let k3 = key("ウ", "", ConversionStrategy::MainModelOnly);
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
        let k1 = key("ア", "", ConversionStrategy::MainModelOnly);
        let k2 = key("イ", "", ConversionStrategy::MainModelOnly);
        cache.insert(k1.clone(), vec!["亜".to_string()]);
        cache.insert(k2.clone(), vec!["伊".to_string()]);
        cache.insert(k1.clone(), vec!["阿".to_string()]);
        assert_eq!(cache.get(&k1), Some(vec!["阿".to_string()]));
        assert!(cache.get(&k2).is_some());
    }
}
