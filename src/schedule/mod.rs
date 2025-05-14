// TODO: Review the usage of the 'link' and 'git' modules in this file.
//       Verify if their usage aligns with the intended design and functionality.
//       If necessary, refer to issue #123 for further context and discussion.
use crate::git;
use crate::link::{LinkCheckResult, check_link};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

static REPO_TASKS: Lazy<Mutex<HashMap<String, CancellationToken>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[instrument(skip(interval_duration), fields(repo_url = repo_url))]
pub async fn check_repository_links(repo_url: &str, interval_duration: Duration) {
    let token = CancellationToken::new();
    {
        let mut map = REPO_TASKS.lock().unwrap();
        map.insert(repo_url.to_string(), token.clone());
    }
    info!("Starting repository link checker for {}", repo_url);

    let mut interval = tokio::time::interval(interval_duration);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!("Checking links for repository: {}", repo_url);

                match git::extract_links_from_repo_url(repo_url) {
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
                info!("Link check completed for {}. Waiting for next interval...", repo_url);
            },
            _ = token.cancelled() => {
                info!("Repository checker cancelled for: {}", repo_url);
                break;
            }
        }
    }
    let mut map = REPO_TASKS.lock().unwrap();
    map.remove(repo_url);
    info!("Repository checker cleanup completed for: {}", repo_url);
}

// FIXME: 지금 리턴 타입이 없는데, Result를 반환해는게 나은가. 지금은 그냥 로그로 출력하고 있음.
#[instrument(skip(), fields(repo_url = repo_url))]
pub async fn cancel_repository_checker(repo_url: &str) {
    let token = {
        let mut map = REPO_TASKS.lock().unwrap();
        map.remove(repo_url)
    };
    if let Some(token) = token {
        token.cancel();
        info!("Cancellation requested for repository: {}", repo_url);
    } else {
        warn!("No active checker found for repository: {}", repo_url);
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
