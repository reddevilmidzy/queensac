use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Router, extract::Query, routing::get};
use futures::StreamExt;
use futures::stream::{self, Stream};
use queensac::LinkInfo;
use queensac::RepositoryURL;
use queensac::sse::LinkCheckEvent;
use serde::Deserialize;
use std::convert::Infallible;
use std::pin::Pin;
use tokio::net::TcpListener;
use tokio::time::{Duration, timeout};

// fixme 이거 사실 main.rs에 있는 거랑 똑같다.
// 이 테스트의 위치도 이상하고 StreamRequest 스트럭의 위치도 이상함.
#[derive(Deserialize)]
#[allow(dead_code)]
struct StreamRequest {
    repo_url: RepositoryURL,
    branch: Option<String>,
}

#[tokio::test]
async fn test_sse_stream() {
    // Arrange: 테스트 서버와 클라이언트 설정
    let app = Router::new().route(
        "/stream",
        get(|Query(_params): Query<StreamRequest>| async move {
            let mock_links = vec![
                LinkInfo {
                    url: "https://example.com/1".to_string(),
                    file_path: "test1.md".to_string(),
                    line_number: 1,
                },
                LinkInfo {
                    url: "https://example.com/2".to_string(),
                    file_path: "test2.md".to_string(),
                    line_number: 2,
                },
                LinkInfo {
                    url: "https://example.com/3".to_string(),
                    file_path: "test3.md".to_string(),
                    line_number: 3,
                },
            ];

            let links_stream = stream::iter(mock_links);
            let events_stream = links_stream.then(|link| async move {
                let event = LinkCheckEvent {
                    url: link.url,
                    file_path: link.file_path,
                    line_number: link.line_number as u32,
                    status: "valid".to_string(),
                    message: None,
                };
                Ok(Event::default().json_data(event).unwrap())
            });

            Sse::new(Box::pin(events_stream)
                as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>)
            .keep_alive(KeepAlive::default())
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Act: SSE 요청 전송 및 응답 수신
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "http://{}/stream?repo_url=https://github.com/test/repo&branch=main",
            addr
        ))
        .send()
        .await
        .unwrap();

    // Assert: 응답 헤더 검증
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    let mut stream = response.bytes_stream().map(|chunk| chunk.unwrap());
    let mut events = Vec::new();

    // 스트림 읽기 작업에 5초 타임아웃 설정
    let timeout_duration = Duration::from_secs(5);
    let stream_future = async {
        while let Some(chunk) = stream.next().await {
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                if line.starts_with("data:") {
                    let data = line[5..].trim();
                    if !data.is_empty() {
                        events.push(data.to_string());
                    }
                }
            }

            if events.len() >= 3 {
                break;
            }
        }
    };

    // Assert: 타임아웃 내에 스트림 완료 확인
    match timeout(timeout_duration, stream_future).await {
        Ok(_) => {
            // Assert: SSE 이벤트 검증
            assert!(!events.is_empty());
            assert_eq!(events.len(), 3);

            // Assert: 각 이벤트의 구조 검증
            for event in events {
                let event_data: serde_json::Value = serde_json::from_str(&event).unwrap();
                assert!(event_data["url"].is_string());
                assert!(event_data["file_path"].is_string());
                assert!(event_data["line_number"].is_number());
                assert_eq!(event_data["status"], "valid");
            }
        }
        Err(_) => panic!(
            "테스트가 {}초 후에 타임아웃되었습니다",
            timeout_duration.as_secs()
        ),
    }
}
