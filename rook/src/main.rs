use queensac::{cancel_repository_checker, check_repository_links};

use axum::{
    Json, Router,
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use std::time::Duration;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

const GITHUB_BASE_URL: &str = "https://github.com/";
const GITHUB_URL_FORMAT: &str = "https://github.com/{owner}/{repo_name}";

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
    repo: RepositoryURL,
    branch: Option<String>,
    interval_secs: Option<u64>,
}

async fn check_handler(Json(payload): Json<CheckRequest>) -> Result<&'static str, StatusCode> {
    info!(
        "Received check request for repository: {}, branch: {:?}",
        payload.repo.url, payload.branch
    );
    // FIXME 일단 interval_secs 는 유저가 수정할 수 없게 할 거긴 한데, 일단 테스트할 때 편하게 요청을 받아보자.
    let interval = payload.interval_secs.unwrap_or(120);
    let interval = Duration::from_secs(interval);
    if let Err(e) = spawn_repository_checker(&payload.repo.url, payload.branch, interval).await {
        error!("Failed to spawn repository checker: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok("Repository checker started")
}

#[derive(Deserialize)]
struct CancelRequest {
    repo: RepositoryURL,
    branch: Option<String>,
}

/// Represents a GitHub repository URL.
///
/// This struct ensures that the URL is valid and follows the format
/// `https://github.com/{owner}/{repo_name}`. It includes validation logic
/// to enforce this format.
#[derive(Debug, Clone)]
struct RepositoryURL {
    /// The URL of the repository.
    url: String,
}

impl<'de> Deserialize<'de> for RepositoryURL {
    /// Custom deserialization logic for `RepositoryURL`.
    ///
    /// This implementation ensures that the URL is validated during
    /// deserialization. If the URL is invalid, an error is returned.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let url = String::deserialize(deserializer)?;
        let repo = RepositoryURL { url };
        repo.validate().map_err(serde::de::Error::custom)?;
        Ok(repo)
    }
}

impl RepositoryURL {
    fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with(GITHUB_BASE_URL) {
            return Err(format!("URL must start with {}", GITHUB_BASE_URL));
        }
        let parts: Vec<&str> = self
            .url
            .trim_start_matches(GITHUB_BASE_URL)
            .split('/')
            .collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(format!("URL must be in format {}", GITHUB_URL_FORMAT));
        }
        Ok(())
    }
}

async fn cancel_handler(Json(payload): Json<CancelRequest>) -> Result<&'static str, StatusCode> {
    if let Err(e) = cancel_repository_checker(&payload.repo.url, payload.branch).await {
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

    let app = app();

    Ok(app.into())
}

fn app() -> Router {
    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
        .route("/check", delete(cancel_handler))
}
