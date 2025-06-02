use queensac::configuration::get_configuration;
use queensac::db::init_db;
use queensac::domain::{NewSubscriber, RepositoryURL};
use queensac::email_client::EmailClient;
use queensac::schedule::sse::stream_link_checks;
use queensac::{cancel_repository_checker, check_repository_links};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
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

async fn check_handler(
    State((_pool, email_client)): State<(PgPool, Arc<EmailClient>)>,
    Json(payload): Json<CheckRequest>,
) -> Result<&'static str, StatusCode> {
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
    email_client
        .send_email(
            payload.subscriber.email().clone(),
            "Repository checker started".to_string(),
            "Repository checker started".to_string(),
            "Repository checker started".to_string(),
        )
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            StatusCode::BAD_REQUEST
        })?;
    Ok("Repository checker started")
}

#[derive(Deserialize)]
struct CancelRequest {
    subscriber: NewSubscriber,
}

async fn cancel_handler(
    State((_pool, email_client)): State<(PgPool, Arc<EmailClient>)>,
    Json(payload): Json<CancelRequest>,
) -> Result<&'static str, StatusCode> {
    if let Err(e) = cancel_repository_checker(
        payload.subscriber.repository_url().url(),
        payload.subscriber.branch().cloned(),
    )
    .await
    {
        error!("Repository checker failed: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    email_client
        .send_email(
            payload.subscriber.email().clone(),
            "Repository checker cancelled".to_string(),
            "Repository checker cancelled".to_string(),
            "Repository checker cancelled".to_string(),
        )
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            StatusCode::BAD_REQUEST
        })?;
    Ok("Repository checker cancelled")
}

#[derive(Deserialize)]
struct StreamRequest {
    repo_url: RepositoryURL,
    branch: Option<String>,
}

async fn stream_handler(Query(params): Query<StreamRequest>) -> impl axum::response::IntoResponse {
    stream_link_checks(params.repo_url.url().to_string(), params.branch).await
}

#[shuttle_runtime::main]
async fn main(#[shuttle_shared_db::Postgres] pool: PgPool) -> shuttle_axum::ShuttleAxum {
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

    // 이메일 클라이언트 설정 로드
    let configuration = get_configuration().expect("Failed to read configuration.");
    let sender = configuration
        .email_client
        .sender()
        .expect("Failed to create sender email");
    let email_client = EmailClient::new(
        configuration.email_client.base_url.clone(),
        sender,
        configuration.email_client.authorization_token.clone(),
        configuration.email_client.timeout(),
    );

    // FIXME: 현재는 shuttle 에서 제공하는 풀을 사용하고 있음.
    // 나중에는 shuttle 에서 배포할게 아니기 때문에 변경해야 함
    // let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    // let pool = create_pool(&database_url)
    //     .await
    //     .expect("Failed to create database pool");

    init_db(&pool).await.expect("Failed to initialize database");

    let app = app(pool, Arc::new(email_client));

    Ok(app.into())
}

fn app(pool: PgPool, email_client: Arc<EmailClient>) -> Router {
    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
        .route("/check", delete(cancel_handler))
        .route("/stream", get(stream_handler))
        .with_state((pool, email_client))
}
