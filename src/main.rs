use git2::Repository;
use regex::Regex;
use std::env;
use std::fs;
use tokio::time;

struct TempDirGuard {
    path: std::path::PathBuf,
}

impl TempDirGuard {
    fn new(path: std::path::PathBuf) -> Result<Self, std::io::Error> {
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn extract_links_from_repo_url(repo_url: &str) -> Result<Vec<String>, git2::Error> {
    let temp_dir = env::temp_dir().join("queensac_temp_repo");
    let _temp_dir_guard = TempDirGuard::new(temp_dir.clone()).map_err(|e| {
        git2::Error::from_str(&format!("Failed to create temporary directory: {}", e))
    })?;
    let repo = Repository::clone(repo_url, &temp_dir)?;

    let mut all_links = Vec::new();
    let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();

    if let Ok(head) = repo.head() {
        if let Ok(tree) = head.peel_to_tree() {
            tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
                if let Some(_) = entry.name() {
                    if let Ok(blob) = entry.to_object(&repo) {
                        if let Ok(blob) = blob.peel_to_blob() {
                            if let Ok(content) = String::from_utf8(blob.content().to_vec()) {
                                all_links.extend(url_regex.find_iter(&content).map(|mat| {
                                    let url = mat.as_str();
                                    url.trim_end_matches(&[')', '>', '.', ',', ';'][..])
                                        .to_string()
                                }));
                            }
                        }
                    }
                }
                git2::TreeWalkResult::Ok
            })?;
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);

    Ok(all_links)
}

#[derive(Debug, Eq, PartialEq)]
enum LinkCheckResult {
    Valid,
    Invalid(String),
}

async fn check_link(url: &str) -> LinkCheckResult {
    let client = reqwest::Client::builder()
        .timeout(time::Duration::from_secs(10))
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
    let links =
        extract_links_from_repo_url("https://github.com/reddevilmidzy/redddy-action").unwrap();

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
    fn test_extract_links_from_repo_url() -> Result<(), Box<dyn std::error::Error>> {
        let repo_url = "https://github.com/reddevilmidzy/redddy-action";

        let links = extract_links_from_repo_url(repo_url)?;

        assert!(!links.is_empty(), "No links found in the repository");

        let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();
        for link in &links {
            assert!(url_regex.is_match(link), "Invalid URL found: {}", link);
        }

        Ok(())
    }
}
