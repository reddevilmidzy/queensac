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
    /// Asynchronously constructs an `Application` instance with the specified configuration.
    ///
    /// Initializes the application's port and router using the provided settings.
    /// Returns an `Application` on success, or an I/O error if initialization fails.
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        // Wrap once, then reuse.
        let configuration = Arc::new(configuration);
        let port = configuration.application.port;
        let router = Self::app(configuration.clone());

        Ok(Self { port, router })
    }

    /// Constructs the Axum router with configured routes, shared state, and CORS settings.
    ///
    /// The router includes the following routes:
    /// - `/`: Returns a static string.
    /// - `/health`: Returns a health check response.
    /// - `/stream`: Handles streaming requests with query parameters.
    ///
    /// CORS is configured based on the provided settings, allowing specified origins, HTTP methods, headers, and credentials.
    ///
    /// # Panics
    ///
    /// Panics if any of the configured CORS origins are invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// let settings = Arc::new(Settings::default());
    /// let router = app(settings);
    /// ```
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

/// Health check endpoint that returns a static "OK" response.
///
/// # Examples
///
/// ```
/// let status = health_check().await;
/// assert_eq!(status, "OK");
/// ```
async fn health_check() -> &'static str {
    "OK"
}

/// Handles streaming link checks for a repository.
///
/// Extracts repository URL and optional branch from the query parameters,
/// then performs streaming link checks and returns the result as the response.
///
/// # Examples
///
/// ```
/// // Example request: GET /stream?repo_url=https://github.com/example/repo&branch=main
/// // The handler will respond with the result of streaming link checks.
/// ```
async fn stream_handler(Query(params): Query<StreamRequest>) -> impl axum::response::IntoResponse {
    stream_link_checks(params.repo_url.url().to_string(), params.branch).await
}

#[derive(Deserialize)]
pub struct StreamRequest {
    pub repo_url: RepositoryURL,
    pub branch: Option<String>,
}
