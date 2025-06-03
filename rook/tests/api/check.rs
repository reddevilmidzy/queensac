use crate::helpers::{TestRouter, create_test_repo_url, create_test_subscriber};

use axum::{
    extract::Json,
    routing::{delete, post},
};
use mockall::mock;
use queensac::NewSubscriber;
use queensac::api::dto::{CancelRequest, CheckRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
struct CheckResponse {
    task_id: String,
}

mock! {
    #[derive(Clone)]
    CheckService {
        fn check_repository(&self, subscriber: NewSubscriber, interval_secs: Option<u64>) -> String;
        fn cancel_check(&self, subscriber: NewSubscriber) -> ();
    }
}

#[tokio::test]
async fn test_check_post() {
    // Arrange
    let mut mock_service = MockCheckService::new();
    mock_service
        .expect_check_repository()
        .returning(|subscriber, interval_secs| {
            if subscriber.repository_url().url() == create_test_repo_url().url()
                && subscriber.branch() == Some(&"main".to_string())
                && interval_secs == Some(300)
            {
                "test-task-id".to_string()
            } else {
                panic!("Unexpected arguments to check_repository")
            }
        });

    let mock_service = Arc::new(mock_service);
    let mock_service_clone = mock_service.clone();

    let test_router = TestRouter::new()
        .await
        .with_route(
            "/check",
            post(move |Json(params): Json<CheckRequest>| {
                let mock_service = mock_service_clone.clone();
                async move {
                    let task_id =
                        mock_service.check_repository(params.subscriber, params.interval_secs);
                    Json(CheckResponse { task_id })
                }
            }),
        )
        .await;

    // Act
    let client = test_router.get_client();
    let response = client
        .post(test_router.get_url("/check"))
        .json(&CheckRequest {
            subscriber: create_test_subscriber(),
            interval_secs: Some(300),
        })
        .send()
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status(), 200);
    let check_response: CheckResponse = response.json().await.unwrap();
    assert_eq!(check_response.task_id, "test-task-id");
}

#[tokio::test]
async fn test_check_delete() {
    // Arrange
    let mut mock_service = MockCheckService::new();
    mock_service.expect_cancel_check().returning(|subscriber| {
        if subscriber.repository_url().url() == create_test_repo_url().url()
            && subscriber.branch() == Some(&"main".to_string())
        {
            ()
        } else {
            panic!("Unexpected arguments to cancel_check")
        }
    });

    let mock_service = Arc::new(mock_service);
    let mock_service_clone = mock_service.clone();

    let test_router = TestRouter::new()
        .await
        .with_route(
            "/check",
            delete(move |Json(params): Json<CancelRequest>| {
                let mock_service = mock_service_clone.clone();
                async move {
                    mock_service.cancel_check(params.subscriber);
                    axum::http::StatusCode::NO_CONTENT
                }
            }),
        )
        .await;

    // Act
    let client = test_router.get_client();
    let response = client
        .delete(test_router.get_url("/check"))
        .json(&CancelRequest {
            subscriber: create_test_subscriber(),
        })
        .send()
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status(), 204);
}

#[tokio::test]
async fn test_check_post_invalid_repo_url() {
    // Arrange
    let mock_service = MockCheckService::new();
    // No expectations set as the request should fail before reaching the service

    let mock_service = Arc::new(mock_service);
    let mock_service_clone = mock_service.clone();

    let test_router = TestRouter::new()
        .await
        .with_route(
            "/check",
            post(move |Json(params): Json<CheckRequest>| {
                let mock_service = mock_service_clone.clone();
                async move {
                    let task_id =
                        mock_service.check_repository(params.subscriber, params.interval_secs);
                    Json(CheckResponse { task_id })
                }
            }),
        )
        .await;

    // Act
    let client = test_router.get_client();
    let response = client
        .post(test_router.get_url("/check"))
        .json(&serde_json::json!({
            "subscriber": {
                "email": "test@example.com",
                "repository_url": "invalid-url",
                "branch": "main"
            },
            "interval_secs": 300
        }))
        .send()
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status(), 422);
}
