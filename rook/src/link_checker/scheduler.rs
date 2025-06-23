use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::git;
use crate::link::{LinkCheckResult, check_link};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RepoKey {
    repo_url: String,
    branch: Option<String>,
}

static REPO_TASKS: Lazy<Mutex<HashMap<RepoKey, CancellationToken>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
struct InvalidLink {
    url: String,
    error_message: String,
}

#[derive(Debug)]
struct RedirectedLink {
    original_url: String,
    new_url: String,
}

#[derive(Debug)]
struct MovedLink {
    original_path: String,
    new_path: String,
}

#[derive(Debug)]
/// A summary of link checking results for a repository
///
/// This struct holds statistics and details about the link checking process:
/// - Total number of links checked
/// - Number of valid links found
/// - List of invalid links with error messages
/// - List of links that redirect to new URLs
/// - List of GitHub files that have moved to new paths
///
/// # Fields
///
/// * `total_links` - Total number of links checked in the repository
/// * `valid_links` - Number of links that were successfully validated
/// * `invalid_links` - Vector of InvalidLink structs containing invalid URLs and their error messages
/// * `redirected_links` - Vector of RedirectedLink structs containing original URLs and their redirect destinations
/// * `moved_links` - Vector of MovedLink structs containing original GitHub file paths and their new locations
struct LinkCheckSummary {
    total_links: usize,
    valid_links: usize,
    invalid_links: Vec<InvalidLink>,
    redirected_links: Vec<RedirectedLink>,
    moved_links: Vec<MovedLink>,
}

impl LinkCheckSummary {
    fn new() -> Self {
        Self {
            total_links: 0,
            valid_links: 0,
            invalid_links: Vec::new(),
            redirected_links: Vec::new(),
            moved_links: Vec::new(),
        }
    }

    fn add_result(&mut self, url: String, result: LinkCheckResult) {
        self.total_links += 1;
        match result {
            LinkCheckResult::Valid => {
                self.valid_links += 1;
            }
            LinkCheckResult::Invalid(error_msg) => {
                self.invalid_links.push(InvalidLink {
                    url,
                    error_message: error_msg,
                });
            }
            LinkCheckResult::Redirect(new_url) => {
                self.redirected_links.push(RedirectedLink {
                    original_url: url,
                    new_url,
                });
            }
            LinkCheckResult::GitHubFileMoved(new_path) => {
                self.moved_links.push(MovedLink {
                    original_path: url,
                    new_path,
                });
            }
        }
    }

    fn generate_email_content(&self, repo_url: &str, branch: Option<&str>) -> (String, String) {
        let branch_info = branch
            .map(|b| format!(" (branch: {})", b))
            .unwrap_or_default();
        let subject = format!("Link Check Report - {}{}", repo_url, branch_info);

        let mut html_content = format!(
            r#"<h2>Link Check Report</h2>
            <p><strong>Repository:</strong> {}{}</p>
            <p><strong>Total Links:</strong> {}</p>
            <p><strong>Valid Links:</strong> {}</p>
            <p><strong>Invalid Links:</strong> {}</p>
            <p><strong>Redirected Links:</strong> {}</p>
            <p><strong>Moved Files:</strong> {}</p>"#,
            repo_url,
            branch_info,
            self.total_links,
            self.valid_links,
            self.invalid_links.len(),
            self.redirected_links.len(),
            self.moved_links.len()
        );

        if !self.invalid_links.is_empty() {
            html_content.push_str("<h3>Invalid Links:</h3><ul>");
            for link in &self.invalid_links {
                html_content.push_str(&format!(
                    "<li><strong>{}</strong>: {}</li>",
                    link.url, link.error_message
                ));
            }
            html_content.push_str("</ul>");
        }

        if !self.redirected_links.is_empty() {
            html_content.push_str("<h3>Redirected Links:</h3><ul>");
            for link in &self.redirected_links {
                html_content.push_str(&format!(
                    "<li><strong>{}</strong> → <a href=\"{}\">{}</a></li>",
                    link.original_url, link.new_url, link.new_url
                ));
            }
            html_content.push_str("</ul>");
        }

        if !self.moved_links.is_empty() {
            html_content.push_str("<h3>Moved Files:</h3><ul>");
            for link in &self.moved_links {
                html_content.push_str(&format!(
                    "<li><strong>{}</strong> → <code>{}</code></li>",
                    link.original_path, link.new_path
                ));
            }
            html_content.push_str("</ul>");
        }

        (subject, html_content)
    }
}

#[instrument(skip(interval_duration, email_client, subscriber_email), fields(repo_url = repo_url))]
pub async fn check_repository_links(
    repo_url: &str,
    branch: Option<String>,
    interval_duration: Duration,
    email_client: &EmailClient,
    subscriber_email: SubscriberEmail,
) -> Result<(), String> {
    let repo_key = RepoKey {
        repo_url: repo_url.to_string(),
        branch: branch.clone(),
    };

    // Check if repository is already being monitored
    let token = {
        let mut map = REPO_TASKS.lock().unwrap();
        if map.contains_key(&repo_key) {
            return Err(format!(
                "Repository {} (branch: {:?}) is already being monitored",
                repo_url, branch
            ));
        }
        let token = CancellationToken::new();
        map.insert(repo_key.clone(), token.clone());
        token
    };

    info!(
        "Starting repository link checker for {} (branch: {:?})",
        repo_url, branch
    );

    let mut interval = tokio::time::interval(interval_duration);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!(
                    "Checking links for repository: {} (branch: {:?})",
                    repo_url,
                    branch
                );

                match git::extract_links_from_repo_url(repo_url, branch.clone()) {
                    Ok(links) => {
                        info!("Found {} links to check", links.len());

                        let mut summary = LinkCheckSummary::new();
                        let mut handles = Vec::new();

                        for link in links {
                            let handle = tokio::spawn(async move {
                                let result = check_link(&link.url).await;
                                (link.url, result)
                            });
                            handles.push(handle);
                        }

                        // Wait for all link checks to complete
                        for handle in handles {
                            if let Ok((url, result)) = handle.await {
                                summary.add_result(url, result);
                            }
                        }

                        // Send email report
                        let (subject, html_content) = summary.generate_email_content(repo_url, branch.as_deref());
                        if let Err(e) = email_client.send_email_with_retry(
                            subscriber_email.clone(),
                            subject,
                            html_content,
                            "broadcast".to_string(),
                            3,
                            Duration::from_secs(60),
                        ).await {
                            error!("Failed to send email report: {}", e);
                        } else {
                            info!("Email report sent successfully for {}", repo_url);
                        }
                    }
                    Err(e) => error!("Error processing repository: {}", e),
                }
            },
            _ = token.cancelled() => {
                info!(
                    "Repository checker cancelled for: {} (branch: {:?})",
                    repo_url,
                    branch
                );
                break;
            }
        }
    }

    Ok(())
}

#[instrument(skip(), fields(repo_url = repo_url))]
pub async fn cancel_repository_checker(
    repo_url: &str,
    branch: Option<String>,
) -> Result<(), String> {
    let repo_key = RepoKey {
        repo_url: repo_url.to_string(),
        branch: branch.clone(),
    };

    let token = {
        let mut map = REPO_TASKS.lock().unwrap();
        map.remove(&repo_key)
    };
    if let Some(token) = token {
        token.cancel();
        info!(
            "Cancellation requested for repository: {} (branch: {:?})",
            repo_url, branch
        );
        Ok(())
    } else {
        Err(format!(
            "No active checker found for repository: {} (branch: {:?})",
            repo_url, branch
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::timeout;

    #[test]
    fn test_link_check_summary() {
        let mut summary = LinkCheckSummary::new();

        // 초기 상태 확인
        assert_eq!(summary.total_links, 0);
        assert_eq!(summary.valid_links, 0);
        assert_eq!(summary.invalid_links.len(), 0);
        assert_eq!(summary.redirected_links.len(), 0);
        assert_eq!(summary.moved_links.len(), 0);

        // 다양한 결과 추가
        summary.add_result("https://example.com".to_string(), LinkCheckResult::Valid);
        summary.add_result(
            "https://invalid.com".to_string(),
            LinkCheckResult::Invalid("404 Not Found".to_string()),
        );
        summary.add_result(
            "https://redirect.com".to_string(),
            LinkCheckResult::Redirect("https://new-url.com".to_string()),
        );
        summary.add_result(
            "https://github.com/user/repo/blob/main/file.txt".to_string(),
            LinkCheckResult::GitHubFileMoved("new/path/file.txt".to_string()),
        );

        // 결과 확인
        assert_eq!(summary.total_links, 4);
        assert_eq!(summary.valid_links, 1);
        assert_eq!(summary.invalid_links.len(), 1);
        assert_eq!(summary.redirected_links.len(), 1);
        assert_eq!(summary.moved_links.len(), 1);

        // 이메일 내용 생성 테스트
        let (subject, html_content) =
            summary.generate_email_content("https://github.com/user/repo", Some("main"));

        assert!(subject.contains("Link Check Report"));
        assert!(subject.contains("https://github.com/user/repo"));
        assert!(subject.contains("(branch: main)"));
        assert!(html_content.contains("<p><strong>Total Links:</strong> 4</p>"));
        assert!(html_content.contains("<p><strong>Valid Links:</strong> 1</p>"));
        assert!(html_content.contains("<p><strong>Invalid Links:</strong> 1</p>"));
        assert!(html_content.contains("<p><strong>Redirected Links:</strong> 1</p>"));
        assert!(html_content.contains("<p><strong>Moved Files:</strong> 1</p>"));
        assert!(html_content.contains("https://invalid.com"));
        assert!(html_content.contains("404 Not Found"));
        assert!(html_content.contains("https://redirect.com"));
        assert!(html_content.contains("https://new-url.com"));
        assert!(html_content.contains("new/path/file.txt"));
    }

    #[tokio::test]
    async fn test_scheduled_execution() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        // 카운터를 증가시키는 check_repository_links의 mock 생성
        async fn mock_check_repository_links(_repo_url: &str, counter: Arc<AtomicUsize>) {
            counter.fetch_add(1, Ordering::SeqCst);
        }

        let repo_url = "https://github.com/test/repo";
        let interval = Duration::from_millis(100); // Use 100ms for faster testing

        // 스케줄링 작업 시작
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            for _ in 0..3 {
                // 3번 실행
                interval.tick().await;
                mock_check_repository_links(repo_url, counter_clone.clone()).await;
            }
        });

        // 모든 실행이 완료될 때까지 대기 (타임아웃 포함)
        let result = timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "테스트 타임아웃");

        // 함수가 정확히 3번 호출되었는지 확인
        assert_eq!(
            counter.load(Ordering::SeqCst),
            3,
            "함수가 정확히 3번 호출되지 않았습니다."
        );
    }

    #[tokio::test]
    async fn test_interval_timing() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let start_time = std::time::Instant::now();

        async fn mock_check_repository_links(_repo_url: &str, counter: Arc<AtomicUsize>) {
            counter.fetch_add(1, Ordering::SeqCst);
        }

        let repo_url = "https://github.com/test/repo";
        let interval = Duration::from_millis(100);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            for _ in 0..3 {
                interval.tick().await;
                mock_check_repository_links(repo_url, counter_clone.clone()).await;
            }
        });

        let result = timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "테스트 타임아웃");

        let elapsed = start_time.elapsed();
        // 일정한 타이밍 변경 허용 (100ms 간격으로 3번 실행하면 약 200ms ~ 400ms 사이)
        assert!(
            elapsed >= Duration::from_millis(200),
            "간격이 너무 짧습니다."
        );
        assert!(
            elapsed <= Duration::from_millis(400),
            "간격이 너무 길습니다."
        );
    }
}
