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
    /// * `GITHUB_APP_ID` - The GitHub App ID
    /// * `GITHUB_APP_PRIVATE_KEY` - The GitHub App private key (PEM format)
    pub fn from_env() -> Result<Self, PrError> {
        let app_id = read_env_var("GITHUB_APP_ID")?
            .parse::<u64>()
            .map_err(|e| PrError::Config(format!("Invalid GITHUB_APP_ID: {e}")))?;

        let private_key = read_env_var("GITHUB_APP_PRIVATE_KEY")?;

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
        .unwrap()
        .as_secs();
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
}
