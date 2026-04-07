# Zombie Resurrection

Rust workspace for turning raw RoboCup SSL logs into model-ready training samples.

## What it provides

- Live mode: parse `.log` / `.log.gz` files with `loguna`, clean them, segment sequences, and emit per-robot training samples on demand.
- Archive mode: preprocess logs into compressed `rkyv + zstd` shards with manifest, split bundle, and normalization stats.
- Review tool: embedded Rust review server plus React/Vite web app for sequence review, playback, overlays, and bulk triage.
- Optional `tch` feature in `zr-core`: convert sample batches into weighted `tch::Tensor` values.

## Workspace crates

- `crates/zr-core` - core library only
- `crates/zr-cli` - CLI tools
- `crates/zr-review` - embedded local review server
- `apps/zr-review-web` - review web frontend

## Binaries

- `zombie-resurrection` — interactive preprocessing with optional manual review
- `zr-auto-pipeline` — fully-automated raw → training-data pipeline (no human intervention)
- `zr-review` — embedded local review server

## Quick start

Generate a starter config:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- init-config --output zr-config.json
```

Audit one log:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- audit \
  --input /path/to/raw-log \
  --team-name YourTeam
```

Preprocess logs into archived shards:

```sh
cargo run -p zr-cli --bin zombie-resurrection -- preprocess \
  --input /path/to/raw-logs \
  --team-name YourTeam \
  --output-dir data/processed \
  --review-file data/review.json
```

Automated pipeline (no review, enhanced cleaning):

```sh
cargo run -p zr-cli --bin zr-auto-pipeline -- \
  --input /path/to/raw-logs \
  --team-name YourTeam \
  --output-dir data/auto \
  --verbose
```

Run the embedded review server:

```sh
cargo run -p zr-review --bin zr-review -- \
  --logs-dir /path/to/raw-logs \
  --team-name YourTeam \
  --review-file data/review.json
```

The frontend is built and embedded automatically by `build.rs`.

For frontend-only iteration you can still run:

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
config.target_team = TeamSelector::Name("YourTeam".to_string());

let dataset = LiveDataset::new(
    vec![PathBuf::from("/path/to/raw-log")],
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
