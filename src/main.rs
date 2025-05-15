use queensac::{cancel_repository_checker, check_repository_links};

use axum::{
    Json, Router,
    routing::{delete, get, post},
};
use serde::Deserialize;
use std::time::Duration;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

async fn spawn_repository_checker(repo_url: &str, branch: Option<String>, interval: Duration) {
    let repo_url = repo_url.to_string();
    info!("Spawning repository checker for {}", repo_url);
    tokio::spawn(async move {
        info!("Starting repository link check for {}", repo_url);
        if let Err(e) = check_repository_links(&repo_url, branch, interval).await {
            error!("Repository checker failed: {}", e);
        }
    });
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
struct CheckRequest {
    repo_url: String,
    branch: Option<String>,
    interval_secs: u64,
}

async fn check_handler(Json(payload): Json<CheckRequest>) -> &'static str {
    info!(
        "Received check request for repository: {}, branch: {:?}",
        payload.repo_url, payload.branch
    );
    let interval = Duration::from_secs(payload.interval_secs);
    spawn_repository_checker(&payload.repo_url, payload.branch, interval).await;
    "Repository checker started"
}

#[derive(Deserialize)]
struct CancelRequest {
    repo_url: String,
    branch: Option<String>,
}

async fn cancel_handler(Json(payload): Json<CancelRequest>) -> &'static str {
    if let Err(e) = cancel_repository_checker(&payload.repo_url, payload.branch).await {
        error!("Repository checker failed: {}", e);
    }
    "Repository checker cancelled"
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_level(true)
        .with_ansi(true)
        .pretty()
        .init();

    info!("Starting Queensac service...");

    let app = app();

    Ok(app.into())
}

fn app() -> Router {
    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
        .route("/cancel", delete(cancel_handler))
}
