// todo 어떤게 깔끔한 import 구조인지, 조사하기. 베스트 쁘락띠쓰 찾기.
use queensac::api::dto::*;
use queensac::configuration::{Settings, get_configuration};
use queensac::db::init_db;
use queensac::domain::SubscriberEmail;
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
use chrono::{FixedOffset, Utc};
use sqlx::PgPool;
use std::{fmt, sync::Arc, time::Duration};
use tower_http::cors::CorsLayer;
use tracing::{Level, error, info};
use tracing_subscriber::{FmtSubscriber, fmt::format::Writer, fmt::time::FormatTime};

async fn spawn_repository_checker(
    repo_url: &str,
    branch: Option<String>,
    interval: Duration,
    email_client: Arc<EmailClient>,
    subscriber_email: SubscriberEmail,
) -> Result<(), String> {
    let repo_url = repo_url.to_string();
    info!("Spawning repository checker for {}", repo_url);
    tokio::spawn(async move {
        info!("Starting repository link check for {}", repo_url);
        if let Err(e) =
            check_repository_links(&repo_url, branch, interval, &email_client, subscriber_email)
                .await
        {
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
        email_client.clone(),
        payload.subscriber.email().clone(),
    )
    .await
    {
        error!("Failed to spawn repository checker: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    email_client
        .send_email_with_retry(
            payload.subscriber.email().clone(),
            "Repository checker started".to_string(),
            "<p>Repository checker started</p>".to_string(),
            "broadcast".to_string(),
            3,
            Duration::from_secs(60),
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
        .send_email_with_retry(
            payload.subscriber.email().clone(),
            "Repository checker cancelled".to_string(),
            "<p>Repository checker cancelled</p>".to_string(),
            "broadcast".to_string(),
            3,
            Duration::from_secs(60),
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
        .with_timer(KoreanTime)
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

/// The offset in seconds for Korean Standard Time (UTC+9)
const KST_OFFSET: i32 = 9 * 3600;

/// A time formatter that outputs timestamps in Korean Standard Time (KST)
///
/// This struct implements the `FormatTime` trait to format timestamps in KST
/// with millisecond precision and timezone offset.
///
/// # Format
/// The output format is: `YYYY-MM-DDThh:mm:ss.sss+09:00`
///
/// # Example
/// ```
/// use tracing_subscriber::fmt::time::FormatTime;
///
/// let formatter = KoreanTime;
/// // Will output something like: 2024-02-14T15:30:45.123+09:00
/// ```
struct KoreanTime;

impl FormatTime for KoreanTime {
    fn format_time(&self, w: &mut Writer<'_>) -> Result<(), fmt::Error> {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(KST_OFFSET).unwrap());
        write!(w, "{}", now.format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}
