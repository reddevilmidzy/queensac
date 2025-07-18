use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{error, info, instrument};

use crate::{LinkCheckResult, check_link, git};

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkCheckEvent {
    pub url: String,
    pub file_path: String,
    pub line_number: u32,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkCheckSummaryEvent {
    pub total: usize,
    pub valid: usize,
    pub invalid: usize,
    pub redirect: usize,
    pub moved: usize,
}

#[derive(Debug)]
struct LinkCheckCounters {
    total: AtomicUsize,
    valid: AtomicUsize,
    invalid: AtomicUsize,
    redirect: AtomicUsize,
    moved: AtomicUsize,
}

impl LinkCheckCounters {
    fn new() -> Self {
        Self {
            total: AtomicUsize::new(0),
            valid: AtomicUsize::new(0),
            invalid: AtomicUsize::new(0),
            redirect: AtomicUsize::new(0),
            moved: AtomicUsize::new(0),
        }
    }

    fn increment_total(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_valid(&self) {
        self.valid.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_invalid(&self) {
        self.invalid.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_redirect(&self) {
        self.redirect.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_moved(&self) {
        self.moved.fetch_add(1, Ordering::Relaxed);
    }

    fn to_summary(&self) -> LinkCheckSummaryEvent {
        LinkCheckSummaryEvent {
            total: self.total.load(Ordering::Relaxed),
            valid: self.valid.load(Ordering::Relaxed),
            invalid: self.invalid.load(Ordering::Relaxed),
            redirect: self.redirect.load(Ordering::Relaxed),
            moved: self.moved.load(Ordering::Relaxed),
        }
    }
}

#[instrument(skip(), fields(repo_url = repo_url))]
pub async fn stream_link_checks(
    repo_url: String,
    branch: Option<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!(
        "Starting SSE stream for repository: {} (branch: {:?})",
        repo_url, branch
    );

    let stream = match git::extract_links_from_repo_url(&repo_url, branch.clone()) {
        Ok(links) => {
            info!("Found {} links to check", links.len());

            let counters = Arc::new(LinkCheckCounters::new());

            let links_stream = stream::iter(links);
            let events_stream = links_stream
                .map({
                    let counters = Arc::clone(&counters);
                    move |link| {
                        let counters = Arc::clone(&counters);
                        async move {
                            let result = check_link(&link.url).await;

                            counters.increment_total();
                            match &result {
                                LinkCheckResult::Valid => counters.increment_valid(),
                                LinkCheckResult::Invalid(_) => counters.increment_invalid(),
                                LinkCheckResult::Redirect(_) => counters.increment_redirect(),
                                LinkCheckResult::GitHubFileMoved(_) => counters.increment_moved(),
                            };

                            let event = LinkCheckEvent {
                                url: link.url,
                                file_path: link.file_path,
                                line_number: link.line_number as u32,
                                status: match &result {
                                    LinkCheckResult::Valid => "valid".to_string(),
                                    LinkCheckResult::Invalid(_) => "invalid".to_string(),
                                    LinkCheckResult::Redirect(_) => "redirect".to_string(),
                                    LinkCheckResult::GitHubFileMoved(_) => "file_moved".to_string(),
                                },
                                message: match result {
                                    LinkCheckResult::Valid => None,
                                    LinkCheckResult::Invalid(msg) => Some(msg),
                                    LinkCheckResult::Redirect(url) => {
                                        Some(format!("Redirected to: {url}"))
                                    }
                                    LinkCheckResult::GitHubFileMoved(msg) => {
                                        Some(format!("Moved to: {msg}"))
                                    }
                                },
                            };

                            match Event::default().json_data(event) {
                                Ok(event) => Ok(event),
                                Err(e) => {
                                    error!("Failed to serialize event: {e}");
                                    Ok(Event::default()
                                        .data(format!("Error serializing event: {e}")))
                                }
                            }
                        }
                    }
                })
                .buffer_unordered(10)
                .chain(stream::once(async move {
                    let counters = Arc::clone(&counters);
                    let summary = counters.to_summary();

                    match Event::default().json_data(summary) {
                        Ok(event) => Ok(event),
                        Err(e) => {
                            error!("Failed to serialize summary event: {e}");
                            Ok(Event::default().data(format!("Error serializing summary: {e}")))
                        }
                    }
                }));

            Box::pin(events_stream) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>
        }
        Err(e) => {
            error!("Error processing repository: {}", e);
            let error_event = Event::default().data(e.to_string());
            Box::pin(stream::iter(vec![Ok(error_event)]))
                as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use futures::StreamExt;
    use serde_json::Value;

    #[tokio::test]
    async fn test_stream_link_checks() {
        let repo_url = "https://github.com/reddevilmidzy/kingsac".to_string();
        let branch = Some("main".to_string());
        let sse = stream_link_checks(repo_url, branch).await;
        let mut stream = sse.into_response().into_body().into_data_stream();

        // 스트림에서 이벤트를 수집
        let mut events = Vec::new();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            if let Ok(chunk) = chunk {
                if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                    buffer.push_str(&text);

                    if let Some(event_end) = buffer.find("\n\n") {
                        let event_str = buffer[..event_end].to_string();
                        buffer = buffer[event_end + 2..].to_string();
                        // "data: " 접두사를 제거하고 JSON 파싱
                        if let Some(json_str) = event_str.strip_prefix("data: ") {
                            if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                                events.push(json);
                            }
                        }
                    }
                }
            }
        }

        assert!(!events.is_empty(), "No events were received");

        assert!(events.last().is_some(), "Last event should exist");
        if let Some(last_event) = events.last() {
            assert!(last_event.get("total").is_some());
            assert!(last_event.get("total").unwrap().as_u64().unwrap() > 0);
            assert!(last_event.get("valid").is_some());
            assert!(last_event.get("invalid").is_some());
            assert!(last_event.get("redirect").is_some());
            assert!(last_event.get("moved").is_some());
        }

        assert!(events.first().is_some(), "First event should exist");
        if let Some(first_event) = events.first() {
            assert!(first_event.get("url").is_some());
            assert!(first_event.get("file_path").is_some());
            assert!(first_event.get("line_number").is_some());
            assert!(first_event.get("status").is_some());
        }
    }
}
