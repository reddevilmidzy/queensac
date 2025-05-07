use regex::Regex;
use std::fs;
use std::path::Path;
use tokio::time;

fn extract_links_from_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    let content = fs::read_to_string(&path).unwrap();
    let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();

    url_regex
        .find_iter(&content)
        .map(|mat| {
            let url = mat.as_str();
            url.trim_end_matches(&[')', '>', '.', ',', ';'][..])
                .to_string()
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
enum LinkCheckResult {
    Valid,
    Invalid(String),
}

async fn check_link(url: &str) -> LinkCheckResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let mut attempts = 3;
    while attempts > 0 {
        match client.get(url).send().await {
            Ok(res) => {
                let status = res.status();
                return if status.is_success() || status.is_redirection() {
                    LinkCheckResult::Valid
                } else {
                    LinkCheckResult::Invalid(format!("HTTP status code: {}", status))
                };
            }
            Err(e) => {
                if attempts == 1 {
                    return LinkCheckResult::Invalid(format!("Request error: {}", e));
                }
            }
        }
        attempts -= 1;
        time::sleep(time::Duration::from_secs(1)).await;
    }
    LinkCheckResult::Invalid("Max retries exceeded".to_string())
}

#[tokio::main]
async fn main() {
    let file_path = "example.md";
    let links = extract_links_from_file(file_path);

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

    println!("Sacrifice THE QUEEN!!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn extracts_links_from_file() -> Result<(), std::io::Error> {
        // 테스트용 파일 생성
        let test_file_path = "test_file.txt";
        let mut file = File::create(test_file_path)?;

        writeln!(
            file,
            "Visit https://example.com and https://rust-lang.org for more info."
        )?;

        // 함수 호출 및 결과 확인
        let links = extract_links_from_file(test_file_path);
        assert_eq!(
            links,
            vec![
                "https://example.com".to_string(),
                "https://rust-lang.org".to_string()
            ]
        );

        // 테스트 후 파일 삭제
        fs::remove_file(test_file_path)?;
        Ok(())
    }

    #[tokio::test]
    async fn validate_link() {
        let link = "https://redddy.com";
        assert!(matches!(
            check_link(link).await,
            LinkCheckResult::Invalid(_)
        ));
        let link = "https://lazypazy.tistory.com";
        assert_eq!(check_link(link).await, LinkCheckResult::Valid);
    }
}
