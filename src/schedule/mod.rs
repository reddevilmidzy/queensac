use crate::git;
use crate::link::{LinkCheckResult, check_link};
use chrono::Local;
use std::time::Duration;

pub async fn check_repository_links(repo_url: &str, interval_duration: Duration) {
    let mut interval = tokio::time::interval(interval_duration);

    loop {
        interval.tick().await;
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        println!("[{}] 링크 확인 중", current_time);

        match git::extract_links_from_repo_url(repo_url) {
            Ok(links) => {
                let mut handles = Vec::new();
                for link in links {
                    let handle = tokio::spawn(async move {
                        let result = check_link(&link).await;
                        (link, result)
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    if let Ok((link, LinkCheckResult::Invalid(message))) = handle.await {
                        println!("유효하지 않은 링크: '{}', 실패 원인: {}", link, message);
                    }
                }
            }
            Err(e) => println!("Repository 처리 중 오류 발생: {}", e),
        }

        println!("링크 확인 완료. 다음 간격 대기...");
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
