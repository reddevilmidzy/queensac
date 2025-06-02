use crate::git;
use crate::link_checker::link::{LinkCheckResult, check_link};

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::StreamExt;
use tracing::{error, info, instrument};

#[derive(Debug, serde::Serialize)]
struct LinkCheckEvent {
    url: String,
    file_path: String,
    line_number: u32,
    status: String,
    message: Option<String>,
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

            let links_stream = stream::iter(links);
            let events_stream = links_stream.then(move |link| async move {
                let result = check_link(&link.url).await;
                let event = LinkCheckEvent {
                    url: link.url,
                    file_path: link.file_path,
                    line_number: link.line_number as u32,
                    status: match &result {
                        LinkCheckResult::Valid => "valid".to_string(),
                        LinkCheckResult::Invalid(_) => "invalid".to_string(),
                        LinkCheckResult::Redirect(_) => "redirect".to_string(),
                    },
                    message: match result {
                        LinkCheckResult::Valid => None,
                        LinkCheckResult::Invalid(msg) => Some(msg),
                        LinkCheckResult::Redirect(url) => Some(format!("Redirected to: {}", url)),
                    },
                };
                Ok(Event::default().json_data(event).unwrap())
            });
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
