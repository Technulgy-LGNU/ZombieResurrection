use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rkyv::{from_bytes, Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use crate::types::{GameMetadata, MatchPhase, ReviewSequenceSummary, TrainingSample};

const MAGIC: &[u8; 8] = b"ZRRYKV1\0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizationStats {
    pub velocity_mean: f32,
    pub velocity_std: f32,
    pub acceleration_mean: f32,
    pub acceleration_std: f32,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAssignment {
    pub game_id: String,
    pub split: String,
    pub phase: MatchPhase,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitBundle {
    pub assignments: Vec<SplitAssignment>,
    pub normalization: NormalizationStats,
    pub elimination_sample_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub version: u32,
    pub shard_paths: Vec<String>,
    pub total_samples: usize,
    pub games: Vec<GameMetadata>,
    pub review_sequences: Vec<(String, Vec<ReviewSequenceSummary>)>,
    pub split_bundle_path: String,
}

#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct GameShard {
    pub metadata: GameMetadata,
    pub review_sequences: Vec<ReviewSequenceSummary>,
    pub samples: Vec<TrainingSample>,
}

pub fn write_dataset(
    output_dir: &Path,
    shards: &[GameShard],
    split_bundle: &SplitBundle,
) -> Result<DatasetManifest> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;

    let mut manifest = DatasetManifest {
        version: 2,
        shard_paths: Vec::new(),
        total_samples: 0,
        games: Vec::new(),
        review_sequences: Vec::new(),
        split_bundle_path: "splits.json".to_string(),
    };

    for shard in shards {
        let shard_name = format!("{}.zrshard", shard.metadata.game_id);
        let shard_path = output_dir.join(&shard_name);
        write_shard(&shard_path, shard)?;
        manifest.total_samples += shard.samples.len();
        manifest.shard_paths.push(shard_name);
        manifest.games.push(shard.metadata.clone());
        manifest.review_sequences.push((
            shard.metadata.game_id.clone(),
            shard.review_sequences.clone(),
        ));
    }

    let manifest_path = output_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("failed to write manifest {}", manifest_path.display()))?;
    let split_path = output_dir.join(&manifest.split_bundle_path);
    std::fs::write(&split_path, serde_json::to_string_pretty(split_bundle)?)
        .with_context(|| format!("failed to write split bundle {}", split_path.display()))?;

    Ok(manifest)
}

pub fn load_manifest(path: &Path) -> Result<DatasetManifest> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_split_bundle(path: &Path) -> Result<SplitBundle> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read split bundle {}", path.display()))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_shard(path: &Path, shard: &GameShard) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("failed to create shard {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer.write_all(MAGIC)?;
    let payload = rkyv::to_bytes::<rkyv::rancor::Error>(shard)?;
    let compressed = zstd::stream::encode_all(payload.as_slice(), 8)?;
    writer.write_all(&(compressed.len() as u64).to_le_bytes())?;
    writer.write_all(&compressed)?;
    writer.flush()?;
    Ok(())
}

pub fn load_shard(path: &Path) -> Result<GameShard> {
    let file =
        File::open(path).with_context(|| format!("failed to open shard {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        bail!("invalid shard magic in {}", path.display());
    }
    let mut len_buf = [0u8; 8];
    reader.read_exact(&mut len_buf)?;
    let len = u64::from_le_bytes(len_buf) as usize;
    let mut compressed = vec![0u8; len];
    reader.read_exact(&mut compressed)?;
    let decompressed = zstd::stream::decode_all(compressed.as_slice())?;
    Ok(from_bytes::<GameShard, rkyv::rancor::Error>(&decompressed)?)
}

pub fn resolve_shard_paths(manifest_path: &Path, manifest: &DatasetManifest) -> Vec<PathBuf> {
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    manifest
        .shard_paths
        .iter()
        .map(|relative| base.join(relative))
        .collect()
}

pub fn resolve_split_bundle_path(manifest_path: &Path, manifest: &DatasetManifest) -> PathBuf {
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(&manifest.split_bundle_path)
}
