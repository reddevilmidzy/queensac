use queensac::check_repository_links;

use axum::{
    Json, Router,
    routing::{get, post},
    serve,
};
use serde::Deserialize;
use std::time::Duration;
use tokio::net::TcpListener;

async fn spawn_repository_checker(repo_url: &str, interval: Duration) {
    let repo_url = repo_url.to_string();
    tokio::spawn(async move {
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
    let interval = Duration::from_secs(payload.interval_secs);
    spawn_repository_checker(&payload.repo_url, interval).await;
    "Repository checker started"
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = TcpListener::bind("localhost:3000").await.unwrap();

    serve(listener, app).await.unwrap();
}

fn app() -> Router {
    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
}
