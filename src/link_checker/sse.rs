use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
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

#[instrument(level = "info", skip_all, fields(repo_url = %repo_url, branch = ?branch))]
pub async fn stream_link_checks(
    repo_url: String,
    branch: Option<String>,
) -> Result<LinkCheckSummaryEvent, String> {
    info!(
        "Starting link checks for repository: {} (branch: {:?})",
        repo_url, branch
    );

    let result = git::extract_links_from_repo_url(&repo_url, branch.clone());
    let links = match result {
        Ok(links) => {
            info!("Found {} links to check", links.len());
            links
        }
        Err(e) => {
            error!("Error processing repository: {}", e);
            return Err(e.to_string());
        }
    };

    let counters = Arc::new(LinkCheckCounters::new());

    stream::iter(links)
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

                    let status = match &result {
                        LinkCheckResult::Valid => "valid",
                        LinkCheckResult::Invalid(_) => "invalid",
                        LinkCheckResult::Redirect(_) => "redirect",
                        LinkCheckResult::GitHubFileMoved(_) => "file_moved",
                    };

                    let message: Option<String> = match result {
                        LinkCheckResult::Valid => None,
                        LinkCheckResult::Invalid(msg) => Some(msg),
                        LinkCheckResult::Redirect(url) => Some(format!("Redirected to: {url}")),
                        LinkCheckResult::GitHubFileMoved(msg) => Some(format!("Moved to: {msg}")),
                    };

                    let message_str = message.as_deref().unwrap_or("");
                    info!(
                        url = %link.url,
                        file_path = %link.file_path,
                        line_number = link.line_number as u32,
                        status = %status,
                        message = %message_str,
                        "link check"
                    );
                }
            }
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    let summary = counters.to_summary();
    info!(
        total = summary.total,
        valid = summary.valid,
        invalid = summary.invalid,
        redirect = summary.redirect,
        moved = summary.moved,
        "link check summary"
    );
    return Ok(summary);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_link_checks_runs() {
        let repo_url = "https://github.com/reddevilmidzy/kingsac".to_string();
        let branch = Some("main".to_string());
        let summary = stream_link_checks(repo_url, branch).await;

        assert!(summary.is_ok());
        let summary = summary.unwrap();
        assert_eq!(
            summary.total,
            summary.valid + summary.invalid + summary.redirect + summary.moved
        );
    }
}
