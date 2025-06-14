use crate::git;
use crate::link::{LinkCheckResult, check_link};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RepoKey {
    repo_url: String,
    branch: Option<String>,
}

static REPO_TASKS: Lazy<Mutex<HashMap<RepoKey, CancellationToken>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// TODO 단순히 로그를 출력하여 링크 결과를 확인할 게 아니라 적절한 응답을 주도록 변경해야 함
#[instrument(skip(interval_duration), fields(repo_url = repo_url))]
pub async fn check_repository_links(
    repo_url: &str,
    branch: Option<String>,
    interval_duration: Duration,
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
                        let mut handles = Vec::new();
                        for link in links {
                            let handle = tokio::spawn(async move {
                                let result = check_link(&link.url).await;
                                (link, result)
                            });
                            handles.push(handle);
                        }
                        for handle in handles {
                            if let Ok((link, LinkCheckResult::Invalid(message))) = handle.await {
                                warn!(
                                    "Invalid link found: '{}' at {}:{}, reason: {}",
                                    link.url,
                                    link.file_path,
                                    link.line_number,
                                    message
                                );
                            }
                        }
                    }
                    Err(e) => error!("Error processing repository: {}", e),
                }
                info!(
                    "Link check completed for {} (branch: {:?}). Waiting for next interval...",
                    repo_url,
                    branch
                );
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
