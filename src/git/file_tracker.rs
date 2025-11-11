use git2::{Commit, Delta, DiffFindOptions, ErrorCode, Repository};
use std::path;

/// Represents the result of searching for the last commit that touched a target path.
///
/// This struct contains both the commit that last modified the target path and,
/// if the path was renamed in that commit, the new path it was renamed to.
///
/// # Fields
/// * `commit` - The commit that last touched the target path
/// * `renamed_path` - If the target path was renamed in this commit, contains the new path.
///   For files, this is the full new file path. For directories, this is the new directory
///   path with a trailing slash. If the path was not renamed, this is `None`.
pub struct CommitSearchResult<'a> {
    /// The commit that last touched the target path
    pub commit: Commit<'a>,
    /// The new path if the target was renamed in this commit, `None` otherwise
    pub renamed_path: Option<String>,
}

/// This function searches through the commit history from HEAD backwards to find
/// the most recent commit that modified the target path. It also detects if the
/// path was renamed in that commit and returns the new path.
///
/// The function ignores merge commits (commits with 2+ parents) and initial commits
/// (commits with 0 parents), following the same behavior as `git whatchanged`.
///
/// # Arguments
/// * `target_file` - The path to the target file or directory to search for
/// * `repo` - The repository to search in
///
/// # Returns
/// * `Ok(CommitSearchResult)` - Contains the commit that last touched the target path
///   and optionally the new path if it was renamed in that commit
/// * `Err(git2::Error)` - If there was an error accessing the repository or if the
///   target path was not found in the repository history
pub fn find_last_commit_id<'a>(
    target_file: &str,
    repo: &'a Repository,
) -> Result<CommitSearchResult<'a>, git2::Error> {
    let target_path = path::Path::new(target_file);
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    for commit_id in revwalk {
        let commit_id = commit_id?;
        let commit = repo.find_commit(commit_id)?;

        if commit.parent_count() == 1 {
            let prev_commit = commit.parent(0)?;
            let tree = commit.tree()?;
            let prev_tree = prev_commit.tree()?;
            let mut diff = repo.diff_tree_to_tree(Some(&prev_tree), Some(&tree), None)?;

            let mut find_opts = DiffFindOptions::new();
            find_opts.rename_threshold(50); // Git default threshold 50%
            diff.find_similar(Some(&mut find_opts))?;
            for delta in diff.deltas() {
                let mut renamed_path = None;

                // file check
                if let Some(file_path) = delta.new_file().path()
                    && file_path == target_path
                {
                    return Ok(CommitSearchResult {
                        commit,
                        renamed_path: None,
                    });
                }
                // directory check
                if let Some(old_path) = delta.old_file().path()
                    && old_path.starts_with(target_path)
                {
                    if old_path == target_path && delta.status() == Delta::Renamed {
                        renamed_path = delta
                            .new_file()
                            .path()
                            .and_then(|p| p.to_str())
                            .map(|s| s.to_string());
                    } else if delta.status() == Delta::Renamed
                        && let Some(path) = delta.new_file().path()
                        && let Some(parent) = path.parent()
                    {
                        let mut dir = parent.to_string_lossy().to_string();
                        if !dir.ends_with('/') && !dir.ends_with('\\') {
                            dir.push('/');
                        }
                        renamed_path = Some(dir);
                    }

                    return Ok(CommitSearchResult {
                        commit,
                        renamed_path,
                    });
                }
            }
        }
    }
    Err(git2::Error::from_str("File not found"))
}

/// Checks if a file exists in the repository at the given path
///
/// # Arguments
/// * `repo` - The repository to check in
/// * `file_path` - The path of the file to check
///
/// # Returns
/// * `Ok(bool)` - `true` if the file exists, `false` otherwise
/// * `Err(git2::Error)` - If there was an error accessing the repository
pub fn file_exists_in_repo(repo: &Repository, file_path: &str) -> Result<bool, git2::Error> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    let tree = commit.tree()?;

    match tree.get_path(path::Path::new(file_path)) {
        Ok(_) => Ok(true),
        Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use crate::{GitHubUrl, RepoManager};

    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_file_exists_in_repo() -> Result<(), git2::Error> {
        let github_url = GitHubUrl::new(
            "reddevilmidzy".to_string(),
            "kingsac".to_string(),
            None,
            None,
        );
        let repo_manager = RepoManager::from(&github_url)?;

        assert!(file_exists_in_repo(repo_manager.get_repo(), "README.md")?);

        assert!(!file_exists_in_repo(
            repo_manager.get_repo(),
            "non_existent_file.md"
        )?);

        Ok(())
    }

    #[test]
    #[serial]
    /// This test demonstrates the low-level Git operations for tracking file movements.
    /// It is related to `url::tests::test_find_github_file_new_path` which tests the same
    /// file movement pattern at a higher level using GitHub URLs.
    ///
    /// The test verifies the following file movement pattern:
    /// 1. Initially located at: test_for_multiple_moves.rs (root directory)
    /// 2. First moved to: foo/test_for_multiple_moves.rs
    /// 3. Finally moved to: bar/test_for_multiple_moves.rs
    fn test_track_file_rename_in_commit_with_multiple_moves() -> Result<(), git2::Error> {
        let github_url = GitHubUrl::new(
            "reddevilmidzy".to_string(),
            "kingsac".to_string(),
            None,
            None,
        );
        let repo_manager = RepoManager::from(&github_url)?;

        // 1. Find the commit where test_for_multiple_moves.rs was moved to foo/test_for_multiple_moves.rs
        let result = find_last_commit_id("test_for_multiple_moves.rs", repo_manager.get_repo())?;
        assert_eq!(
            result.renamed_path,
            Some("foo/test_for_multiple_moves.rs".to_string())
        );

        // 2. Find the commit where foo/test_for_multiple_moves.rs was moved to bar/test_for_multiple_moves.rs
        let result =
            find_last_commit_id("foo/test_for_multiple_moves.rs", repo_manager.get_repo())?;
        assert_eq!(
            result.renamed_path,
            Some("bar/test_for_multiple_moves.rs".to_string())
        );

        // 3. Verify that the file exists at the final location
        assert!(file_exists_in_repo(
            repo_manager.get_repo(),
            "bar/test_for_multiple_moves.rs"
        )?);

        // 4. Verify that the file doesn't exist at the original location
        assert!(!file_exists_in_repo(
            repo_manager.get_repo(),
            "test_for_multiple_moves.rs"
        )?);
        assert!(!file_exists_in_repo(
            repo_manager.get_repo(),
            "foo/test_for_multiple_moves.rs"
        )?);

        Ok(())
    }
}
