use regex::Regex;

use crate::{RepoManager, find_file_new_path, find_last_commit_id};

#[derive(Debug)]
pub struct GitHubUrl {
    owner: String,
    repo: String,
    branch: Option<String>,
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

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }

    pub fn find_github_file_new_path(&self) -> Result<Option<String>, git2::Error> {
        let file_path = self
            .file_path()
            .ok_or_else(|| git2::Error::from_str("No file path in URL"))?;

        let repo_manager = RepoManager::clone_repo(&self.clone_url(), self.branch())?;

        let commit = find_last_commit_id(file_path, repo_manager.get_repo())?;
        let new_path = find_file_new_path(repo_manager.get_repo(), &commit, file_path)?;
        Ok(new_path)
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
}
