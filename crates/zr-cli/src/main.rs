use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use globwalk::GlobWalkerBuilder;
use zr_core::archive::{write_dataset, GameShard};
use zr_core::config::{PipelineConfig, TeamSelector};
use zr_core::pipeline::{audit_log, summarize_games_by_phase};
use zr_core::review::load_review_store;
use zr_core::types::TeamColor;

#[derive(Parser)]
#[command(
    name = "zombie-resurrection",
    about = "Training-data pipeline and tooling for RoboCup SSL logs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Audit(InputArgs),
    Preprocess(PreprocessArgs),
    Samples(SamplesArgs),
    InitConfig { output: PathBuf },
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

#[derive(Args, Clone)]
struct PreprocessArgs {
    #[command(flatten)]
    input: InputArgs,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long)]
    review_file: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct SamplesArgs {
    #[arg(long)]
    manifest: PathBuf,

    #[arg(long, default_value_t = 3)]
    limit: usize,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TeamColorArg {
    Yellow,
    Blue,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Audit(args) => run_audit(args),
        Commands::Preprocess(args) => run_preprocess(args),
        Commands::Samples(args) => run_samples(args),
        Commands::InitConfig { output } => run_init_config(&output),
    }
}

fn run_audit(args: InputArgs) -> Result<()> {
    let config = load_config(&args)?;
    let paths = expand_inputs(&args.inputs)?;
    for path in paths {
        let audit = audit_log(&path, &config)?;
        println!("{}", serde_json::to_string_pretty(&audit)?);
    }
    Ok(())
}

fn run_preprocess(args: PreprocessArgs) -> Result<()> {
    let config = load_config(&args.input)?;
    let paths = expand_inputs(&args.input.inputs)?;
    let review = match args.review_file.as_ref() {
        Some(path) => Some(load_review_store(path)?),
        None => None,
    };

    let dataset = zr_core::LiveDataset::new(paths, config, review);
    let (outputs, split_bundle) = dataset.preprocess_all()?;
    let shards = outputs
        .iter()
        .map(|output| GameShard {
            metadata: output.metadata.clone(),
            review_sequences: output.review_game.sequence_summaries.clone(),
            samples: output.samples.clone(),
        })
        .collect::<Vec<_>>();

    let manifest = write_dataset(&args.output_dir, &shards, &split_bundle)?;
    let by_phase = summarize_games_by_phase(&outputs);
    println!(
        "wrote {} samples across {} shards",
        manifest.total_samples,
        manifest.shard_paths.len()
    );
    println!("{}", serde_json::to_string_pretty(&by_phase)?);
    Ok(())
}

fn run_samples(args: SamplesArgs) -> Result<()> {
    let manifest = zr_core::load_manifest(&args.manifest)?;
    println!("total_samples={}", manifest.total_samples);
    for shard in manifest.shard_paths.iter().take(args.limit) {
        println!("shard={shard}");
    }
    Ok(())
}

fn run_init_config(output: &Path) -> Result<()> {
    let config = PipelineConfig::default();
    let content = serde_json::to_string_pretty(&config)?;
    fs::write(output, content)
        .with_context(|| format!("failed to write config {}", output.display()))?;
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
