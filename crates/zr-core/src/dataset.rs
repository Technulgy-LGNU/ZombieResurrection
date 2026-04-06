use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::archive::{
    load_manifest, load_shard, load_split_bundle, resolve_shard_paths, resolve_split_bundle_path,
    GameShard, SplitBundle,
};
use crate::config::PipelineConfig;
use crate::pipeline::{preprocess_log, preprocess_logs_with_splits, PipelineOutput};
use crate::raw::load_raw_game;
use crate::review::ReviewStore;
use crate::types::TrainingSample;

pub type SampleIter = Box<dyn Iterator<Item = Result<TrainingSample>>>;

pub enum DatasetSource {
    Archived(ArchivedDataset),
    Live(LiveDataset),
}

impl DatasetSource {
    pub fn iter(self) -> SampleIter {
        match self {
            Self::Archived(dataset) => dataset.iter(),
            Self::Live(dataset) => dataset.iter(),
        }
    }
}

pub struct ArchivedDataset {
    shards: Vec<PathBuf>,
    pub split_bundle: SplitBundle,
}

impl ArchivedDataset {
    pub fn open(manifest_path: &Path) -> Result<Self> {
        let manifest = load_manifest(manifest_path)?;
        let shards = resolve_shard_paths(manifest_path, &manifest);
        let split_bundle = load_split_bundle(&resolve_split_bundle_path(manifest_path, &manifest))?;
        Ok(Self {
            shards,
            split_bundle,
        })
    }

    pub fn iter(self) -> SampleIter {
        Box::new(ArchivedDatasetIter {
            shard_paths: self.shards,
            shard_index: 0,
            current_samples: Vec::new().into_iter(),
        })
    }
}

struct ArchivedDatasetIter {
    shard_paths: Vec<PathBuf>,
    shard_index: usize,
    current_samples: std::vec::IntoIter<TrainingSample>,
}

impl Iterator for ArchivedDatasetIter {
    type Item = Result<TrainingSample>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(sample) = self.current_samples.next() {
                return Some(Ok(sample));
            }
            let path = self.shard_paths.get(self.shard_index)?.clone();
            self.shard_index += 1;
            match load_shard(&path) {
                Ok(shard) => self.current_samples = shard.samples.into_iter(),
                Err(err) => {
                    return Some(Err(
                        err.context(format!("failed to load shard {}", path.display()))
                    ))
                }
            }
        }
    }
}

pub struct LiveDataset {
    logs: Vec<PathBuf>,
    config: PipelineConfig,
    review: Option<ReviewStore>,
}

impl LiveDataset {
    pub fn new(logs: Vec<PathBuf>, config: PipelineConfig, review: Option<ReviewStore>) -> Self {
        Self {
            logs,
            config,
            review,
        }
    }

    pub fn iter(self) -> SampleIter {
        Box::new(LiveDatasetIter {
            log_paths: self.logs,
            log_index: 0,
            current_samples: Vec::new().into_iter(),
            config: self.config,
            review: self.review,
        })
    }

    pub fn preprocess_all(self) -> Result<(Vec<PipelineOutput>, SplitBundle)> {
        let mut raws = Vec::new();
        for path in self.logs {
            raws.push(load_raw_game(&path, &self.config)?);
        }
        Ok(preprocess_logs_with_splits(
            raws,
            &self.config,
            self.review.as_ref(),
        ))
    }
}

struct LiveDatasetIter {
    log_paths: Vec<PathBuf>,
    log_index: usize,
    current_samples: std::vec::IntoIter<TrainingSample>,
    config: PipelineConfig,
    review: Option<ReviewStore>,
}

impl Iterator for LiveDatasetIter {
    type Item = Result<TrainingSample>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(sample) = self.current_samples.next() {
                return Some(Ok(sample));
            }
            let path = self.log_paths.get(self.log_index)?.clone();
            self.log_index += 1;
            match preprocess_log(&path, &self.config, self.review.as_ref()) {
                Ok(output) => self.current_samples = output.samples.into_iter(),
                Err(err) => {
                    return Some(Err(
                        err.context(format!("failed to preprocess log {}", path.display()))
                    ));
                }
            }
        }
    }
}

pub fn load_archived_game(path: &Path) -> Result<GameShard> {
    load_shard(path)
}
