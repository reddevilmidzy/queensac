use regex::Regex;
use std::fs;
use std::path::Path;
use tokio::time;

fn process_path(path: &Path, url_regex: &Regex) -> Vec<String> {
    let mut links = Vec::new();
    
    if path.is_file() {
        if let Ok(content) = fs::read_to_string(path) {
            links.extend(
                url_regex
                    .find_iter(&content)
                    .map(|mat| {
                        mat.as_str()
                            .trim_end_matches(&[')', '>', '.', ',', ';'][..])
                            .to_string()
                    })
            );
        }
    } else if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                links.extend(process_path(&entry.path(), url_regex));
            }
        }
    }
    
    links
}

fn extract_links_from_path<P: AsRef<Path>>(path: P) -> Vec<String> {
    let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();
    process_path(path.as_ref(), &url_regex)
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
    let links = extract_links_from_path(file_path);

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
    use std::fs::{self, File};
    use std::io::Write;

    #[test]
    fn test_extracts_links_from_file() -> Result<(), std::io::Error> {
        // 테스트용 파일 생성
        let test_file_path = "test_file.txt";
        let mut file = File::create(test_file_path)?;

        writeln!(
            file,
            "Visit https://example.com and https://rust-lang.org for more info."
        )?;

        // 함수 호출 및 결과 확인
        let links = extract_links_from_path(test_file_path);
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

    #[test]
    fn test_process_path() -> Result<(), std::io::Error> {
        let test_dir = "test_dir";
        if Path::new(test_dir).exists() {
            fs::remove_dir_all(test_dir)?;
        }
        
        fs::create_dir_all(test_dir)?;
        
        let subdir = format!("{}/subdir", test_dir);
        fs::create_dir(&subdir)?;
        
        let mut file1 = File::create(format!("{}/file1.txt", test_dir))?;
        writeln!(file1, "Visit https://example1.com for more info.")?;
        
        let mut file2 = File::create(format!("{}/file2.txt", subdir))?;
        writeln!(file2, "Check https://example2.com and https://example3.com")?;
        
        let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();
        
        let links = process_path(Path::new(test_dir), &url_regex);
        
        fs::remove_dir_all(test_dir)?;
        
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"https://example1.com".to_string()));
        assert!(links.contains(&"https://example2.com".to_string()));
        assert!(links.contains(&"https://example3.com".to_string()));
        
        Ok(())
    }
}
