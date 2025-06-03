use axum::{
    Router,
    extract::Json,
    response::IntoResponse,
    routing::{delete, post},
};
use queensac::RepositoryURL;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

#[derive(Deserialize, Serialize)]
struct CheckRequest {
    repo_url: RepositoryURL,
    branch: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct CheckResponse {
    task_id: String,
}

async fn test_check_handler(Json(_params): Json<CheckRequest>) -> impl IntoResponse {
    Json(CheckResponse {
        task_id: "test-task-id".to_string(),
    })
}

async fn test_cancel_handler() -> impl IntoResponse {
    axum::http::StatusCode::NO_CONTENT
}

#[tokio::test]
async fn test_check_post() {
    // Arrange: 테스트 서버 설정
    let app = Router::new().route("/check", post(test_check_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Act: POST 요청 전송
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/check", addr))
        .json(&CheckRequest {
            repo_url: RepositoryURL::new("https://github.com/test/repo").unwrap(),
            branch: Some("main".to_string()),
        })
        .send()
        .await
        .unwrap();

    // Assert: 응답 검증
    assert_eq!(response.status(), 200);
    let check_response: CheckResponse = response.json().await.unwrap();
    assert_eq!(check_response.task_id, "test-task-id");
}

#[tokio::test]
async fn test_check_delete() {
    // Arrange: 테스트 서버 설정
    let app = Router::new().route("/check", delete(test_cancel_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Act: DELETE 요청 전송
    let client = reqwest::Client::new();
    let response = client
        .delete(format!("http://{}/check", addr))
        .send()
        .await
        .unwrap();

    // Assert: 응답 검증
    assert_eq!(response.status(), 204);
}

#[tokio::test]
async fn test_check_post_invalid_repo_url() {
    // Arrange: 테스트 서버 설정
    let app = Router::new().route("/check", post(test_check_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Act: 잘못된 repo_url로 POST 요청 전송
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/check", addr))
        .json(&serde_json::json!({
            "repo_url": "invalid-url",
            "branch": "main"
        }))
        .send()
        .await
        .unwrap();

    // Assert: 응답 검증
    assert_eq!(response.status(), 422);
}
