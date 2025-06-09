use git2::Repository;

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
) -> Result<git2::Commit<'a>, git2::Error> {
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
pub fn find_file_new_path(
    repo: &Repository,
    commit: &git2::Commit,
    target_file: &str,
) -> Result<Option<String>, git2::Error> {
    if commit.parent_count() != 1 {
        return Ok(None);
    }

    let mut diff_opts = git2::DiffOptions::new();

    let mut find_opts = git2::DiffFindOptions::new();
    // TODO 적절한 값 찾기
    find_opts.rename_threshold(28);

    let parent = commit.parent(0)?;
    let tree = commit.tree()?;
    let parent_tree = parent.tree()?;

    let mut diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut diff_opts))?;
    diff.find_similar(Some(&mut find_opts))?;

    for delta in diff.deltas() {
        if delta.status() == git2::Delta::Renamed
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

#[cfg(test)]
mod tests {
    use crate::RepoManager;

    use super::*;

    #[test]
    fn test_find_file_new_path() -> Result<(), git2::Error> {
        let repo_manager =
            RepoManager::clone_repo("https://github.com/reddevilmidzy/queensac", Some("main"))?;
        let commit = find_last_commit_id("Cargo.toml", &repo_manager.get_repo())?;
        assert_eq!(
            commit.id().to_string(),
            "45203e841d42cf393e4d0a786b0a1a4ab267e91d"
        );
        assert_eq!(
            find_file_new_path(&repo_manager.get_repo(), &commit, "Cargo.toml")?,
            Some("rook/Cargo.toml".to_string())
        );

        Ok(())
    }
}
