use crate::{GitHubUrl, RepoManager};
use url::Url;

pub struct LinkChecker {
    client: reqwest::Client,
}

impl LinkChecker {
    /// Creates a `LinkChecker` with an HTTP client configured to use a 5-second timeout and no redirects.
    ///
    /// # Returns
    ///
    /// `Ok(LinkChecker)` with the configured `reqwest::Client`, or `Err(reqwest::Error)` if building the client fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use queensac::LinkChecker;
    ///
    /// let checker = LinkChecker::new().expect("failed to build LinkChecker");
    /// ```
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        Ok(LinkChecker { client })
    }

    /// Checks a URL and classifies its link status.
    ///
    /// Sends an HTTP GET to the given URL (with internal retrying) and returns whether the link is valid,
    /// redirects (with the redirect target), is invalid (with a short reason), or indicates a GitHub file
    /// move discovered from a 404 on github.com.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use queensac::{LinkChecker, LinkCheckResult};
    ///
    /// // Example requires a runtime; create one and run the async call.
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let checker = LinkChecker::default();
    ///     let result = checker.check_link("https://example.com").await;
    ///     match result {
    ///         LinkCheckResult::Valid => println!("valid"),
    ///         LinkCheckResult::Redirect(target) => println!("redirect -> {}", target),
    ///         LinkCheckResult::Invalid(reason) => println!("invalid: {}", reason),
    ///         LinkCheckResult::GitHubFileMoved(new_path) => println!("moved: {}", new_path),
    ///     }
    /// });
    /// ```
    ///
    /// # Returns
    ///
    /// `LinkCheckResult` indicating the check outcome:
    /// - `Valid` if the URL resolves successfully or only performs a trivial redirect,
    /// - `Redirect(String)` with the redirect target for nontrivial redirects,
    /// - `Invalid(String)` with a brief diagnostic message for HTTP errors, request failures, or retry exhaustion,
    /// - `GitHubFileMoved(String)` when a GitHub 404 is resolved to a new file location discovered in the repository.
    pub async fn check_link(&self, url: &str) -> LinkCheckResult {
        let mut attempts = 3;
        while attempts > 0 {
            match self.client.get(url).send().await {
                Ok(res) => {
                    let status = res.status();
                    if status.is_success() {
                        return LinkCheckResult::Valid;
                    } else if status.is_redirection() {
                        if let Some(redirect_url) = res.headers().get("location")
                            && let Ok(redirect_str) = redirect_url.to_str()
                        {
                            if is_trivial_redirect(url, redirect_str) {
                                return LinkCheckResult::Valid;
                            }
                            return LinkCheckResult::Redirect(redirect_str.to_string());
                        }
                        return LinkCheckResult::Valid;
                    } else if status.as_u16() == 404 && is_github_url(url) {
                        return handle_github_404(url);
                    } else {
                        return LinkCheckResult::Invalid(format!("HTTP status code: {status}"));
                    }
                }
                Err(e) => {
                    if attempts == 1 {
                        return LinkCheckResult::Invalid(format!("Request error: {e}"));
                    }
                }
            }
            attempts -= 1;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        LinkCheckResult::Invalid("Max retries exceeded".to_string())
    }
}

impl Default for LinkChecker {
    /// Creates a default LinkChecker configured with a ready-to-use HTTP client.
    ///
    /// Panics if the internal HTTP client cannot be constructed.
    ///
    /// # Examples
    ///
    /// ```
    /// use queensac::LinkChecker;
    ///
    /// let checker = LinkChecker::default();
    /// // `checker` is ready to use for link checks.
    /// ```
    fn default() -> Self {
        Self::new().expect("failed to create LinkChecker client")
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum LinkCheckResult {
    Valid,
    Redirect(String),
    Invalid(String),
    GitHubFileMoved(String),
}

fn is_github_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h == "github.com" || h.ends_with(".github.com"))
        })
        .unwrap_or(false)
}

/// Attempts to resolve a GitHub 404 by locating the file's current path in the repository.
///
/// Parses the provided GitHub URL, clones or accesses the repository, and searches for the file's current location.
///
/// # Parameters
///
/// - `url`: The GitHub URL (file path) that returned a 404.
///
/// # Returns
///
/// - `LinkCheckResult::GitHubFileMoved(new_path)` if the file was found at a new path inside the repository.
/// - `LinkCheckResult::Invalid(...)` with a descriptive message if the URL is not a valid GitHub URL, the repository could not be accessed or cloned, the file does not exist in the repository, or an error occurred while searching.
fn handle_github_404(url: &str) -> LinkCheckResult {
    let parsed = match GitHubUrl::parse(url) {
        Some(parsed) => parsed,
        None => {
            return LinkCheckResult::Invalid(format!("Invalid GitHub URL format: {url}"));
        }
    };

    let repo_manager = match RepoManager::from_github_url(&parsed) {
        Ok(repo_manager) => repo_manager,
        Err(e) => {
            return LinkCheckResult::Invalid(format!("Error cloning repository: {e}"));
        }
    };

    match repo_manager.find_current_location(&parsed) {
        Ok(Some(new_path)) => LinkCheckResult::GitHubFileMoved(new_path.to_string()),
        Ok(None) => LinkCheckResult::Invalid(format!("File not found in repository: {url}")),
        Err(e) => LinkCheckResult::Invalid(format!("Error finding file location: {e}")),
    }
}

/// Determines whether a redirect URL is a trivial change from the original URL.
///
/// A trivial redirect preserves scheme, host, port, and query, and differs only by an
/// optional trailing slash on the path.
///
/// # Returns
///
/// `true` if the redirect is trivial (same scheme, host, port, and query, and the path differs only by a trailing slash), `false` otherwise.
fn is_trivial_redirect(original: &str, redirect: &str) -> bool {
    let orig_url = match Url::parse(original) {
        Ok(url) => url,
        Err(_) => return false,
    };

    let redirect_url = match Url::parse(redirect) {
        Ok(url) => url,
        Err(_) => return false,
    };

    if orig_url.scheme() != redirect_url.scheme()
        || orig_url.host() != redirect_url.host()
        || orig_url.port() != redirect_url.port()
    {
        return false;
    }

    if orig_url.query() != redirect_url.query() {
        return false;
    }

    let orig_path = orig_url.path();
    let redirect_path = redirect_url.path();

    redirect_path == format!("{}/", orig_path.trim_end_matches('/'))
        || redirect_path == orig_path.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_link() {
        let link_checker = LinkChecker::default();
        let link = "https://redddy.ai";
        assert!(matches!(
            link_checker.check_link(link).await,
            LinkCheckResult::Invalid(_)
        ));
        let link = "https://lazypazy.tistory.com";
        assert_eq!(link_checker.check_link(link).await, LinkCheckResult::Valid);
    }

    #[tokio::test]
    async fn change_organization_name() {
        let link_checker = LinkChecker::default();
        let link = "https://github.com/Bibimbap-Team/git-playground";
        assert_eq!(
            link_checker.check_link(link).await,
            LinkCheckResult::Redirect("https://github.com/Coduck-Team/git-playground".to_string())
        );
    }

    #[tokio::test]
    async fn change_branch_name() {
        let link_checker = LinkChecker::default();
        let link = "https://github.com/reddevilmidzy/kingsac/tree/forever";
        assert_eq!(
            link_checker.check_link(link).await,
            LinkCheckResult::Redirect(
                "https://github.com/reddevilmidzy/kingsac/tree/lie".to_string()
            )
        );
    }

    #[tokio::test]
    async fn change_repository_name() {
        let link_checker = LinkChecker::default();
        let link = "https://github.com/reddevilmidzy/test-queensac";
        assert_eq!(
            link_checker.check_link(link).await,
            LinkCheckResult::Redirect("https://github.com/reddevilmidzy/kingsac".to_string())
        );
    }

    #[tokio::test]
    async fn check_redirect_url() {
        let link_checker = LinkChecker::default();
        let link = "https://gluesql.org/docs";
        assert_eq!(
            link_checker.check_link(link).await,
            LinkCheckResult::Valid,
            "check trivial redirect"
        );
        let link = "https://gluesql.org/docs/";
        assert_eq!(
            link_checker.check_link(link).await,
            LinkCheckResult::Valid,
            "check trivial redirect"
        );
    }

    #[test]
    fn test_is_trivial_redirect() {
        // Basic trailing slash cases
        assert!(is_trivial_redirect(
            "https://example.com",
            "https://example.com/"
        ));
        assert!(is_trivial_redirect(
            "https://example.com/",
            "https://example.com"
        ));

        // Multiple trailing slashes should not be considered trivial
        assert!(!is_trivial_redirect(
            "https://example.com",
            "https://example.com//"
        ));

        // Different paths should not be trivial
        assert!(!is_trivial_redirect(
            "https://example.com/docs",
            "https://example.com/guides"
        ));

        // Query parameters should not affect trivial redirect detection
        assert!(is_trivial_redirect(
            "https://example.com?q=test",
            "https://example.com/?q=test"
        ));

        // Different query parameters should not be trivial
        assert!(!is_trivial_redirect(
            "https://example.com/?q=test",
            "https://example.com/?q=test&p=1"
        ));

        // Different ports should not be trivial
        assert!(!is_trivial_redirect(
            "https://example.com:8080",
            "https://example.com:8081"
        ));
    }

    #[test]
    fn test_is_github_url() {
        // GitHub URLs should be detected correctly
        assert!(is_github_url("https://github.com/reddevilmidzy/queensac"));
        assert!(is_github_url(
            "https://github.com/reddevilmidzy/queensac/blob/main/src/main.rs"
        ));
    }

    #[test]
    fn test_is_not_github_url() {
        // GitHub URLs should not be detected incorrectly
        assert!(!is_github_url("https://example.com"));
        assert!(!is_github_url("https://example.com/docs"));
        assert!(!is_github_url("https://notgithub.com"))
    }
}
