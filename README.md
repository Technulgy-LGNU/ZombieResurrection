# Zombie Resurrection

Rust workspace for turning raw RoboCup SSL logs into model-ready training samples.

## What it provides

- Live mode: parse `.log` / `.log.gz` files with `loguna`, clean them, segment sequences, and emit per-robot training samples on demand.
- Archive mode: preprocess logs into compressed `rkyv + zstd` shards with manifest, split bundle, and normalization stats.
- Review tool: local Rust API plus React/Vite web app for sequence review, playback, overlays, and bulk triage.
- Optional `tch` feature in `zr-core`: convert sample batches into weighted `tch::Tensor` values.

## Workspace crates

- `crates/zr-core` - core library only
- `crates/zr-cli` - CLI tools
- `crates/zr-review-api` - local review API server
- `apps/zr-review-web` - review web frontend

## Binaries

- `zombie-resurrection`
- `zr-review-api`

## Quick start

Generate a starter config:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- init-config --output zr-config.json
```

Audit one log:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- audit \
  --input loguna-src/2025-07-17_21-37_GROUP_PHASE_The_Bots-vs-ITAndroids.log.gz \
  --team-name The_Bots
```

Preprocess logs into archived shards:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- preprocess \
  --input loguna-src \
  --team-name The_Bots \
  --output-dir data/processed \
  --review-file data/review.json
```

Run the local review API:

```sh
cargo run -p zr-review-api --bin zr-review-api -- \
  --logs-dir loguna-src \
  --team-name The_Bots \
  --review-file data/review.json \
  --web-root apps/zr-review-web/dist
```

Build and run the web frontend in dev mode:

```sh
cd apps/zr-review-web
npm install
npm run dev
```

## Library usage

```rust
use std::path::PathBuf;

use zr_core::{LiveDataset, PipelineConfig, ReviewStore, TeamSelector};

let mut config = PipelineConfig::default();
config.target_team = TeamSelector::Name("The_Bots".to_string());

let dataset = LiveDataset::new(
    vec![PathBuf::from("loguna-src/2025-07-19_18-59_ELIMINATION_PHASE_The_Bots-vs-ITAndroids.log.gz")],
    config,
    Some(ReviewStore::default()),
);

for sample in dataset.iter().take(8) {
    let sample = sample?;
    println!("{} -> {:?}", sample.metadata.game_id, sample.target);
}
# Ok::<(), anyhow::Error>(())
```

## Current v1 scope

- Raw source: `VisionTracker2020`
- Training target: per-robot next relative delta `(dx, dy, dtheta)`
- Manual review: web queue, playback, scrubber, overlays, compare mode, keep/drop/needs-attention with notes
- Archive format: `rkyv` payload compressed with `zstd`
- Split export: train/validation/test assignments plus normalization stats and sample weights
- Identity handling: Hungarian-style matching with gating and suspicious-swap flags
- Live-play handling: referee commands plus motion heuristics
