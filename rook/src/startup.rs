use std::{sync::Arc, time::Duration};

use crate::{
    CancelRequest, CheckRequest, EmailClient, Settings, StreamRequest, SubscriberEmail,
    cancel_repository_checker, check_repository_links, init_db, stream_link_checks,
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::HeaderValue,
    routing::{delete, get, post},
};
use reqwest::{
    Method, StatusCode,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

pub struct Application {
    pub port: u16,
    pub router: Router,
}

impl Application {
    pub async fn build(configuration: Settings, pool: PgPool) -> Result<Self, std::io::Error> {
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

        init_db(&pool).await.expect("Failed to initialize database");
        let router = Self::app(
            pool,
            Arc::new(email_client),
            Arc::new(configuration.clone()),
        );

        let port = configuration.application.port;

        Ok(Self { port, router })
    }

    pub fn app(
        pool: PgPool,
        email_client: Arc<EmailClient>,
        configuration: Arc<Settings>,
    ) -> Router {
        let allowed_origins: Vec<HeaderValue> = configuration
            .cors
            .allowed_origins
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin)
                    .map_err(|e| format!("Invalid CORS origin '{origin}': {e}"))
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
}

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
