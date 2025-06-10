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

        // Ignore merge commits(2+ Paranent) because that's what 'git whatchenged' does.
        // Ignore commit with 0 parent (initial commit) because there's nothing to diff againist.

        if commit.parent_count() == 1 {
            let prev_commit = commit.parent(0)?;
            let tree = commit.tree()?;
            let prev_tree = prev_commit.tree()?;
            let diff = repo.diff_tree_to_tree(Some(&prev_tree), Some(&tree), None)?;
            for delta in diff.deltas() {
                let file_path = delta.new_file().path().unwrap();
                if file_path.to_str().unwrap() == target_file {
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
            && delta.old_file().path().unwrap().to_str().unwrap() == target_file
        {
            return Ok(Some(
                delta
                    .new_file()
                    .path()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ));
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
    use crate::RepoManager;

    use super::*;

    #[test]
    fn test_track_file_rename_in_commit() -> Result<(), git2::Error> {
        let repo_manager =
            RepoManager::clone_repo("https://github.com/reddevilmidzy/queensac", Some("main"))?;
        let commit = find_last_commit_id("Cargo.toml", &repo_manager.get_repo())?;
        assert_eq!(
            commit.id().to_string(),
            "45203e841d42cf393e4d0a786b0a1a4ab267e91d"
        );
        assert_eq!(
            track_file_rename_in_commit(&repo_manager.get_repo(), &commit, "Cargo.toml")?,
            Some("rook/Cargo.toml".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_file_exists_in_repo() -> Result<(), git2::Error> {
        let repo_manager =
            RepoManager::clone_repo("https://github.com/reddevilmidzy/reddevilmidzy", None)?;

        assert!(file_exists_in_repo(repo_manager.get_repo(), "README.md")?);

        assert!(!file_exists_in_repo(
            repo_manager.get_repo(),
            "non_existent_file.md"
        )?);

        Ok(())
    }

    #[test]
    /// This test demonstrates the low-level Git operations for tracking file movements.
    /// It is related to `url::tests::test_find_github_file_new_path` which tests the same
    /// file movement pattern at a higher level using GitHub URLs.
    ///
    /// The test verifies the following file movement pattern:
    /// 1. Initially located at: tmp.txt (root directory)
    /// 2. First moved to: dockerfile_history/tmp.txt
    /// 3. Finally moved to: img/tmp.txt
    fn test_track_file_rename_in_commit_with_multiple_moves() -> Result<(), git2::Error> {
        let repo_manager = RepoManager::clone_repo(
            "https://github.com/reddevilmidzy/zero2prod",
            Some("test_for_queensac"),
        )?;

        // 1. Find the commit where tmp.txt was moved to dockerfile_history/tmp.txt
        let commit = find_last_commit_id("tmp.txt", &repo_manager.get_repo())?;
        let new_path = track_file_rename_in_commit(&repo_manager.get_repo(), &commit, "tmp.txt")?;
        assert_eq!(new_path, Some("dockerfile_history/tmp.txt".to_string()));

        // 2. Find the commit where dockerfile_history/tmp.txt was moved to img/tmp.txt
        let commit = find_last_commit_id("dockerfile_history/tmp.txt", &repo_manager.get_repo())?;
        let new_path = track_file_rename_in_commit(
            &repo_manager.get_repo(),
            &commit,
            "dockerfile_history/tmp.txt",
        )?;
        assert_eq!(new_path, Some("img/tmp.txt".to_string()));

        // 3. Verify that the file exists at the final location
        assert!(file_exists_in_repo(
            &repo_manager.get_repo(),
            "img/tmp.txt"
        )?);

        // 4. Verify that the file doesn't exist at the original location
        assert!(!file_exists_in_repo(&repo_manager.get_repo(), "tmp.txt")?);

        Ok(())
    }
}
