//! `vietnamese.cm.dict` loader (UTF-8, one word per line).

use std::collections::HashSet;
use std::io::{BufRead, Read};
use unicode_normalization::UnicodeNormalization;

/// In-memory Vietnamese word list for commit-time spell checks.
#[derive(Debug, Clone)]
pub struct VietnameseDict {
    words: HashSet<String>,
}

impl VietnameseDict {
    pub fn load_from_reader<R: Read>(r: R) -> std::io::Result<Self> {
        let mut words = HashSet::new();
        for line in std::io::BufReader::new(r).lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            words.insert(Self::normalize_key(line));
        }
        Ok(Self { words })
    }

    pub fn load_from_path(path: &std::path::Path) -> std::io::Result<Self> {
        let f = std::fs::File::open(path)?;
        Self::load_from_reader(f)
    }

    /// Lowercase + NFC for stable lookup.
    #[inline]
    pub fn normalize_key(word: &str) -> String {
        word.to_lowercase().chars().nfc().collect()
    }

    pub fn contains_normalized(&self, key: &str) -> bool {
        self.words.contains(key)
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}
