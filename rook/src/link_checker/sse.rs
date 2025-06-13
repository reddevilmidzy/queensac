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

            let total = Arc::new(AtomicUsize::new(0));
            let valid = Arc::new(AtomicUsize::new(0));
            let invalid = Arc::new(AtomicUsize::new(0));
            let redirect = Arc::new(AtomicUsize::new(0));
            let moved = Arc::new(AtomicUsize::new(0));

            let links_stream = stream::iter(links);
            let events_stream = links_stream
                .map({
                    let total = Arc::clone(&total);
                    let valid = Arc::clone(&valid);
                    let invalid = Arc::clone(&invalid);
                    let redirect = Arc::clone(&redirect);
                    let moved = Arc::clone(&moved);
                    move |link| {
                        let total = Arc::clone(&total);
                        let valid = Arc::clone(&valid);
                        let invalid = Arc::clone(&invalid);
                        let redirect = Arc::clone(&redirect);
                        let moved = Arc::clone(&moved);

                        async move {
                            let result = check_link(&link.url).await;

                            // 카운터 업데이트
                            total.fetch_add(1, Ordering::Relaxed);
                            match &result {
                                LinkCheckResult::Valid => valid.fetch_add(1, Ordering::Relaxed),
                                LinkCheckResult::Invalid(_) => {
                                    invalid.fetch_add(1, Ordering::Relaxed)
                                }
                                LinkCheckResult::Redirect(_) => {
                                    redirect.fetch_add(1, Ordering::Relaxed)
                                }
                                LinkCheckResult::GitHubFileMoved(_) => {
                                    moved.fetch_add(1, Ordering::Relaxed)
                                }
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
                                        Some(format!("Redirected to: {}", url))
                                    }
                                    LinkCheckResult::GitHubFileMoved(msg) => {
                                        Some(format!("Moved to: {}", msg))
                                    }
                                },
                            };

                            match Event::default().json_data(event) {
                                Ok(event) => Ok(event),
                                Err(e) => {
                                    error!("Failed to serialize event: {}", e);
                                    Ok(Event::default()
                                        .data(format!("Error serializing event: {}", e)))
                                }
                            }
                        }
                    }
                })
                .buffer_unordered(10)
                .chain(stream::once(async move {
                    let total = Arc::clone(&total);
                    let valid = Arc::clone(&valid);
                    let invalid = Arc::clone(&invalid);
                    let redirect = Arc::clone(&redirect);
                    let moved = Arc::clone(&moved);

                    let summary = LinkCheckSummaryEvent {
                        total: total.load(Ordering::Relaxed),
                        valid: valid.load(Ordering::Relaxed),
                        invalid: invalid.load(Ordering::Relaxed),
                        redirect: redirect.load(Ordering::Relaxed),
                        moved: moved.load(Ordering::Relaxed),
                    };

                    match Event::default().json_data(summary) {
                        Ok(event) => Ok(event),
                        Err(e) => {
                            error!("Failed to serialize summary event: {}", e);
                            Ok(Event::default().data(format!("Error serializing summary: {}", e)))
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
