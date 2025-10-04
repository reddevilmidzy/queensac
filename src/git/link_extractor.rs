use regex::Regex;
use std::collections::HashSet;

use crate::RepoManager;

const REGEX_DOMAIN: &str = r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)";
const REGEX_IP_ADDRESS: &str = r"https?://(localhost|(?:\d{1,3}\.){3}\d{1,3})(?::\d+)?";

#[derive(Debug, Clone)]
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

pub fn extract_links_from_repo_url(
    repo_url: &str,
    branch: Option<String>,
) -> Result<HashSet<LinkInfo>, git2::Error> {
    let repo_manager = RepoManager::clone_repo(repo_url, branch.as_deref())?;

    let mut all_links = HashSet::new();
    if let Ok(head) = repo_manager.get_repo().head()
        && let Ok(tree) = head.peel_to_tree()
    {
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if let Some(name) = entry.name() {
                let file_path = if dir.is_empty() {
                    name.to_string()
                } else {
                    format!("{dir}/{name}")
                };

                if let Ok(blob) = entry.to_object(repo_manager.get_repo())
                    && let Ok(blob) = blob.peel_to_blob()
                    && let Ok(content) = String::from_utf8(blob.content().to_vec())
                {
                    let links = find_link_in_content(&content, file_path.clone());
                    all_links.extend(links);
                }
            }
            git2::TreeWalkResult::Ok
        })?;
    }

    Ok(all_links)
}

fn find_link_in_content(content: &str, file_path: String) -> HashSet<LinkInfo> {
    let domain_regex = Regex::new(REGEX_DOMAIN).unwrap();
    let ip_address_regex = Regex::new(REGEX_IP_ADDRESS).unwrap();
    let mut result = HashSet::new();

    for (line_num, line) in content.lines().enumerate() {
        for mat in domain_regex.find_iter(line) {
            if ip_address_regex.is_match(mat.as_str()) {
                continue;
            }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    static TEST_REPO_URL: &str = "https://github.com/reddevilmidzy/kingsac";

    #[test]
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
    fn test_skip_ip_addresses() {
        let content = r#"
        http://192.168.1.1
        http://192.168.1.1/path
        http://192.168.1.1/path?param=value
        this is localhost ip address http://127.0.0.1
        front server http://localhost:3000
        backend server http://localhost:8080
        "#;

        let file_path = "test.txt".to_string();
        let links = find_link_in_content(content, file_path);
        assert!(links.is_empty(), "Expected no links");
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
        let branch = "main";
        let result = extract_links_from_repo_url(TEST_REPO_URL, Some(branch.to_string()));

        assert!(result.is_ok(), "Expected branch to be found");
    }

    #[test]
    #[serial]
    fn test_branch_not_found() {
        let non_existent_branch = "non-existent-branch";

        let result =
            extract_links_from_repo_url(TEST_REPO_URL, Some(non_existent_branch.to_string()));

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
    fn test_extract_links_from_repo_url() -> Result<(), Box<dyn std::error::Error>> {
        let result = extract_links_from_repo_url(TEST_REPO_URL, None)?;

        assert!(!result.is_empty(), "No links found in the repository");

        let domain_regex = Regex::new(REGEX_DOMAIN).unwrap();
        for link in &result {
            assert!(
                domain_regex.is_match(&link.url),
                "Invalid URL found: {} at {}:{}",
                link.url,
                link.file_path,
                link.line_number
            );
        }

        Ok(())
    }
}
