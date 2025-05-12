use queensac::check_repository_links;

use axum::{Router, routing::get};
use std::time::Duration;

async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/check", get(health_check));

    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .unwrap();

    let repo_url = "https://github.com/reddevilmidzy/redddy-action";
    let interval_duration = Duration::from_secs(60);
    let _repo_check = check_repository_links(repo_url, interval_duration);

    axum::serve(listener, app).await.unwrap();
}
