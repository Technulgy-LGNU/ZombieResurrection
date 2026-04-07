use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, ValueEnum};
use globwalk::GlobWalkerBuilder;
use zr_core::archive::{write_dataset, GameShard};
use zr_core::config::{PipelineConfig, TeamSelector};
use zr_core::pipeline::{auto_preprocess_logs_with_splits, summarize_games_by_phase};
use zr_core::raw::load_raw_game;
use zr_core::types::TeamColor;

/// Fully-automated raw-log → training-data pipeline.
///
/// Reads raw `.log` / `.log.gz` files, applies every available automated
/// cleaning pass (visibility filtering, teleport removal, velocity-spike
/// detection, median smoothing, team-completeness checks, duplicate-timestamp
/// removal, and quality-score gating), then writes compressed `rkyv + zstd`
/// shards with a manifest and train/validation/test split bundle.
///
/// No human review step is involved — the quality gate is purely score-based.
#[derive(Parser)]
#[command(
    name = "zr-auto-pipeline",
    about = "Automated raw-log → training-data pipeline (no manual review)"
)]
struct Cli {
    #[command(flatten)]
    input: InputArgs,

    /// Directory to write shards, manifest, and split bundle to.
    #[arg(long)]
    output_dir: PathBuf,

    /// Minimum quality score for a sequence to be kept (overrides config).
    #[arg(long)]
    min_quality: Option<f32>,

    /// Print per-game audit information after processing.
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

#[derive(Args, Clone)]
struct InputArgs {
    #[arg(long = "input", required = true)]
    inputs: Vec<PathBuf>,

    #[arg(long)]
    team_name: Option<String>,

    #[arg(long, value_enum)]
    team_color: Option<TeamColorArg>,

    #[arg(long)]
    tracker_source: Option<String>,

    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TeamColorArg {
    Yellow,
    Blue,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = load_config(&cli.input)?;

    if let Some(min_q) = cli.min_quality {
        config.auto_clean.min_quality_score = min_q;
    }

    let paths = expand_inputs(&cli.input.inputs)?;
    eprintln!(
        "zr-auto-pipeline: found {} log file(s), loading…",
        paths.len()
    );

    let start = Instant::now();

    // Load all raw games.
    let mut raws = Vec::new();
    let mut load_failures = Vec::new();
    for path in &paths {
        match load_raw_game(path, &config) {
            Ok(raw) => {
                if cli.verbose {
                    eprintln!(
                        "  loaded {} — {} frames, {:.1}s, {:.0} Hz",
                        raw.metadata.game_id,
                        raw.frames.len(),
                        raw.metadata.duration_s,
                        raw.metadata.sample_rate_hz,
                    );
                    for note in &raw.audit.notes {
                        eprintln!("    note: {note}");
                    }
                }
                raws.push(raw);
            }
            Err(err) => {
                eprintln!("  SKIP {}: {err:#}", path.display());
                load_failures.push(path.clone());
            }
        }
    }

    if raws.is_empty() {
        bail!(
            "all {} log files failed to load — cannot produce a dataset",
            paths.len()
        );
    }

    eprintln!(
        "loaded {} game(s) in {:.2}s ({} failed)",
        raws.len(),
        start.elapsed().as_secs_f64(),
        load_failures.len(),
    );

    // Run the automated pipeline (enhanced cleaning + no review).
    let pipeline_start = Instant::now();
    let (outputs, split_bundle) = auto_preprocess_logs_with_splits(raws, &config);
    eprintln!(
        "pipeline finished in {:.2}s",
        pipeline_start.elapsed().as_secs_f64()
    );

    // Build shards and write to disk.
    let shards: Vec<GameShard> = outputs
        .iter()
        .map(|output| GameShard {
            metadata: output.metadata.clone(),
            review_sequences: output.review_game.sequence_summaries.clone(),
            samples: output.samples.clone(),
        })
        .collect();

    let manifest = write_dataset(&cli.output_dir, &shards, &split_bundle)?;

    // Summary.
    let total_sequences: usize = outputs
        .iter()
        .map(|o| o.review_game.sequence_summaries.len())
        .sum();
    let by_phase = summarize_games_by_phase(&outputs);
    let total_time = start.elapsed();

    eprintln!();
    eprintln!("=== zr-auto-pipeline summary ===");
    eprintln!("  log files found   : {}", paths.len());
    eprintln!("  games loaded      : {}", outputs.len());
    eprintln!("  load failures     : {}", load_failures.len());
    eprintln!("  sequences kept    : {total_sequences}");
    eprintln!(
        "  training samples  : {}",
        manifest.total_samples
    );
    eprintln!("  shards written    : {}", manifest.shard_paths.len());
    eprintln!("  output directory  : {}", cli.output_dir.display());
    eprintln!("  wall-clock time   : {:.2}s", total_time.as_secs_f64());

    // Print phase breakdown and split info.
    eprintln!();
    eprintln!("  games by phase:");
    for (phase, count) in &by_phase {
        eprintln!("    {phase}: {count}");
    }
    eprintln!();
    eprintln!("  split assignments:");
    for assignment in &split_bundle.assignments {
        let sample_count = outputs
            .iter()
            .find(|o| o.metadata.game_id == assignment.game_id)
            .map(|o| o.samples.len())
            .unwrap_or_default();
        eprintln!(
            "    {} → {} ({} samples)",
            assignment.game_id, assignment.split, sample_count,
        );
    }

    // Print auto-clean config that was used.
    eprintln!();
    eprintln!("  auto-clean settings:");
    eprintln!(
        "    min_visibility              : {}",
        config.auto_clean.min_visibility
    );
    eprintln!(
        "    min_ball_visibility         : {}",
        config.auto_clean.min_ball_visibility
    );
    eprintln!(
        "    min_visible_target_robots   : {}",
        config.auto_clean.min_visible_target_robots
    );
    eprintln!(
        "    min_visible_opponent_robots : {}",
        config.auto_clean.min_visible_opponent_robots
    );
    eprintln!(
        "    min_quality_score           : {}",
        config.auto_clean.min_quality_score
    );
    eprintln!(
        "    teleport_threshold_m        : {}",
        config.auto_clean.teleport_threshold_m
    );
    eprintln!(
        "    velocity_spike_threshold    : {} m/s",
        config.auto_clean.velocity_spike_threshold_m_s
    );
    eprintln!(
        "    position_smoothing          : {} (window={})",
        config.auto_clean.enable_position_smoothing,
        config.auto_clean.smoothing_window
    );
    eprintln!(
        "    drop_duplicate_timestamps   : {}",
        config.auto_clean.drop_duplicate_timestamps
    );

    // Also write the config used to the output directory for reproducibility.
    let used_config_path = cli.output_dir.join("pipeline-config.json");
    fs::write(
        &used_config_path,
        serde_json::to_string_pretty(&config)?,
    )
    .with_context(|| {
        format!(
            "failed to write used config to {}",
            used_config_path.display()
        )
    })?;
    eprintln!();
    eprintln!(
        "  config snapshot   : {}",
        used_config_path.display()
    );

    Ok(())
}

fn load_config(args: &InputArgs) -> Result<PipelineConfig> {
    let mut config = if let Some(path) = &args.config {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        serde_json::from_str::<PipelineConfig>(&content)?
    } else {
        PipelineConfig::default()
    };

    config.tracker_source = args.tracker_source.clone();
    config.target_team = if let Some(name) = &args.team_name {
        TeamSelector::Name(name.clone())
    } else if let Some(color) = args.team_color {
        TeamSelector::Color(match color {
            TeamColorArg::Yellow => TeamColor::Yellow,
            TeamColorArg::Blue => TeamColor::Blue,
        })
    } else {
        bail!("pass either --team-name or --team-color")
    };
    Ok(config)
}

fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for input in inputs {
        let input_str = input.to_string_lossy();
        if input_str.contains('*') || input_str.contains('?') || input_str.contains('[') {
            let parent = input.parent().unwrap_or_else(|| Path::new("."));
            let pattern = input
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("*");
            let walker = GlobWalkerBuilder::from_patterns(parent, &[pattern]).build()?;
            for entry in walker.filter_map(Result::ok) {
                let path = entry.path();
                if is_log_path(path) {
                    paths.push(path.to_path_buf());
                }
            }
        } else if input.is_dir() {
            let walker = GlobWalkerBuilder::from_patterns(input, &["*.log", "*.log.gz"]).build()?;
            for entry in walker.filter_map(Result::ok) {
                paths.push(entry.path().to_path_buf());
            }
        } else if is_log_path(input) {
            paths.push(input.clone());
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        bail!("no log files found in inputs")
    }
    Ok(paths)
}

fn is_log_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    name.ends_with(".log") || name.ends_with(".log.gz")
}
