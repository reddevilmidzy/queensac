use crate::{GitHubUrl, RepoManager};
use url::Url;

pub struct LinkChecker {
    client: reqwest::Client,
}

impl LinkChecker {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build();

        if let Ok(client) = client {
            Ok(LinkChecker { client })
        } else {
            Err("failed to create Client".to_string())
        }
    }

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
                    } else if status.as_u16() == 404 && url.contains("github.com") {
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
    fn default() -> Self {
        Self::new().expect("failed to create LinkChecker")
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum LinkCheckResult {
    Valid,
    Redirect(String),
    Invalid(String),
    GitHubFileMoved(String),
}

/// Handles GitHub 404 errors by attempting to find the current file location
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
}
