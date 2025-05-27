use queensac::db::{create_pool, init_db};
use queensac::domain::NewSubscriber;
use queensac::{cancel_repository_checker, check_repository_links};

use axum::{
    Json, Router,
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

async fn spawn_repository_checker(
    repo_url: &str,
    branch: Option<String>,
    interval: Duration,
) -> Result<(), String> {
    let repo_url = repo_url.to_string();
    info!("Spawning repository checker for {}", repo_url);
    tokio::spawn(async move {
        info!("Starting repository link check for {}", repo_url);
        if let Err(e) = check_repository_links(&repo_url, branch, interval).await {
            return Err(e.to_string());
        }
        Ok(())
    });
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
struct CheckRequest {
    subscriber: NewSubscriber,
    interval_secs: Option<u64>,
}

async fn check_handler(Json(payload): Json<CheckRequest>) -> Result<&'static str, StatusCode> {
    info!(
        "Received check request for repository: {}, branch: {:?}, email: {}",
        payload.subscriber.repository_url().url(),
        payload.subscriber.branch(),
        payload.subscriber.email().as_str()
    );
    // FIXME 일단 interval_secs 는 유저가 수정할 수 없게 할 거긴 한데, 일단 테스트할 때 편하게 요청을 받아보자.
    let interval = payload.interval_secs.unwrap_or(120);
    let interval = Duration::from_secs(interval);
    if let Err(e) = spawn_repository_checker(
        payload.subscriber.repository_url().url(),
        payload.subscriber.branch().cloned(),
        interval,
    )
    .await
    {
        error!("Failed to spawn repository checker: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok("Repository checker started")
}

#[derive(Deserialize)]
struct CancelRequest {
    subscriber: NewSubscriber,
}

async fn cancel_handler(Json(payload): Json<CancelRequest>) -> Result<&'static str, StatusCode> {
    if let Err(e) = cancel_repository_checker(
        payload.subscriber.repository_url().url(),
        payload.subscriber.branch().cloned(),
    )
    .await
    {
        error!("Repository checker failed: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok("Repository checker cancelled")
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

    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = create_pool(&database_url)
        .await
        .expect("Failed to create database pool");

    init_db(&pool).await.expect("Failed to initialize database");

    let app = app(pool);

    Ok(app.into())
}

fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
        .route("/check", delete(cancel_handler))
        .with_state(pool)
}
