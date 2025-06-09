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
    /// Clones a Git repository and optionally checks out a specific branch.
    ///
    /// # Arguments
    /// * `repo_url` - The URL of the repository to clone
    /// * `branch` - Optional branch name to check out after cloning
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

        let repo = Repository::clone(repo_url, &temp_dir)?;

        if let Some(branch_name) = branch {
            let mut remote = repo.find_remote("origin")?;
            remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
            Self::checkout_branch(&repo, branch_name)?;
        }

        Ok(Self {
            repo,
            _temp_dir_guard,
        })
    }

    /// Checks out a specific branch in the repository.
    ///
    /// # Arguments
    /// * `repo` - The repository to check out the branch in
    /// * `branch_name` - The name of the branch to check out
    ///
    /// # Returns
    /// `Ok(())` if the branch was successfully checked out, or an error if the branch doesn't exist.
    pub fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<(), git2::Error> {
        let remote_branch = format!("origin/{}", branch_name);
        let reference = repo
            .find_reference(&format!("refs/remotes/{}", remote_branch))
            .map_err(|_| git2::Error::from_str(&format!("Branch not found: {}", branch_name)))?;

        let commit = reference.peel_to_commit()?;
        repo.checkout_tree(commit.as_object(), None)?;
        repo.set_head(&format!("refs/heads/{}", branch_name))?;
        Ok(())
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

    #[test]
    #[serial]
    fn test_checkout_branch_with_valid_branch() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let repo_manager = RepoManager::clone_repo(repo_url, Some("main")).unwrap();
        assert!(repo_manager.get_repo().head().unwrap().name().unwrap() == "refs/heads/main");
    }

    #[test]
    #[serial]
    fn test_checkout_branch_with_invalid_branch() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let result = RepoManager::clone_repo(repo_url, Some("non-existent-branch"));

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
    fn test_clone_without_branch() {
        let repo_url = "https://github.com/reddevilmidzy/woowalog";
        let repo_manager = RepoManager::clone_repo(repo_url, None).unwrap();
        assert!(repo_manager.get_repo().head().is_ok());
    }
}
