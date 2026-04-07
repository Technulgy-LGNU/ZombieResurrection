use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use zr_core::config::{PipelineConfig, TeamSelector};
use zr_core::pipeline::preprocess_review_log;
use zr_core::PipelineOutput;
use zr_core::review::{ReviewStore, ReviewVerdict, load_review_store, save_review_store};
use zr_core::{
    ReviewGamePayload, ReviewSequenceQueryPayload, TeamColor, build_review_payload,
    build_review_sequence_payload,
};

static FRONTEND_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../apps/zr-review-web/dist");

#[derive(Parser)]
#[command(name = "zr-review", about = "Local review workstation for Zombie Resurrection")]
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
}

#[derive(Clone)]
struct AppState {
    logs_dir: PathBuf,
    review_file: PathBuf,
    config: PipelineConfig,
    review_store: Arc<Mutex<ReviewStore>>,
    game_cache: Arc<Mutex<HashMap<PathBuf, Arc<CachedGame>>>>,
}

struct CachedGame {
    game: ReviewGamePayload,
    output: PipelineOutput,
    raw_frames: Vec<zr_core::types::CleanFrame>,
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
struct SequenceQuery {
    path: String,
    sequence_index: usize,
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
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_writer(std::io::stdout)
        .compact()
        .init();

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
        game_cache: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/api/games", get(list_games))
        .route("/api/game", get(load_game))
        .route("/api/sequence", get(load_sequence))
        .route("/api/review", post(update_review))
        .route("/{*path}", get(serve_frontend))
        .fallback(get(serve_frontend))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                        .on_request(DefaultOnRequest::new().level(Level::INFO))
                        .on_response(DefaultOnResponse::new().level(Level::INFO)),
                )
                .layer(CorsLayer::permissive()),
        );

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    let local_addr = listener.local_addr()?;
    println!("zr-review listening on http://{local_addr}");
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
) -> Result<Json<ReviewGamePayload>, AppError> {
    let cached = load_or_build_cached_game(&state, &PathBuf::from(&query.path))?;
    Ok(Json(cached.game.clone()))
}

async fn load_sequence(
    State(state): State<AppState>,
    Query(query): Query<SequenceQuery>,
) -> Result<Json<ReviewSequenceQueryPayload>, AppError> {
    let cached = load_or_build_cached_game(&state, &PathBuf::from(&query.path))?;
    let review = state
        .review_store
        .lock()
        .map_err(|_| AppError::msg("review store lock poisoned"))?
        .clone();
    let sequence = build_review_sequence_payload(
        &cached.output,
        &cached.raw_frames,
        &review,
        query.sequence_index,
    )
    .ok_or_else(|| AppError::msg("sequence not found"))?;
    Ok(Json(sequence))
}

async fn update_review(
    State(state): State<AppState>,
    Json(request): Json<ReviewUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut store = state
        .review_store
        .lock()
        .map_err(|_| AppError::msg("review store lock poisoned"))?;
    store.set(
        &request.game_id,
        request.sequence_index,
        request.verdict,
        request.note,
    );
    save_review_store(&state.review_file, &store).map_err(AppError::from)?;
    refresh_cached_review(&state, &request.game_id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn load_or_build_cached_game(state: &AppState, path: &PathBuf) -> Result<Arc<CachedGame>, AppError> {
    if !path.exists() {
        return Err(AppError::msg("game path does not exist"));
    }
    if !path.starts_with(&state.logs_dir) {
        return Err(AppError::msg("game path is outside logs_dir"));
    }

    let cached = state
        .game_cache
        .lock()
        .map_err(|_| AppError::msg("game cache lock poisoned"))?
        .get(path)
        .cloned();
    if let Some(cached) = cached {
        return Ok(cached);
    }

    let review = state
        .review_store
        .lock()
        .map_err(|_| AppError::msg("review store lock poisoned"))?
        .clone();
    let (output, raw_frames) = preprocess_review_log(path, &state.config).map_err(AppError::from)?;
    let game = build_review_payload(&output, &raw_frames, &review);
    let cached = Arc::new(CachedGame {
        game,
        output,
        raw_frames,
    });
    let mut cache = state
        .game_cache
        .lock()
        .map_err(|_| AppError::msg("game cache lock poisoned"))?;
    cache.clear();
    cache.insert(path.clone(), Arc::clone(&cached));
    Ok(cached)
}

fn refresh_cached_review(state: &AppState, game_id: &str) -> Result<(), AppError> {
    let mut cache = state
        .game_cache
        .lock()
        .map_err(|_| AppError::msg("game cache lock poisoned"))?;
    cache.retain(|_, cached| cached.game.game_id != game_id);
    Ok(())
}

async fn serve_frontend(path: Option<axum::extract::Path<String>>) -> Response {
    let requested = path
        .map(|value| value.0)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "index.html".to_string());

    if requested.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    let file = FRONTEND_DIST
        .get_file(&requested)
        .or_else(|| FRONTEND_DIST.get_file("index.html"));

    match file {
        Some(file) => {
            let mime = content_type_for(file.path().extension().and_then(|value| value.to_str()));
            let mut response = Response::new(Body::from(file.contents().to_vec()));
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
            response
        }
        None => (StatusCode::NOT_FOUND, "missing embedded frontend").into_response(),
    }
}

fn content_type_for(extension: Option<&str>) -> &'static str {
    match extension {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
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
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.message).into_response()
    }
}
