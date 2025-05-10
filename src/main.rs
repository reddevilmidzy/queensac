mod git;
mod link;

use link::{LinkCheckResult, check_link};
use std::time::Duration;
use chrono::Local;

#[tokio::main]
async fn main() {
    let repo_url = "https://github.com/reddevilmidzy/redddy-action";
    let mut interval = tokio::time::interval(Duration::from_secs(60)); // Run every 5 minutes

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
