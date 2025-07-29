use crate::{RepositoryURL, Settings, stream_link_checks};
use axum::{Router, extract::Query, http::HeaderValue, routing::get};
use reqwest::{
    Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub struct Application {
    pub port: u16,
    pub router: Router,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        // Wrap once, then reuse.
        let configuration = Arc::new(configuration);
        let port = configuration.application.port;
        let router = Self::app(configuration.clone());

        Ok(Self { port, router })
    }

    pub fn app(configuration: Arc<Settings>) -> Router {
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
            .route("/stream", get(stream_handler))
            .with_state(configuration)
            .layer(cors)
    }
}

async fn health_check() -> &'static str {
    "OK"
}

async fn stream_handler(Query(params): Query<StreamRequest>) -> impl axum::response::IntoResponse {
    stream_link_checks(params.repo_url.url().to_string(), params.branch).await
}

#[derive(Deserialize)]
pub struct StreamRequest {
    pub repo_url: RepositoryURL,
    pub branch: Option<String>,
}
