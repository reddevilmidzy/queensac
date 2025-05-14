use queensac::{cancel_repository_checker, check_repository_links};

use axum::{
    Json, Router,
    routing::{delete, get, post},
};
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

async fn spawn_repository_checker(repo_url: &str, interval: Duration) {
    let repo_url = repo_url.to_string();
    info!("Spawning repository checker for {}", repo_url);
    tokio::spawn(async move {
        info!("Starting repository link check for {}", repo_url);
        check_repository_links(&repo_url, interval).await;
    });
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
struct CheckRequest {
    repo_url: String,
    interval_secs: u64,
}

async fn check_handler(Json(payload): Json<CheckRequest>) -> &'static str {
    info!(
        "Received check request for repository: {}",
        payload.repo_url
    );
    let interval = Duration::from_secs(payload.interval_secs);
    spawn_repository_checker(&payload.repo_url, interval).await;
    "Repository checker started"
}

#[derive(Deserialize)]
struct CancelRequest {
    repo_url: String,
}

async fn cancel_handler(Json(payload): Json<CancelRequest>) -> &'static str {
    cancel_repository_checker(&payload.repo_url).await;
    "Repository checker cancelled"
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
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
