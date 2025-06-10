use regex::Regex;
use tracing::error;

use crate::{RepoManager, file_exists_in_repo, find_last_commit_id, track_file_rename_in_commit};

/// Represents a parsed GitHub URL with its components
#[derive(Debug)]
pub struct GitHubUrl {
    /// The owner/organization name from the GitHub URL
    owner: String,
    /// The repository name from the GitHub URL
    repo: String,
    /// The branch name if specified in the URL (e.g. master, main)
    branch: Option<String>,
    /// The file path within the repository if specified in the URL
    file_path: Option<String>,
}

impl GitHubUrl {
    /// Parses a GitHub URL string into a GitHubUrl struct
    ///
    /// # Arguments
    /// * `url` - A GitHub URL string to parse
    ///
    /// # Returns
    /// * `Some(GitHubUrl)` if the URL is valid and can be parsed
    /// * `None` if the URL is invalid or cannot be parsed
    ///
    /// # Examples
    /// ```
    /// use queensac::GitHubUrl;
    ///
    /// let url = "https://github.com/owner/repo/blob/main/src/main.rs";
    /// let github_url = GitHubUrl::parse(url).unwrap();
    /// assert_eq!(github_url.owner(), "owner");
    /// assert_eq!(github_url.repo(), "repo");
    /// ```
    pub fn parse(url: &str) -> Option<Self> {
        let re = Regex::new(r"^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)(?:/(?:tree|blob)/([^/]+)(?:/(.+))?)?$").ok()?;

        re.captures(url).and_then(|caps| {
            let owner = caps.get(1)?.as_str().to_string();
            let repo = caps.get(2)?.as_str().to_string();
            let branch = caps.get(3).map(|m| m.as_str().to_string());
            let file_path = caps.get(4).map(|m| m.as_str().to_string());

            Some(Self {
                owner,
                repo,
                branch,
                file_path,
            })
        })
    }

    /// Returns the owner/organization name from the GitHub URL
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Returns the repository name from the GitHub URL
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// Returns the branch name if specified in the URL
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Returns the file path within the repository if specified in the URL
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Returns the clone URL for the GitHub repository
    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }

    /// Attempts to find the current location of a file in the repository
    ///
    /// # Returns
    /// * `Ok(Some(String))` - The current location of the file if found
    /// * `Ok(None)` - If the file was not found
    /// * `Err(git2::Error)` - If there was an error accessing the repository
    pub fn find_current_location(&self) -> Result<Option<String>, git2::Error> {
        let file_path = self
            .file_path()
            .ok_or_else(|| git2::Error::from_str("No file path in URL"))?;

        let repo_manager = RepoManager::clone_repo(&self.clone_url(), self.branch())?;
        let repo = repo_manager.get_repo();

        let mut current_path = file_path.to_string();

        loop {
            if file_exists_in_repo(repo, &current_path)? {
                return Ok(Some(current_path));
            }

            let commit = match find_last_commit_id(&current_path, repo) {
                Ok(commit) => commit,
                Err(e) => {
                    error!("Error finding last commit for {}: {}", current_path, e);
                    return Ok(None);
                }
            };

            match track_file_rename_in_commit(repo, &commit, &current_path)? {
                Some(new_path) => {
                    current_path = new_path;
                }
                None => {
                    error!(
                        "Could not find new path for {} in commit {}",
                        current_path,
                        commit.id()
                    );
                    return Ok(None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_url_parse() {
        let url = "https://github.com/owner/repo/blob/main/src/main.rs";
        let github_url = GitHubUrl::parse(url).unwrap();

        assert_eq!(github_url.owner(), "owner");
        assert_eq!(github_url.repo(), "repo");
        assert_eq!(github_url.branch(), Some("main"));
        assert_eq!(github_url.file_path(), Some("src/main.rs"));
        assert_eq!(github_url.clone_url(), "https://github.com/owner/repo");
    }

    #[test]
    fn test_github_url_parse_invalid() {
        let url = "https://redddy.com/owner/repo";
        assert!(GitHubUrl::parse(url).is_none());
    }

    #[test]
    fn test_github_url_parse_with_branch() {
        let tree_url =
            GitHubUrl::parse("https://github.com/owner/repo/tree/master/tests/ui").unwrap();
        let blob_url =
            GitHubUrl::parse("https://github.com/owner/repo/blob/main/src/main.rs").unwrap();

        assert_eq!(tree_url.file_path(), Some("tests/ui"));
        assert_eq!(blob_url.file_path(), Some("src/main.rs"));
    }

    #[test]
    fn test_extract_branch_from_url() {
        let tree_url = GitHubUrl::parse("https://github.com/owner/repo/tree/main/src").unwrap();
        let blob_url =
            GitHubUrl::parse("https://github.com/owner/repo/blob/develop/Cargo.toml").unwrap();

        assert_eq!(tree_url.branch(), Some("main"));
        assert_eq!(blob_url.branch(), Some("develop"));
    }

    #[test]
    fn test_no_branch() {
        let url = "https://github.com/owner/repo/blob";
        assert!(GitHubUrl::parse(url).is_none());
    }

    #[test]
    /// This test is related to `file_tracker::tests::test_track_file_rename_in_commit_with_multiple_moves`
    /// which demonstrates the same file movement pattern:
    /// 1. Initially located at: tmp.txt (root directory)
    /// 2. First moved to: dockerfile_history/tmp.txt
    /// 3. Finally moved to: img/tmp.txt
    fn test_find_current_location() {
        let url = GitHubUrl::parse(
            "https://github.com/reddevilmidzy/zero2prod/blob/test_for_queensac/tmp.txt",
        )
        .unwrap();

        assert_eq!(
            url.find_current_location().unwrap(),
            Some("img/tmp.txt".to_string())
        );
    }

    #[test]
    /// This test verifies the behavior when a file cannot be found in the repository.
    /// It tests two scenarios:
    /// 1. A file that never existed in the repository
    /// 2. A file that was deleted and not moved anywhere else
    fn test_find_current_location_file_not_found() {
        // Test case 1: File that never existed
        let url = GitHubUrl::parse(
            "https://github.com/reddevilmidzy/test-queensac/blob/main/non_existent.txt",
        )
        .unwrap();

        assert_eq!(url.find_current_location().unwrap(), None);

        // Test case 2: File that was deleted
        let url = GitHubUrl::parse(
            "https://github.com/reddevilmidzy/test-queensac/blob/main/will_be_deleted.rs",
        )
        .unwrap();

        assert_eq!(url.find_current_location().unwrap(), None);
    }
}
