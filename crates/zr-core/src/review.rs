use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewVerdict {
    Keep,
    Drop,
    NeedsAttention,
    Unreviewed,
}

impl Default for ReviewVerdict {
    fn default() -> Self {
        Self::Unreviewed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewEntry {
    pub verdict: ReviewVerdict,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewStore {
    pub entries: BTreeMap<String, BTreeMap<usize, ReviewEntry>>,
}

impl ReviewStore {
    pub fn verdict_for(&self, game_id: &str, sequence_index: usize) -> ReviewVerdict {
        self.entries
            .get(game_id)
            .and_then(|game| game.get(&sequence_index))
            .map(|entry| entry.verdict)
            .unwrap_or(ReviewVerdict::Unreviewed)
    }

    pub fn note_for(&self, game_id: &str, sequence_index: usize) -> String {
        self.entries
            .get(game_id)
            .and_then(|game| game.get(&sequence_index))
            .map(|entry| entry.note.clone())
            .unwrap_or_default()
    }

    pub fn set(
        &mut self,
        game_id: &str,
        sequence_index: usize,
        verdict: ReviewVerdict,
        note: String,
    ) {
        self.entries
            .entry(game_id.to_string())
            .or_default()
            .insert(sequence_index, ReviewEntry { verdict, note });
    }
}

pub fn load_review_store(path: &Path) -> Result<ReviewStore> {
    if !path.exists() {
        return Ok(ReviewStore::default());
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read review file {}", path.display()))?;
    let store = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse review file {}", path.display()))?;
    Ok(store)
}

pub fn save_review_store(path: &Path, store: &ReviewStore) -> Result<()> {
    let content = serde_json::to_string_pretty(store)?;
    fs::write(path, content)
        .with_context(|| format!("failed to write review file {}", path.display()))?;
    Ok(())
}
