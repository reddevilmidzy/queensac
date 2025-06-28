use std::{env, fs, path::PathBuf, time};

//TODO 문서화 보완 지금 하자!!!
use git2::{BranchType, Oid, Repository, Signature, build::CheckoutBuilder};
use tracing::{error, info};

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

    /// Returns a reference to the temporary directory path.
    pub fn get_path(&self) -> &PathBuf {
        &self.path
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
            "github_repo_temp/{}/{}_{}",
            repo_url.split('/').nth(3).unwrap_or("unknown"),
            repo_url.split('/').nth(4).unwrap_or("unknown"),
            time::SystemTime::now()
                .duration_since(time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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

    /// Creates a new branch from the current HEAD
    pub async fn create_branch(&self, branch_name: &str) -> Result<(), git2::Error> {
        info!("Creating branch: {}", branch_name);

        let head = self.repo.head()?;
        let head_commit = self.repo.find_commit(head.target().unwrap())?;

        self.repo.branch(branch_name, &head_commit, false)?;

        info!("Successfully created branch: {}", branch_name);
        Ok(())
    }

    /// Checks out a branch
    pub async fn checkout_branch(&self, branch_name: &str) -> Result<(), git2::Error> {
        info!("Checking out branch: {}", branch_name);

        // Find the branch
        let (object, reference) = self.repo.revparse_ext(branch_name)?;

        let mut checkout_builder = CheckoutBuilder::new();
        checkout_builder.force();

        self.repo
            .checkout_tree(&object, Some(&mut checkout_builder))?;

        match reference {
            Some(reference) => {
                if let Some(name) = reference.name() {
                    self.repo.set_head(name)?;
                } else {
                    return Err(git2::Error::from_str("Could not get branch name"));
                }
            }
            None => {
                self.repo.set_head_detached(object.id())?;
            }
        }

        info!("Successfully checked out branch: {}", branch_name);
        Ok(())
    }

    /// Adds a file to the staging area
    pub async fn add_file(&self, file_path: &str) -> Result<(), git2::Error> {
        info!("Adding file to staging area: {}", file_path);

        let mut index = self.repo.index()?;
        index.add_path(std::path::Path::new(file_path))?;
        index.write()?;

        info!("Successfully added file: {}", file_path);
        Ok(())
    }

    /// Adds all changes to the staging area
    pub async fn add_all(&self) -> Result<(), git2::Error> {
        info!("Adding all changes to staging area");

        let mut index = self.repo.index()?;
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;

        info!("Successfully added all changes");
        Ok(())
    }

    /// Creates a commit with the given message
    pub async fn commit(
        &self,
        message: &str,
        author_name: &str,
        author_email: &str,
    ) -> Result<Oid, git2::Error> {
        info!("Creating commit with message: {}", message);

        let signature = Signature::now(author_name, author_email)?;

        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let head = self.repo.head()?;
        let parent_commit = self.repo.find_commit(head.target().unwrap())?;

        let commit_id = self.repo.commit(
            Some(head.name().unwrap()),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )?;

        info!("Successfully created commit: {}", commit_id);
        Ok(commit_id)
    }

    /// Pushes the current branch to the remote repository
    pub async fn push(&self, remote_name: &str, branch_name: &str) -> Result<(), git2::Error> {
        info!("Pushing branch {} to remote {}", branch_name, remote_name);

        let mut remote = self.repo.find_remote(remote_name)?;

        // Get the current branch reference
        let branch = self.repo.find_branch(branch_name, BranchType::Local)?;
        let reference = branch.get();

        // Push the branch
        remote.push(&[reference.name().unwrap()], None)?;

        info!(
            "Successfully pushed branch {} to remote {}",
            branch_name, remote_name
        );
        Ok(())
    }

    /// Gets the current branch name
    pub fn get_current_branch(&self) -> Result<String, git2::Error> {
        let head = self.repo.head()?;
        let branch_name = head
            .shorthand()
            .ok_or_else(|| git2::Error::from_str("Could not get branch name"))?;

        Ok(branch_name.to_string())
    }

    /// Checks if there are any uncommitted changes
    pub fn has_uncommitted_changes(&self) -> Result<bool, git2::Error> {
        let statuses = self.repo.statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .include_ignored(false)
                .include_unmodified(false),
        ))?;

        // Check if there are any changes in working directory or staging area
        for entry in statuses.iter() {
            let status = entry.status();

            // Working directory changes
            if status.is_wt_new() || status.is_wt_modified() || status.is_wt_deleted() {
                return Ok(true);
            }

            // Staging area changes
            if status.is_index_new() || status.is_index_modified() || status.is_index_deleted() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Gets the repository path
    pub fn get_repo_path(&self) -> PathBuf {
        self.repo.path().parent().unwrap().to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

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

    #[tokio::test]
    async fn test_create_and_checkout_branch() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, Some("main")).unwrap();

        // Create initial commit if needed
        if repo_manager.has_uncommitted_changes().unwrap() {
            repo_manager.add_all().await.unwrap();
            repo_manager
                .commit("Test commit", "Test User", "test@example.com")
                .await
                .unwrap();
        }

        // Create and checkout new branch
        repo_manager
            .create_branch("test-feature-branch")
            .await
            .unwrap();

        repo_manager
            .checkout_branch("test-feature-branch")
            .await
            .unwrap();

        assert_eq!(
            repo_manager.get_current_branch().unwrap(),
            "test-feature-branch"
        );
    }

    #[tokio::test]
    async fn test_add_and_commit() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, Some("main")).unwrap();

        // Create a test file
        let test_file = repo_manager.get_repo_path().join("test_file.txt");
        fs::write(&test_file, "test content").unwrap();

        // Add and commit
        repo_manager.add_file("test_file.txt").await.unwrap();
        let commit_id = repo_manager
            .commit("Test commit", "Test User", "test@example.com")
            .await
            .unwrap();

        assert!(!commit_id.is_zero());
        assert!(!repo_manager.has_uncommitted_changes().unwrap());
    }

    #[tokio::test]
    async fn test_has_uncommitted_changes() {
        let repo_manager = RepoManager::clone_repo(TEST_REPO_URL, Some("main")).unwrap();

        assert!(!repo_manager.has_uncommitted_changes().unwrap());

        let test_file = repo_manager.get_repo_path().join("test_changes.txt");
        fs::write(&test_file, "test content").unwrap();

        // Should have uncommitted changes
        assert!(repo_manager.has_uncommitted_changes().unwrap());

        // Add the file
        repo_manager.add_file("test_changes.txt").await.unwrap();

        // Should still have uncommitted changes (not committed yet)
        assert!(repo_manager.has_uncommitted_changes().unwrap());

        // Commit the changes
        repo_manager
            .commit("Test commit", "Test User", "test@example.com")
            .await
            .unwrap();

        // Should not have uncommitted changes
        assert!(!repo_manager.has_uncommitted_changes().unwrap());
    }
}
