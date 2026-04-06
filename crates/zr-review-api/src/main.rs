use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use zr_core::config::{PipelineConfig, TeamSelector};
use zr_core::pipeline::preprocess_log_with_raw;
use zr_core::review::{ReviewStore, ReviewVerdict, load_review_store, save_review_store};
use zr_core::{build_review_payload, TeamColor};

#[derive(Parser)]
#[command(name = "zr-review-api", about = "Local review API for Zombie Resurrection")]
struct Cli {
    #[arg(long)]
    logs_dir: PathBuf,

    #[arg(long)]
    review_file: PathBuf,

    #[arg(long)]
    team_name: Option<String>,

    #[arg(long)]
    team_color: Option<String>,

    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,

    #[arg(long, default_value = "apps/zr-review-web/dist")]
    web_root: PathBuf,
}

#[derive(Clone)]
struct AppState {
    logs_dir: PathBuf,
    review_file: PathBuf,
    config: PipelineConfig,
    review_store: Arc<Mutex<ReviewStore>>,
}

#[derive(Debug, Serialize)]
struct GameListItem {
    id: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct GameQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct ReviewUpdateRequest {
    game_id: String,
    sequence_index: usize,
    verdict: ReviewVerdict,
    note: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let review_store = load_review_store(&cli.review_file)?;
    let mut config = PipelineConfig::default();
    config.target_team = if let Some(name) = cli.team_name {
        TeamSelector::Name(name)
    } else {
        TeamSelector::Color(match cli.team_color.as_deref() {
            Some("yellow") => TeamColor::Yellow,
            _ => TeamColor::Blue,
        })
    };

    if let Some(parent) = cli.review_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let state = AppState {
        logs_dir: cli.logs_dir,
        review_file: cli.review_file,
        config,
        review_store: Arc::new(Mutex::new(review_store)),
    };

    let api = Router::new()
        .route("/api/games", get(list_games))
        .route("/api/game", get(load_game))
        .route("/api/review", post(update_review))
        .with_state(state.clone())
        .layer(CorsLayer::permissive());

    let app = Router::new()
        .nest_service("/", ServeDir::new(&cli.web_root))
        .merge(api);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn list_games(State(state): State<AppState>) -> Result<Json<Vec<GameListItem>>, AppError> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(&state.logs_dir).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        if name.ends_with(".log") || name.ends_with(".log.gz") {
            items.push(GameListItem {
                id: name.replace('.', "_"),
                path: path.display().to_string(),
            });
        }
    }
    items.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Json(items))
}

async fn load_game(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Json<zr_core::ReviewGamePayload>, AppError> {
    let path = PathBuf::from(&query.path);
    if !path.exists() {
        bail_app("game path does not exist")?;
    }
    let review = state.review_store.lock().map_err(|_| AppError::msg("review store lock poisoned"))?.clone();
    let (output, raw_frames) = preprocess_log_with_raw(&path, &state.config, Some(&review)).map_err(AppError::from)?;
    Ok(Json(build_review_payload(&output, &raw_frames, &review)))
}

async fn update_review(
    State(state): State<AppState>,
    Json(request): Json<ReviewUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut store = state.review_store.lock().map_err(|_| AppError::msg("review store lock poisoned"))?;
    store.set(&request.game_id, request.sequence_index, request.verdict, request.note);
    save_review_store(&state.review_file, &store).map_err(AppError::from)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug)]
struct AppError {
    message: String,
}

impl AppError {
    fn msg(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        Self { message: value.to_string() }
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self { message: value.to_string() }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, self.message).into_response()
    }
}

fn bail_app(message: impl Into<String>) -> Result<(), AppError> {
    Err(AppError::msg(message))
}

#[allow(dead_code)]
fn ensure_inside(base: &Path, path: &Path) -> Result<(), AppError> {
    let base = base.canonicalize().map_err(AppError::from)?;
    let path = path.canonicalize().map_err(AppError::from)?;
    if !path.starts_with(&base) {
        bail_app("path is outside logs directory")?;
    }
    Ok(())
}
