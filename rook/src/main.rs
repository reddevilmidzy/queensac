// todo 어떤게 깔끔한 import 구조인지, 조사하기. 베스트 쁘락띠쓰 찾기.
use queensac::api::dto::*;
use queensac::configuration::{Settings, get_configuration};
use queensac::db::init_db;
use queensac::email_client::EmailClient;
use queensac::{cancel_repository_checker, check_repository_links, stream_link_checks};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{
        HeaderValue, Method, StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    },
    routing::{delete, get, post},
};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
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

async fn check_handler(
    State((_pool, email_client, configuration)): State<(PgPool, Arc<EmailClient>, Arc<Settings>)>,
    Json(payload): Json<CheckRequest>,
) -> Result<&'static str, StatusCode> {
    info!(
        "Received check request for repository: {}, branch: {:?}, email: {}",
        payload.subscriber.repository_url().url(),
        payload.subscriber.branch(),
        payload.subscriber.email().as_str()
    );
    let interval = Duration::from_secs(configuration.repository_checker.interval_seconds);
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
            "<p>Repository checker started</p>".to_string(),
            "broadcast".to_string(),
        )
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            StatusCode::BAD_REQUEST
        })?;
    Ok("Repository checker started")
}

async fn cancel_handler(
    State((_pool, email_client, _configuration)): State<(PgPool, Arc<EmailClient>, Arc<Settings>)>,
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
            "<p>Repository checker cancelled</p>".to_string(),
            "broadcast".to_string(),
        )
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            StatusCode::BAD_REQUEST
        })?;
    Ok("Repository checker cancelled")
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

    info!("Starting queensac service...");
    dotenvy::dotenv().ok();

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

    let configuration = Arc::new(configuration);
    let app = app(pool, Arc::new(email_client), configuration);

    Ok(app.into())
}

fn app(pool: PgPool, email_client: Arc<EmailClient>, configuration: Arc<Settings>) -> Router {
    let allowed_origins: Vec<HeaderValue> = configuration
        .cors
        .allowed_origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin)
                .map_err(|e| format!("Invalid CORS origin '{}': {}", origin, e))
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse CORS origins");

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION, ACCEPT])
        .allow_credentials(true);

    Router::new()
        .route("/", get(|| async { "Sacrifice the Queen!!" }))
        .route("/health", get(health_check))
        .route("/check", post(check_handler))
        .route("/check", delete(cancel_handler))
        .route("/stream", get(stream_handler))
        .with_state((pool, email_client, configuration))
        .layer(cors)
}
