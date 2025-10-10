use tracing::{error, info, instrument};

use crate::{LinkCheckResult, LinkChecker, RepoManager, git};

#[derive(Debug)]
pub struct LinkCheckEvent {
    pub url: String,
    pub file_path: String,
    pub line_number: u32,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug)]
pub struct LinkCheckSummaryEvent {
    pub total: usize,
    pub valid: usize,
    pub invalid: usize,
    pub redirect: usize,
    pub moved: usize,
}

#[derive(Debug, Clone)]
pub struct InvalidLinkInfo {
    pub url: String,
    pub file_path: String,
    pub line_number: usize,
}

#[derive(Debug)]
struct LinkCheckCounters {
    total: usize,
    valid: usize,
    invalid: usize,
    redirect: usize,
    moved: usize,
}

impl LinkCheckCounters {
    fn new() -> Self {
        Self {
            total: 0,
            valid: 0,
            invalid: 0,
            redirect: 0,
            moved: 0,
        }
    }

    fn increment_total(&mut self) {
        self.total += 1;
    }

    fn increment_valid(&mut self) {
        self.valid += 1;
    }

    fn increment_invalid(&mut self) {
        self.invalid += 1;
    }

    fn increment_redirect(&mut self) {
        self.redirect += 1;
    }

    fn increment_moved(&mut self) {
        self.moved += 1;
    }

    fn to_summary(&self) -> LinkCheckSummaryEvent {
        LinkCheckSummaryEvent {
            total: self.total,
            valid: self.valid,
            invalid: self.invalid,
            redirect: self.redirect,
            moved: self.moved,
        }
    }
}

/// Checks all links referenced in the given repository and returns details for any links that are not valid.
///
/// # Parameters
///
/// - `repo_manager`: A reference to the RepoManager instance containing the cloned repository to scan for links.
///
/// # Returns
///
/// `Result<Vec<InvalidLinkInfo>, String>` containing information about each link that is not valid (invalid, redirected, or moved). The vector is empty if all links are valid. Returns an error if there was a problem processing the repository.
///
/// # Examples
///
/// ```rust,no_run
/// #[tokio::test]
/// async fn example_check_links() {
///     let github_url = GitHubUrl::new("reddevilmidzy".to_string(), "kingsac".to_string(), Some("main".to_string()), None);
///     let repo_manager = RepoManager::from_github_url(&github_url).unwrap();
///     let invalid = check_links(&repo_manager).await.unwrap();
///     // `invalid` contains any links that failed validation
///     println!("Found {} invalid links", invalid.len());
/// }
/// ```
#[instrument(level = "info", skip_all)]
pub async fn check_links(repo_manager: &RepoManager) -> Result<Vec<InvalidLinkInfo>, String> {
    let result = git::extract_links_from_repo(repo_manager);
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

    let link_checker = LinkChecker::default();
    let mut counters = LinkCheckCounters::new();
    let mut invalid_links = Vec::new();

    for link in links {
        let result = link_checker.check_link(&link.url).await;

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

        let message: Option<String> = match &result {
            LinkCheckResult::Valid => None,
            LinkCheckResult::Invalid(msg) => Some(msg.clone()),
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

        if !matches!(result, LinkCheckResult::Valid) {
            invalid_links.push(InvalidLinkInfo {
                url: link.url,
                file_path: link.file_path,
                line_number: link.line_number,
            });
        }
    }

    let summary = counters.to_summary();
    info!(
        total = summary.total,
        valid = summary.valid,
        invalid = summary.invalid,
        redirect = summary.redirect,
        moved = summary.moved,
        "link check summary"
    );

    return Ok(invalid_links);
}

#[cfg(test)]
mod tests {
    use crate::GitHubUrl;

    use super::*;

    #[tokio::test]
    async fn test_stream_link_checks_runs() {
        let github_url = GitHubUrl::new(
            "reddevilmidzy".to_string(),
            "kingsac".to_string(),
            Some("main".to_string()),
            None,
        );
        let repo_manager = RepoManager::from(&github_url).unwrap();
        let invalid_links = check_links(&repo_manager).await;
        assert!(invalid_links.is_ok());
        let invalid_links = invalid_links.unwrap();
        assert_eq!(invalid_links.len(), 1);
    }
}
