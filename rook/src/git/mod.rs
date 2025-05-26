use git2::Repository;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

const REGEX_URL: &str = r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)";

/// Generate a unique directory name using repo owner and name
fn generate_dir_name(repo_url: &str, branch: Option<&str>) -> String {
    let parts: Vec<&str> = repo_url
        .trim_start_matches("https://github.com/")
        .split('/')
        .collect();
    let (user_name, repo_name) = (parts[0], parts[1]);
    format!(
        "queensac_temp_repo/{}/{}/{}",
        user_name,
        repo_name,
        branch.unwrap_or("default")
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Represents a hyperlink found in a repository, along with its location.
pub struct LinkInfo {
    /// The URL string. This should be a valid HTTP or HTTPS URL.
    pub url: String,
    /// The relative file path where the URL was found.
    pub file_path: String,
    /// The 1-based line number in the file where the URL was found.
    pub line_number: usize,
}

impl PartialEq for LinkInfo {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for LinkInfo {}

impl std::hash::Hash for LinkInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.url.hash(state);
    }
}

/// Checkout a specific branch in the repository
fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<(), git2::Error> {
    let remote_branch = format!("origin/{}", branch_name);
    let reference = repo
        .find_reference(&format!("refs/remotes/{}", remote_branch))
        .map_err(|_| git2::Error::from_str(&format!("Branch not found: {}", branch_name)))?;

    let commit = reference.peel_to_commit()?;
    repo.checkout_tree(commit.as_object(), None)?;
    repo.set_head(&format!("refs/heads/{}", branch_name))?;
    Ok(())
}

pub fn extract_links_from_repo_url(
    repo_url: &str,
    branch: Option<String>,
) -> Result<HashSet<LinkInfo>, git2::Error> {
    let temp_dir = env::temp_dir().join(generate_dir_name(repo_url, branch.as_deref()));
    let _temp_dir_guard = TempDirGuard::new(temp_dir.clone()).map_err(|e| {
        git2::Error::from_str(&format!("Failed to create temporary directory: {}", e))
    })?;

    // Clone repository
    let repo = Repository::clone(repo_url, &temp_dir)?;

    // 브랜치가 지정된 경우에만 fetch와 체크아웃 수행
    if let Some(branch_name) = branch {
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
        checkout_branch(&repo, &branch_name)?;
    }

    let mut all_links = HashSet::new();

    if let Ok(head) = repo.head() {
        if let Ok(tree) = head.peel_to_tree() {
            tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if let Some(name) = entry.name() {
                    let file_path = if dir.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}/{}", dir, name)
                    };

                    if let Ok(blob) = entry.to_object(&repo) {
                        if let Ok(blob) = blob.peel_to_blob() {
                            if let Ok(content) = String::from_utf8(blob.content().to_vec()) {
                                let links = find_link_in_content(&content, file_path.clone());
                                all_links.extend(links);
                            }
                        }
                    }
                }
                git2::TreeWalkResult::Ok
            })?;
        }
    }

    Ok(all_links)
}

fn find_link_in_content(content: &str, file_path: String) -> HashSet<LinkInfo> {
    let url_regex = Regex::new(REGEX_URL).unwrap();
    let mut result = HashSet::new();

    for (line_num, line) in content.lines().enumerate() {
        for mat in url_regex.find_iter(line) {
            let url = mat
                .as_str()
                .trim_end_matches(&[')', '>', '.', ',', ';'][..])
                .to_string();

            result.insert(LinkInfo {
                url,
                file_path: file_path.clone(),
                line_number: line_num + 1,
            });
        }
    }
    result
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_extract_links_from_repo_url() -> Result<(), Box<dyn std::error::Error>> {
        let repo_url = "https://github.com/reddevilmidzy/redddy-action";

        let links = extract_links_from_repo_url(repo_url, None)?;

        assert!(!links.is_empty(), "No links found in the repository");

        let url_regex = Regex::new(REGEX_URL).unwrap();
        for link in &links {
            assert!(
                url_regex.is_match(&link.url),
                "Invalid URL found: {} at {}:{}",
                link.url,
                link.file_path,
                link.line_number
            );
        }

        Ok(())
    }

    #[test]
    #[serial]
    fn test_find_link_in_content_duplicates() {
        let content = r#"
        https://example.com
        https://example.com
        https://example.com/path
        https://example.com/path
        https://example.com/path?param=value
        "#;

        let file_path = "test.txt".to_string();
        let links = find_link_in_content(content, file_path);

        // Should have exactly 3 unique URLs
        assert_eq!(links.len(), 3, "Expected 3 unique URLs");

        // Verify each URL exists
        let urls: Vec<String> = links.iter().map(|link| link.url.clone()).collect();
        assert!(urls.contains(&"https://example.com".to_string()));
        assert!(urls.contains(&"https://example.com/path".to_string()));
        assert!(urls.contains(&"https://example.com/path?param=value".to_string()));

        // Verify line numbers are correct
        for link in links {
            assert!(
                link.line_number >= 2 && link.line_number <= 6,
                "Line number should be between 2 and 6, got {}",
                link.line_number
            );
        }
    }

    #[test]
    fn test_link_info_uniqueness() {
        let mut links = HashSet::new();

        // Same URL, different file paths and line numbers
        let link1 = LinkInfo {
            url: "https://example.com".to_string(),
            file_path: "file1.txt".to_string(),
            line_number: 1,
        };

        let link2 = LinkInfo {
            url: "https://example.com".to_string(),
            file_path: "file2.txt".to_string(),
            line_number: 2,
        };

        links.insert(link1);
        links.insert(link2);

        // Should only have one entry because URLs are the same
        assert_eq!(links.len(), 1, "Expected only one unique URL entry");

        // Different URL
        let link3 = LinkInfo {
            url: "https://example.org".to_string(),
            file_path: "file1.txt".to_string(),
            line_number: 1,
        };

        links.insert(link3);

        // Should now have two entries because URLs are different
        assert_eq!(links.len(), 2, "Expected two unique URL entries");
    }

    #[test]
    #[serial]
    fn test_branch_found() {
        let repo_url = "https://github.com/reddevilmidzy/riir_os";
        let branch = "a-freestanding-rust-binary";

        let result = extract_links_from_repo_url(repo_url, Some(branch.to_string()));

        assert!(result.is_ok(), "Expected branch to be found");
    }

    #[test]
    #[serial]
    fn test_branch_not_found() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let non_existent_branch = "non-existent-branch";

        let result = extract_links_from_repo_url(repo_url, Some(non_existent_branch.to_string()));

        assert!(result.is_err(), "Expected error for non-existent branch");
        if let Err(e) = result {
            assert!(
                e.message().contains(non_existent_branch),
                "Error message should contain the branch name"
            );
        }
    }

    #[test]
    #[serial]
    fn test_checkout_branch_with_valid_branch() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let temp_dir = env::temp_dir().join("test_checkout_branch");
        let _temp_dir_guard = TempDirGuard::new(temp_dir.clone()).unwrap();

        // Clone repository
        let repo = Repository::clone(repo_url, &temp_dir).unwrap();

        // Fetch all branches
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
            .unwrap();

        // Test checkout with a known existing branch
        let result = checkout_branch(&repo, "main");
        assert!(
            result.is_ok(),
            "Should successfully checkout existing branch"
        );
    }

    #[test]
    #[serial]
    fn test_checkout_branch_with_invalid_branch() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let temp_dir = env::temp_dir().join("test_checkout_invalid_branch");
        let _temp_dir_guard = TempDirGuard::new(temp_dir.clone()).unwrap();

        // Clone repository
        let repo = Repository::clone(repo_url, &temp_dir).unwrap();

        // Fetch all branches
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
            .unwrap();

        // Test checkout with a non-existent branch
        let result = checkout_branch(&repo, "non-existent-branch");
        assert!(
            result.is_err(),
            "Should fail to checkout non-existent branch"
        );
        if let Err(e) = result {
            assert!(
                e.message().contains("non-existent-branch"),
                "Error message should contain the branch name"
            );
        }
    }
}
