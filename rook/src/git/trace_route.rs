use git2::Repository;
use regex::Regex;

use crate::RepoManager;

// git log --follow -- <file_path>
fn find_last_commit_id<'a>(
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

/// Find the file new path in a commit
fn find_file_new_path(
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

fn extract_file_path_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"^https?://(?:www\.)?github\.com/[^/]+/[^/]+/(?:tree|blob)/[^/]+/(.+)$")
        .ok()?;

    re.captures(url)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_repo_info_from_url(url: &str) -> Option<(String, String)> {
    let re = Regex::new(r"^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)").ok()?;
    re.captures(url).and_then(|caps| {
        let owner = caps.get(1)?.as_str().to_string();
        let repo = caps.get(2)?.as_str().to_string();
        Some((owner, repo))
    })
}

fn extract_branch_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"github\.com/[^/]+/[^/]+/(?:tree|blob)/([^/]+)").ok()?;
    re.captures(url)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn find_github_file_new_path(repo_url: &str) -> Result<Option<String>, git2::Error> {
    let file_path = extract_file_path_from_url(repo_url)
        .ok_or_else(|| git2::Error::from_str("Invalid GitHub URL format"))?;
    let (owner, repo) = extract_repo_info_from_url(repo_url)
        .ok_or_else(|| git2::Error::from_str("Invalid GitHub URL format"))?;
    let branch = extract_branch_from_url(repo_url)
        .ok_or_else(|| git2::Error::from_str("Invalid GitHub URL format"))?;

    let clone_url = format!("https://github.com/{}/{}", owner, repo);
    let repo_manager = RepoManager::clone_repo(&clone_url, Some(&branch))?;
    let commit = find_last_commit_id(&file_path, repo_manager.get_repo())?;
    let new_path = find_file_new_path(repo_manager.get_repo(), &commit, &file_path)?;
    Ok(new_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_file_new_path() -> Result<(), git2::Error> {
        let repo = Repository::open("../.")?;
        let commit = find_last_commit_id("Cargo.toml", &repo)?;
        assert_eq!(
            commit.id().to_string(),
            "45203e841d42cf393e4d0a786b0a1a4ab267e91d"
        );
        assert_eq!(
            find_file_new_path(&repo, &commit, "Cargo.toml")?,
            Some("rook/Cargo.toml".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_extract_file_path_from_url() {
        let tree_url = "https://github.com/owner/repo/tree/master/tests/ui";
        let blob_url = "https://github.com/owner/repo/blob/main/src/main.rs";

        assert_eq!(
            extract_file_path_from_url(tree_url),
            Some("tests/ui".to_string())
        );
        assert_eq!(
            extract_file_path_from_url(blob_url),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn test_find_github_file_new_path() -> Result<(), git2::Error> {
        let file_url = "https://github.com/reddevilmidzy/queensac/blob/main/Cargo.toml";

        let new_path = find_github_file_new_path(file_url)?;
        assert_eq!(new_path, Some("rook/Cargo.toml".to_string()));

        Ok(())
    }

    #[test]
    fn test_extract_branch_from_url() {
        let tree_url = "https://github.com/owner/repo/tree/main/src";
        let blob_url = "https://github.com/owner/repo/blob/develop/Cargo.toml";
        let no_branch_url = "https://github.com/owner/repo/blob";

        assert_eq!(extract_branch_from_url(tree_url), Some("main".to_string()));
        assert_eq!(
            extract_branch_from_url(blob_url),
            Some("develop".to_string())
        );
        assert_eq!(extract_branch_from_url(no_branch_url), None);
    }
}
