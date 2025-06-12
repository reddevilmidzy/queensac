use std::{env, fs, path::PathBuf};

use git2::Repository;
use tracing::error;

use crate::{GitHubUrl, file_exists_in_repo, find_last_commit_id, track_file_rename_in_commit};

/// A guard that automatically removes a temporary directory when dropped.
pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    /// Creates a new temporary directory guard.
    /// If the directory already exists, it will be removed and recreated.
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
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

/// Manages a Git repository with automatic cleanup of temporary files.
pub struct RepoManager {
    repo: Repository,
    _temp_dir_guard: TempDirGuard,
}

impl RepoManager {
    /// Clones a Git repository from a GitHub URL.
    ///
    /// # Arguments
    /// * `url` - The GitHub URL of the repository to clone
    ///
    /// # Returns
    /// A `RepoManager` instance that will automatically clean up the cloned repository when dropped.
    pub fn from_github_url(url: &GitHubUrl) -> Result<Self, git2::Error> {
        Self::clone_repo(&url.clone_url(), url.branch())
    }

    /// Clones a Git repository, optionally cloning only a specific branch.
    ///
    /// When a branch name is provided, only that specific branch will be cloned,
    /// which is more efficient than cloning the entire repository and then checking out.
    /// If no branch is specified, the default branch will be cloned.
    ///
    /// # Arguments
    /// * `repo_url` - The URL of the repository to clone
    /// * `branch` - Optional branch name to clone. If provided, only this branch will be cloned.
    ///
    /// # Returns
    /// A `RepoManager` instance that will automatically clean up the cloned repository when dropped.
    pub fn clone_repo(repo_url: &str, branch: Option<&str>) -> Result<Self, git2::Error> {
        let temp_dir = env::temp_dir().join(format!(
            "github_repo_temp/{}/{}",
            repo_url.split('/').nth(3).unwrap_or("unknown"),
            repo_url.split('/').nth(4).unwrap_or("unknown")
        ));

        let _temp_dir_guard = TempDirGuard::new(temp_dir.clone()).map_err(|e| {
            git2::Error::from_str(&format!("Failed to create temporary directory: {}", e))
        })?;

        let mut builder = git2::build::RepoBuilder::new();

        if let Some(branch_name) = branch {
            builder.branch(branch_name);
        }

        let repo = builder.clone(repo_url, &temp_dir)?;

        Ok(Self {
            repo,
            _temp_dir_guard,
        })
    }

    /// Attempts to find the current location of a file in the repository
    ///
    /// # Returns
    /// * `Ok(Some(String))` - The current location of the file if found
    /// * `Ok(None)` - If the file was not found
    /// * `Err(git2::Error)` - If there was an error accessing the repository
    pub fn find_current_location(
        &self,
        github_url: &GitHubUrl,
    ) -> Result<Option<String>, git2::Error> {
        let file_path = github_url
            .file_path()
            .ok_or_else(|| git2::Error::from_str("No file path in URL"))?;

        let repo = self.get_repo();
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

    /// Returns a reference to the managed Git repository.
    pub fn get_repo(&self) -> &Repository {
        &self.repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    static TEST_REPO_URL: &str = "https://github.com/reddevilmidzy/kingsac";

    #[test]
    #[serial]
    fn test_checkout_branch_with_valid_branch() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, Some("main")).unwrap();

        assert!(repo_manager.get_repo().head().is_ok());
        assert!(repo_manager.get_repo().head().unwrap().name().unwrap() == "refs/heads/main");
    }

    #[test]
    #[serial]
    fn test_checkout_branch_with_default_branch() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, None).unwrap();

        assert!(repo_manager.get_repo().head().is_ok());
        assert!(repo_manager.get_repo().head().unwrap().name().unwrap() == "refs/heads/main");
    }

    #[test]
    #[serial]
    fn test_checkout_branch_with_invalid_branch() {
        let result = RepoManager::clone_repo(TEST_REPO_URL, Some("non-existent-branch"));

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

    #[test]
    #[serial]
    fn test_clone_with_not_default_branch() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, Some("maout")).unwrap();

        assert!(repo_manager.get_repo().head().is_ok());
        assert!(repo_manager.get_repo().head().unwrap().name().unwrap() == "refs/heads/maout");
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

        let repo_manager = RepoManager::from_github_url(&url).unwrap();

        assert_eq!(
            repo_manager.find_current_location(&url).unwrap(),
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
        let url =
            GitHubUrl::parse("https://github.com/reddevilmidzy/kingsac/blob/main/non_existent.txt")
                .unwrap();

        let repo_manager = RepoManager::from_github_url(&url).unwrap();
        assert_eq!(repo_manager.find_current_location(&url).unwrap(), None);

        // Test case 2: File that was deleted
        let url = GitHubUrl::parse(
            "https://github.com/reddevilmidzy/kingsac/blob/main/will_be_deleted.rs",
        )
        .unwrap();

        let repo_manager = RepoManager::from_github_url(&url).unwrap();
        assert_eq!(repo_manager.find_current_location(&url).unwrap(), None);
    }
}
