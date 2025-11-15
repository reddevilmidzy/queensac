use crate::RepoManager;

use octocrab::{Octocrab, models::InstallationToken, params::apps::CreateInstallationAccessToken};
use std::{path::PathBuf, time::SystemTime};
use thiserror::Error;
use tracing::{error, info};
use url::Url;

/// Represents errors that can occur during pull request generation.
#[derive(Debug, Error)]
pub enum PrError {
    #[error("Git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("GitHub API error: {0}")]
    GitHub(String),
    #[error("File operation failed: {0}")]
    File(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Represents a file change to be included in a pull request.
#[derive(Debug)]
pub struct FileChange {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub line_number: usize,
}

/// GitHub App configuration for authentication.
#[derive(Debug, Clone)]
pub struct GitHubAppConfig {
    app_id: u64,
    private_key: String,
}

/// Generates pull requests for link fixes in a repository.
pub struct PullRequestGenerator {
    repo_manager: RepoManager,
    base_branch: String,
    octocrab: Octocrab,
    access_token: String,
}

impl GitHubAppConfig {
    /// Creates a GitHub App configuration from environment variables.
    ///
    /// # Environment Variables
    /// * `QUEENSAC_APP_ID` - The GitHub App ID
    /// * `QUEENSAC_APP_PRIVATE_KEY` - The GitHub App private key (PEM format)
    pub fn from_env() -> Result<Self, PrError> {
        let app_id = read_env_var("QUEENSAC_APP_ID")?
            .parse::<u64>()
            .map_err(|e| PrError::Config(format!("Invalid QUEENSAC_APP_ID: {e}")))?;

        let private_key = read_env_var("QUEENSAC_APP_PRIVATE_KEY")?;

        Ok(Self {
            app_id,
            private_key,
        })
    }
}

impl PullRequestGenerator {
    /// Creates a new PullRequestGenerator with GitHub App authentication.
    ///
    /// # Arguments
    /// * `repo_manager` - The repository manager instance
    /// * `app_config` - GitHub App configuration
    /// * `base_branch` - The base branch for the pull request
    pub async fn new(
        repo_manager: RepoManager,
        app_config: GitHubAppConfig,
        base_branch: String,
    ) -> Result<Self, PrError> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(app_config.private_key.as_bytes())
            .map_err(|e| PrError::Config(format!("Failed to parse private key: {e}")))?;

        let octocrab = Octocrab::builder()
            .app(app_config.app_id.into(), key)
            .build()
            .map_err(|e| PrError::Config(format!("Failed to build Octocrab instance: {e}")))?;

        let installations = octocrab
            .apps()
            .installations()
            .send()
            .await
            .map_err(|e| PrError::GitHub(format!("Failed to get installations: {e}")))?;

        let installation = installations
            .into_iter()
            .find(|inst| {
                inst.account
                    .login
                    .eq_ignore_ascii_case(repo_manager.get_github_url().owner())
            })
            .ok_or_else(|| PrError::GitHub("No GitHub App installation found".to_string()))?;

        let mut create_access_token = CreateInstallationAccessToken::default();
        create_access_token.repositories = vec![repo_manager.get_github_url().repo().to_string()];

        let access_token_url =
            Url::parse(installation.access_tokens_url.as_ref().ok_or_else(|| {
                PrError::GitHub("Missing access_token_url in installation".to_string())
            })?)
            .map_err(|e| PrError::GitHub(format!("Failed to parse access token URL: {e}")))?;

        let access_token: InstallationToken = octocrab
            .post(access_token_url.path(), Some(&create_access_token))
            .await
            .map_err(|e| {
                PrError::GitHub(format!("Failed to create installation access token: {e}"))
            })?;

        let octocrab = Octocrab::builder()
            .personal_token(access_token.token.clone())
            .build()
            .map_err(|e| PrError::GitHub(format!("Failed to build Octocrab instance: {e}")))?;
        let token_string = access_token.token;

        Ok(Self {
            repo_manager,
            base_branch,
            octocrab,
            access_token: token_string,
        })
    }

    /// Creates a pull request with link fixes.
    ///
    /// # Arguments
    /// * `fixes` - The list of file changes to apply
    pub async fn create_fix_pr(&self, fixes: Vec<FileChange>) -> Result<String, PrError> {
        let branch_name = generate_branch_name();
        self.create_branch(&branch_name).await?;

        let changes = self.apply_fixes(fixes).await?;

        // Check if there are any changes before proceeding with commit, push, and PR
        if changes.is_empty() {
            info!("No file changes to commit. Skipping push and PR creation.");
            return Err(PrError::Config("No changes to create PR".to_string()));
        }

        self.commit_changes(&changes).await?;

        self.push_to_remote(branch_name.as_str()).await?;

        let pr_url = self
            .generate_pull_request_via_api(branch_name.as_str())
            .await?;

        info!("Successfully created PR: {}", pr_url);
        Ok(pr_url)
    }

    /// Creates a new feature branch from the current branch.
    async fn create_branch(&self, branch_name: &str) -> Result<(), PrError> {
        self.repo_manager.create_branch(branch_name).await?;
        self.repo_manager.checkout_branch(branch_name).await?;

        Ok(())
    }

    /// Applies link fixes to files in the repository.
    ///
    /// # Arguments
    /// * `fixes` - The list of file changes to apply
    async fn apply_fixes(&self, fixes: Vec<FileChange>) -> Result<Vec<FileChange>, PrError> {
        let mut changes = Vec::new();

        for fix in fixes {
            let file_path = PathBuf::from(&fix.file_path);
            let full_path = self.repo_manager.get_repo_path().join(&file_path);

            if !full_path.exists() {
                error!("File not found: {}", fix.file_path);
                continue;
            }

            let current_content = tokio::fs::read_to_string(&full_path).await.map_err(|e| {
                PrError::File(format!("Failed to read file {}: {}", fix.file_path, e))
            })?;

            let new_content = self.replace_line_content(
                &current_content,
                fix.line_number,
                &fix.old_content,
                &fix.new_content,
            )?;

            tokio::fs::write(&full_path, &new_content)
                .await
                .map_err(|e| {
                    PrError::File(format!("Failed to write file {}: {}", fix.file_path, e))
                })?;

            changes.push(FileChange {
                file_path: fix.file_path.clone(),
                old_content: current_content,
                new_content,
                line_number: fix.line_number,
            });

            info!(
                "Applied fix to {}:{}",
                fix.file_path.clone(),
                fix.line_number
            );
        }

        Ok(changes)
    }

    /// Replaces content in a specific line of a file.
    ///
    /// # Arguments
    /// * `content` - The file content
    /// * `line_number` - The line number to replace (1-based)
    /// * `old_url` - The old URL to replace
    /// * `new_url` - The new URL to insert
    fn replace_line_content(
        &self,
        content: &str,
        line_number: usize,
        old_url: &str,
        new_url: &str,
    ) -> Result<String, PrError> {
        let lines: Vec<&str> = content.lines().collect();

        if line_number == 0 || line_number > lines.len() {
            return Err(PrError::File(format!("Invalid line number: {line_number}")));
        }

        let line_index = line_number - 1;
        let old_line = lines[line_index];

        if !old_line.contains(old_url) {
            return Err(PrError::File(format!(
                "Old URL '{old_url}' not found in line {line_number}: {old_line}"
            )));
        }

        let new_line = old_line.replace(old_url, new_url);
        let mut new_lines = lines.clone();
        new_lines[line_index] = &new_line;

        Ok(new_lines.join("\n"))
    }

    /// Commits all file changes to the repository.
    ///
    /// # Arguments
    /// * `changes` - The list of file changes to commit
    async fn commit_changes(&self, changes: &[FileChange]) -> Result<(), PrError> {
        if changes.is_empty() {
            info!("No file changes to commit. Skipping commit creation.");
            return Ok(());
        }
        info!("Committing {} file changes", changes.len());

        for change in changes {
            self.repo_manager.add_file(&change.file_path).await?;
        }
        let author_name = "queensac[bot]";
        let author_email = "218335951+queensac[bot]@users.noreply.github.com";

        let commit_message = self.create_commit_message(changes);

        self.repo_manager
            .commit(&commit_message, author_name, author_email)
            .await?;

        info!("Successfully committed changes");
        Ok(())
    }

    /// Creates a descriptive commit message for the changes.
    ///
    /// # Arguments
    /// * `changes` - The list of file changes
    fn create_commit_message(&self, changes: &[FileChange]) -> String {
        let mut message = String::from("fix: Update broken links\n\n");

        for change in changes {
            message.push_str(&format!(
                "- Update link in {}:{}\n",
                change.file_path, change.line_number
            ));
        }

        message.push_str(
            "\nThis PR was automatically generated to fix broken links in the repository.",
        );
        message
    }

    /// Pushes the feature branch to the remote repository.
    async fn push_to_remote(&self, branch_name: &str) -> Result<(), PrError> {
        self.repo_manager
            .push("origin", branch_name, &self.access_token)
            .await?;

        info!("Successfully pushed branch to remote");
        Ok(())
    }

    /// Generates a pull request via the GitHub API.
    pub async fn generate_pull_request_via_api(
        &self,
        branch_name: &str,
    ) -> Result<String, PrError> {
        let (owner, repo) = self.get_repo_owner_and_name()?;

        let pr = self
            .octocrab
            .pulls(owner.as_str(), repo.as_str())
            .create(
                "fix: Update broken links",
                branch_name,
                self.base_branch.as_str(),
            )
            .body(self.create_pr_description())
            .send()
            .await
            .map_err(|e| PrError::GitHub(format!("Failed to create PR: {e}")))?;

        info!("Successfully created PR #{}", pr.number);
        match pr.html_url {
            Some(url) => Ok(url.to_string()),
            None => Err(PrError::GitHub(
                "PR created but no URL returned by GitHub API".to_string(),
            )),
        }
    }

    /// Gets the owner and repository name from the repository path.
    fn get_repo_owner_and_name(&self) -> Result<(String, String), PrError> {
        let github_url = self.repo_manager.get_github_url();
        let owner = github_url.owner();
        let repo = github_url.repo();

        Ok((owner.to_string(), repo.to_string()))
    }

    /// Creates a description for the pull request.
    fn create_pr_description(&self) -> String {
        "## 🔗 Link Fixes

This pull request was automatically generated to fix broken links in the repository.

### What was changed?
- Updated broken links to their correct destinations
- All changes were automatically detected and fixed

### How to review?
1. Check that the new links are correct and accessible
2. Verify that the changes don't break any existing functionality
3. Ensure the commit messages are descriptive

---
*This PR was generated by the [queens.ac](https://github.com/reddevilmidzy/queensac)*"
            .to_string()
    }
}

fn generate_branch_name() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("queensac-{}", now)
}

fn read_env_var(var_name: &str) -> Result<String, PrError> {
    std::env::var(var_name)
        .map_err(|_| PrError::Config(format!("Missing environment variable: {var_name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GitHubUrl;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    impl PullRequestGenerator {
        #[cfg(test)]
        fn new_for_test() -> Self {
            use crate::git::repo::TempDirGuard;
            use git2::Repository;

            let tmp = std::env::temp_dir().join(format!(
                "github_repo_temp/reddevilmidzy/kingsac_{}",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));

            let guard = TempDirGuard::new(tmp.clone()).unwrap();
            let repo = Repository::init(&tmp).unwrap();
            let github_url = GitHubUrl::new(
                "reddevilmidzy".to_string(),
                "kingsac".to_string(),
                Some("main".to_string()),
                None,
            );
            let repo_manager = RepoManager::new(&github_url, repo, guard);

            let access_token = "queensac_test_token".to_string();
            let base_branch = "main".to_string();
            let octocrab = Octocrab::builder()
                .personal_token(access_token.clone())
                .build()
                .unwrap();
            Self {
                repo_manager,
                base_branch,
                octocrab,
                access_token,
            }
        }
    }

    #[tokio::test]
    async fn test_replace_line_content() {
        let generator = PullRequestGenerator::new_for_test();

        let content = "Line 1\nLine 2 with https://old-url.com\nLine 3";
        let new_content = generator
            .replace_line_content(content, 2, "https://old-url.com", "https://new-url.com")
            .unwrap();

        assert!(new_content.contains("https://new-url.com"));
        assert!(!new_content.contains("https://old-url.com"));
    }

    #[tokio::test]
    async fn test_create_commit_message() {
        let generator = PullRequestGenerator::new_for_test();

        let changes = vec![
            FileChange {
                file_path: "test.md".to_string(),
                old_content: "old".to_string(),
                new_content: "new".to_string(),
                line_number: 3,
            },
            FileChange {
                file_path: "readme.md".to_string(),
                old_content: "old".to_string(),
                new_content: "new".to_string(),
                line_number: 10,
            },
        ];

        let message = generator.create_commit_message(&changes);

        assert!(message.contains("fix: Update broken links"));
        assert!(message.contains("test.md:3"));
        assert!(message.contains("readme.md:10"));
    }

    #[tokio::test]
    async fn test_create_pr_description() {
        let generator = PullRequestGenerator::new_for_test();

        let description = generator.create_pr_description();

        assert!(description.contains("## 🔗 Link Fixes"));
        assert!(description.contains("This pull request was automatically generated"));
        assert!(description.contains("queens.ac"));
    }

    #[test]
    fn test_generate_branch_name() {
        let branch_name = generate_branch_name();
        assert!(branch_name.starts_with("queensac-"));
    }

    #[tokio::test]
    async fn test_generate_pull_request_via_api_success() {
        // Start a mock server
        let mock_server = MockServer::start().await;

        // Create a complete mock response for PR creation matching GitHub's API
        let pr_response = r#"{
  "id": 1,
  "node_id": "PR_kwDOABC123",
  "number": 123,
  "state": "open",
  "locked": false,
  "title": "fix: Update broken links",
  "user": {
    "login": "test-user",
    "id": 1,
    "node_id": "MDQ6VXNlcjE=",
    "avatar_url": "https://avatars.githubusercontent.com/u/1?v=4",
    "gravatar_id": "",
    "url": "https://api.github.com/users/test-user",
    "html_url": "https://github.com/test-user",
    "followers_url": "https://api.github.com/users/test-user/followers",
    "following_url": "https://api.github.com/users/test-user/following{/other_user}",
    "gists_url": "https://api.github.com/users/test-user/gists{/gist_id}",
    "starred_url": "https://api.github.com/users/test-user/starred{/owner}{/repo}",
    "subscriptions_url": "https://api.github.com/users/test-user/subscriptions",
    "organizations_url": "https://api.github.com/users/test-user/orgs",
    "repos_url": "https://api.github.com/users/test-user/repos",
    "events_url": "https://api.github.com/users/test-user/events{/privacy}",
    "received_events_url": "https://api.github.com/users/test-user/received_events",
    "type": "User",
    "site_admin": false
  },
  "body": "Test body",
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z",
  "closed_at": null,
  "merged_at": null,
  "merge_commit_sha": null,
  "assignee": null,
  "assignees": [],
  "requested_reviewers": [],
  "requested_teams": [],
  "labels": [],
  "milestone": null,
  "draft": false,
  "commits_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123/commits",
  "review_comments_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123/comments",
  "review_comment_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/comments{/number}",
  "comments_url": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/123/comments",
  "statuses_url": "https://api.github.com/repos/reddevilmidzy/kingsac/statuses/abc123",
  "head": {
    "label": "reddevilmidzy:queensac-test-branch",
    "ref": "queensac-test-branch",
    "sha": "abc123def456",
    "user": {
      "login": "reddevilmidzy",
      "id": 2,
      "node_id": "MDQ6VXNlcjI=",
      "avatar_url": "https://avatars.githubusercontent.com/u/2?v=4",
      "gravatar_id": "",
      "url": "https://api.github.com/users/reddevilmidzy",
      "html_url": "https://github.com/reddevilmidzy",
      "followers_url": "https://api.github.com/users/reddevilmidzy/followers",
      "following_url": "https://api.github.com/users/reddevilmidzy/following{/other_user}",
      "gists_url": "https://api.github.com/users/reddevilmidzy/gists{/gist_id}",
      "starred_url": "https://api.github.com/users/reddevilmidzy/starred{/owner}{/repo}",
      "subscriptions_url": "https://api.github.com/users/reddevilmidzy/subscriptions",
      "organizations_url": "https://api.github.com/users/reddevilmidzy/orgs",
      "repos_url": "https://api.github.com/users/reddevilmidzy/repos",
      "events_url": "https://api.github.com/users/reddevilmidzy/events{/privacy}",
      "received_events_url": "https://api.github.com/users/reddevilmidzy/received_events",
      "type": "User",
      "site_admin": false
    },
    "repo": null
  },
  "base": {
    "label": "reddevilmidzy:main",
    "ref": "main",
    "sha": "def456abc123",
    "user": {
      "login": "reddevilmidzy",
      "id": 2,
      "node_id": "MDQ6VXNlcjI=",
      "avatar_url": "https://avatars.githubusercontent.com/u/2?v=4",
      "gravatar_id": "",
      "url": "https://api.github.com/users/reddevilmidzy",
      "html_url": "https://github.com/reddevilmidzy",
      "followers_url": "https://api.github.com/users/reddevilmidzy/followers",
      "following_url": "https://api.github.com/users/reddevilmidzy/following{/other_user}",
      "gists_url": "https://api.github.com/users/reddevilmidzy/gists{/gist_id}",
      "starred_url": "https://api.github.com/users/reddevilmidzy/starred{/owner}{/repo}",
      "subscriptions_url": "https://api.github.com/users/reddevilmidzy/subscriptions",
      "organizations_url": "https://api.github.com/users/reddevilmidzy/orgs",
      "repos_url": "https://api.github.com/users/reddevilmidzy/repos",
      "events_url": "https://api.github.com/users/reddevilmidzy/events{/privacy}",
      "received_events_url": "https://api.github.com/users/reddevilmidzy/received_events",
      "type": "User",
      "site_admin": false
    },
    "repo": null
  },
  "_links": {
    "self": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123"
    },
    "html": {
      "href": "https://github.com/reddevilmidzy/kingsac/pull/123"
    },
    "issue": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/123"
    },
    "comments": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/123/comments"
    },
    "review_comments": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123/comments"
    },
    "review_comment": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/comments{/number}"
    },
    "commits": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123/commits"
    },
    "statuses": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/statuses/abc123def456"
    }
  },
  "author_association": "OWNER",
  "auto_merge": null,
  "active_lock_reason": null,
  "merged": false,
  "mergeable": null,
  "rebaseable": null,
  "mergeable_state": "unknown",
  "merged_by": null,
  "comments": 0,
  "review_comments": 0,
  "maintainer_can_modify": false,
  "commits": 1,
  "additions": 10,
  "deletions": 5,
  "changed_files": 2,
  "url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/123",
  "html_url": "https://github.com/reddevilmidzy/kingsac/pull/123",
  "diff_url": "https://github.com/reddevilmidzy/kingsac/pull/123.diff",
  "patch_url": "https://github.com/reddevilmidzy/kingsac/pull/123.patch",
  "issue_url": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/123"
}"#;

        // Mount the mock
        Mock::given(method("POST"))
            .and(path("/repos/reddevilmidzy/kingsac/pulls"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_string(pr_response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&mock_server)
            .await;

        // Create a test generator with mock server
        let generator = PullRequestGenerator::new_for_test();

        // Override the octocrab instance to use the mock server
        let octocrab = Octocrab::builder()
            .base_uri(&mock_server.uri())
            .unwrap()
            .personal_token("test_token".to_string())
            .build()
            .unwrap();

        let generator_with_mock = PullRequestGenerator {
            repo_manager: generator.repo_manager,
            base_branch: generator.base_branch,
            octocrab,
            access_token: generator.access_token,
        };

        // Test the PR generation
        let result = generator_with_mock
            .generate_pull_request_via_api("queensac-test-branch")
            .await;

        assert!(result.is_ok());
        let pr_url = result.unwrap();
        assert_eq!(pr_url, "https://github.com/reddevilmidzy/kingsac/pull/123");
    }

    #[tokio::test]
    async fn test_generate_pull_request_via_api_no_html_url() {
        // Start a mock server
        let mock_server = MockServer::start().await;

        // Create a complete mock response without html_url
        let pr_response = r#"{
  "id": 1,
  "node_id": "PR_kwDOABC456",
  "number": 456,
  "state": "open",
  "locked": false,
  "title": "fix: Update broken links",
  "user": {
    "login": "test-user",
    "id": 1,
    "node_id": "MDQ6VXNlcjE=",
    "avatar_url": "https://avatars.githubusercontent.com/u/1?v=4",
    "gravatar_id": "",
    "url": "https://api.github.com/users/test-user",
    "html_url": "https://github.com/test-user",
    "followers_url": "https://api.github.com/users/test-user/followers",
    "following_url": "https://api.github.com/users/test-user/following{/other_user}",
    "gists_url": "https://api.github.com/users/test-user/gists{/gist_id}",
    "starred_url": "https://api.github.com/users/test-user/starred{/owner}{/repo}",
    "subscriptions_url": "https://api.github.com/users/test-user/subscriptions",
    "organizations_url": "https://api.github.com/users/test-user/orgs",
    "repos_url": "https://api.github.com/users/test-user/repos",
    "events_url": "https://api.github.com/users/test-user/events{/privacy}",
    "received_events_url": "https://api.github.com/users/test-user/received_events",
    "type": "User",
    "site_admin": false
  },
  "body": "Test body",
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z",
  "closed_at": null,
  "merged_at": null,
  "merge_commit_sha": null,
  "assignee": null,
  "assignees": [],
  "requested_reviewers": [],
  "requested_teams": [],
  "labels": [],
  "milestone": null,
  "draft": false,
  "commits_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456/commits",
  "review_comments_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456/comments",
  "review_comment_url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/comments{/number}",
  "comments_url": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/456/comments",
  "statuses_url": "https://api.github.com/repos/reddevilmidzy/kingsac/statuses/abc123",
  "head": {
    "label": "reddevilmidzy:queensac-test-branch",
    "ref": "queensac-test-branch",
    "sha": "abc123def456",
    "user": {
      "login": "reddevilmidzy",
      "id": 2,
      "node_id": "MDQ6VXNlcjI=",
      "avatar_url": "https://avatars.githubusercontent.com/u/2?v=4",
      "gravatar_id": "",
      "url": "https://api.github.com/users/reddevilmidzy",
      "html_url": "https://github.com/reddevilmidzy",
      "followers_url": "https://api.github.com/users/reddevilmidzy/followers",
      "following_url": "https://api.github.com/users/reddevilmidzy/following{/other_user}",
      "gists_url": "https://api.github.com/users/reddevilmidzy/gists{/gist_id}",
      "starred_url": "https://api.github.com/users/reddevilmidzy/starred{/owner}{/repo}",
      "subscriptions_url": "https://api.github.com/users/reddevilmidzy/subscriptions",
      "organizations_url": "https://api.github.com/users/reddevilmidzy/orgs",
      "repos_url": "https://api.github.com/users/reddevilmidzy/repos",
      "events_url": "https://api.github.com/users/reddevilmidzy/events{/privacy}",
      "received_events_url": "https://api.github.com/users/reddevilmidzy/received_events",
      "type": "User",
      "site_admin": false
    },
    "repo": null
  },
  "base": {
    "label": "reddevilmidzy:main",
    "ref": "main",
    "sha": "def456abc123",
    "user": {
      "login": "reddevilmidzy",
      "id": 2,
      "node_id": "MDQ6VXNlcjI=",
      "avatar_url": "https://avatars.githubusercontent.com/u/2?v=4",
      "gravatar_id": "",
      "url": "https://api.github.com/users/reddevilmidzy",
      "html_url": "https://github.com/reddevilmidzy",
      "followers_url": "https://api.github.com/users/reddevilmidzy/followers",
      "following_url": "https://api.github.com/users/reddevilmidzy/following{/other_user}",
      "gists_url": "https://api.github.com/users/reddevilmidzy/gists{/gist_id}",
      "starred_url": "https://api.github.com/users/reddevilmidzy/starred{/owner}{/repo}",
      "subscriptions_url": "https://api.github.com/users/reddevilmidzy/subscriptions",
      "organizations_url": "https://api.github.com/users/reddevilmidzy/orgs",
      "repos_url": "https://api.github.com/users/reddevilmidzy/repos",
      "events_url": "https://api.github.com/users/reddevilmidzy/events{/privacy}",
      "received_events_url": "https://api.github.com/users/reddevilmidzy/received_events",
      "type": "User",
      "site_admin": false
    },
    "repo": null
  },
  "_links": {
    "self": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456"
    },
    "html": {
      "href": "https://github.com/reddevilmidzy/kingsac/pull/456"
    },
    "issue": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/456"
    },
    "comments": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/456/comments"
    },
    "review_comments": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456/comments"
    },
    "review_comment": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/comments{/number}"
    },
    "commits": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456/commits"
    },
    "statuses": {
      "href": "https://api.github.com/repos/reddevilmidzy/kingsac/statuses/abc123def456"
    }
  },
  "author_association": "OWNER",
  "auto_merge": null,
  "active_lock_reason": null,
  "merged": false,
  "mergeable": null,
  "rebaseable": null,
  "mergeable_state": "unknown",
  "merged_by": null,
  "comments": 0,
  "review_comments": 0,
  "maintainer_can_modify": false,
  "commits": 1,
  "additions": 10,
  "deletions": 5,
  "changed_files": 2,
  "url": "https://api.github.com/repos/reddevilmidzy/kingsac/pulls/456",
  "diff_url": "https://github.com/reddevilmidzy/kingsac/pull/456.diff",
  "patch_url": "https://github.com/reddevilmidzy/kingsac/pull/456.patch",
  "issue_url": "https://api.github.com/repos/reddevilmidzy/kingsac/issues/456"
}"#;

        // Mount the mock
        Mock::given(method("POST"))
            .and(path("/repos/reddevilmidzy/kingsac/pulls"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_string(pr_response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&mock_server)
            .await;

        // Create a test generator with mock server
        let generator = PullRequestGenerator::new_for_test();

        let octocrab = Octocrab::builder()
            .base_uri(&mock_server.uri())
            .unwrap()
            .personal_token("test_token".to_string())
            .build()
            .unwrap();

        let generator_with_mock = PullRequestGenerator {
            repo_manager: generator.repo_manager,
            base_branch: generator.base_branch,
            octocrab,
            access_token: generator.access_token,
        };

        // Test the PR generation
        let result = generator_with_mock
            .generate_pull_request_via_api("queensac-test-branch")
            .await;

        assert!(result.is_err());
        if let Err(PrError::GitHub(msg)) = result {
            assert!(msg.contains("no URL returned"));
        } else {
            panic!("Expected GitHub error");
        }
    }

    #[tokio::test]
    async fn test_generate_pull_request_via_api_failure() {
        // Start a mock server
        let mock_server = MockServer::start().await;

        // Create a mock error response
        let error_response = r#"{
            "message": "Validation Failed",
            "errors": [{"message": "A pull request already exists"}]
        }"#;

        // Mount the mock with error status
        Mock::given(method("POST"))
            .and(path("/repos/reddevilmidzy/kingsac/pulls"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_string(error_response)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&mock_server)
            .await;

        // Create a test generator with mock server
        let generator = PullRequestGenerator::new_for_test();

        let octocrab = Octocrab::builder()
            .base_uri(&mock_server.uri())
            .unwrap()
            .personal_token("test_token".to_string())
            .build()
            .unwrap();

        let generator_with_mock = PullRequestGenerator {
            repo_manager: generator.repo_manager,
            base_branch: generator.base_branch,
            octocrab,
            access_token: generator.access_token,
        };

        // Test the PR generation
        let result = generator_with_mock
            .generate_pull_request_via_api("queensac-test-branch")
            .await;

        assert!(result.is_err());
        if let Err(PrError::GitHub(msg)) = result {
            assert!(msg.contains("Failed to create PR"));
        } else {
            panic!("Expected GitHub error");
        }
    }

    #[tokio::test]
    async fn test_create_fix_pr_with_no_changes() {
        use std::fs;

        let generator = PullRequestGenerator::new_for_test();

        // Initialize the repository with an initial commit
        let test_file = generator.repo_manager.get_repo_path().join("README.md");
        fs::write(&test_file, "# Test Repository").unwrap();

        // Create initial commit
        let repo = generator.repo_manager.get_repo();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        index.write().unwrap();

        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        // Create initial commit with no parent
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )
        .unwrap();

        // Create an empty fixes vector - this simulates the case where no changes are needed
        let fixes = vec![];

        // Test that create_fix_pr returns an error when there are no changes
        let result = generator.create_fix_pr(fixes).await;

        assert!(result.is_err());
        if let Err(PrError::Config(msg)) = result {
            assert!(msg.contains("No changes to create PR"));
        } else {
            panic!("Expected Config error for no changes, got: {:?}", result);
        }
    }
}
