use std::{env, fs, path::PathBuf};

use git2::Repository;

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
}
