use git2::{Commit, Delta, DiffFindOptions, DiffOptions, ErrorCode, Repository};
use std::path;

/// Finds the last commit ID that contains the target file
///
/// # Arguments
/// * `target_file` - The path to the target file
/// * `repo` - The repository to search in
///
/// # Returns
/// * `Ok(git2::Commit)` - The last commit ID that contains the target file
/// * `Err(git2::Error)` - If there was an error accessing the repository
pub fn find_last_commit_id<'a>(
    target_file: &str,
    repo: &'a Repository,
) -> Result<Commit<'a>, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    for commit_id in revwalk {
        let commit_id = commit_id?;
        let commit = repo.find_commit(commit_id)?;

        // Ignore merge commits(2+ Parents) because that's what 'git whatchenged' does.
        // Ignore commit with 0 parent (initial commit) because there's nothing to diff against.

        if commit.parent_count() == 1 {
            let prev_commit = commit.parent(0)?;
            let tree = commit.tree()?;
            let prev_tree = prev_commit.tree()?;
            let diff = repo.diff_tree_to_tree(Some(&prev_tree), Some(&tree), None)?;
            for delta in diff.deltas() {
                if let Some(file_path) = delta.new_file().path()
                    && let Some(file_path_str) = file_path.to_str()
                    && file_path_str == target_file
                {
                    return Ok(commit);
                }
            }
        }
    }

    Err(git2::Error::from_str("File not found"))
}

/// Finds the new path of a file that has been moved in a commit
///
/// # Arguments
/// * `repo` - The repository to search in
/// * `commit` - The commit to search in
/// * `target_file` - The path to the target file
///
pub fn track_file_rename_in_commit(
    repo: &Repository,
    commit: &Commit,
    target_file: &str,
) -> Result<Option<String>, git2::Error> {
    if commit.parent_count() != 1 {
        return Ok(None);
    }

    let mut diff_opts = DiffOptions::new();

    let mut find_opts = DiffFindOptions::new();
    // TODO 적절한 값 찾기
    find_opts.rename_threshold(28);

    let parent = commit.parent(0)?;
    let tree = commit.tree()?;
    let parent_tree = parent.tree()?;

    let mut diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut diff_opts))?;
    diff.find_similar(Some(&mut find_opts))?;

    for delta in diff.deltas() {
        if delta.status() == Delta::Renamed
            && (delta.old_file().path().and_then(|p| p.to_str()) == Some(target_file))
        {
            return Ok(delta
                .new_file()
                .path()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string()));
        }
    }

    Ok(None)
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
    fn test_track_file_rename_in_commit() -> Result<(), git2::Error> {
        let github_url = GitHubUrl::new(
            "reddevilmidzy".to_string(),
            "kingsac".to_string(),
            Some("main".to_string()),
            None,
        );
        let repo_manager = RepoManager::from(&github_url)?;
        let commit = find_last_commit_id("main.rs", &repo_manager.get_repo())?;
        // see https://github.com/reddevilmidzy/kingsac/commit/2f3e99cbea53c55c8428d5bc11bfe7f1ff5cccd7
        assert_eq!(
            commit.id().to_string(),
            "2f3e99cbea53c55c8428d5bc11bfe7f1ff5cccd7"
        );
        assert_eq!(
            track_file_rename_in_commit(&repo_manager.get_repo(), &commit, "main.rs")?,
            Some("src/main.rs".to_string())
        );

        Ok(())
    }

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
        let commit = find_last_commit_id("test_for_multiple_moves.rs", &repo_manager.get_repo())?;
        let new_path = track_file_rename_in_commit(
            &repo_manager.get_repo(),
            &commit,
            "test_for_multiple_moves.rs",
        )?;
        assert_eq!(new_path, Some("foo/test_for_multiple_moves.rs".to_string()));

        // 2. Find the commit where foo/test_for_multiple_moves.rs was moved to bar/test_for_multiple_moves.rs
        let commit =
            find_last_commit_id("foo/test_for_multiple_moves.rs", &repo_manager.get_repo())?;
        let new_path = track_file_rename_in_commit(
            &repo_manager.get_repo(),
            &commit,
            "foo/test_for_multiple_moves.rs",
        )?;
        assert_eq!(new_path, Some("bar/test_for_multiple_moves.rs".to_string()));

        // 3. Verify that the file exists at the final location
        assert!(file_exists_in_repo(
            &repo_manager.get_repo(),
            "bar/test_for_multiple_moves.rs"
        )?);

        // 4. Verify that the file doesn't exist at the original location
        assert!(!file_exists_in_repo(
            &repo_manager.get_repo(),
            "test_for_multiple_moves.rs"
        )?);
        assert!(!file_exists_in_repo(
            &repo_manager.get_repo(),
            "foo/test_for_multiple_moves.rs"
        )?);

        Ok(())
    }
}
