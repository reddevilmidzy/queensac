use crate::RepoManager;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{error, info};

/// Represents errors that can occur during pull request generation.
#[derive(Debug, Error)]
pub enum PrError {
    #[error("Git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("GitHub API error: {0}")]
    GitHub(String),
    #[error("File operation failed: {0}")]
    File(String),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    Config(String),
}

#[derive(Debug)]
/// Represents a file change to be included in a pull request.
pub struct FileChange {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub line_number: i32,
}

#[derive(Debug, Serialize)]
/// Structure for creating a GitHub pull request via API.
struct GitHubPullRequest {
    title: String,
    body: String,
    head: String,
    base: String,
}

#[derive(Debug, Deserialize)]
/// Structure for parsing the response from GitHub pull request API.
struct GitHubPullRequestResponse {
    html_url: String,
    number: u32,
}

/// Generates pull requests for link fixes in a repository.
pub struct PullRequestGenerator {
    repo_manager: RepoManager,
    github_token: String,
    base_branch: String,
    feature_branch: String,
    author_name: String,
    author_email: String,
    http_client: Client,
}

impl PullRequestGenerator {
    /// Constructs a new `PullRequestGenerator` with the specified repository manager, GitHub credentials, branch names, author information, and HTTP client.
    ///
    /// Initializes the generator for creating pull requests that fix broken links in a repository.
    pub fn new(
        repo_manager: RepoManager,
        github_token: String,
        base_branch: String,
        feature_branch: String,
        author_name: String,
        author_email: String,
        http_client: Client,
    ) -> Self {
        Self {
            repo_manager,
            github_token,
            base_branch,
            feature_branch,
            author_name,
            author_email,
            http_client,
        }
    }

    /// Creates a pull request with link fixes.
    ///
    /// # Arguments
    /// * `fixes` - The list of file changes to apply
    pub async fn create_fix_pr(&self, fixes: Vec<FileChange>) -> Result<String, PrError> {
        self.create_feature_branch().await?;

        let changes = self.apply_fixes(fixes).await?;

        self.commit_changes(&changes).await?;
        self.push_to_remote().await?;

        let pr_url = self.create_pull_request_via_api().await?;

        info!("Successfully created PR: {}", pr_url);
        Ok(pr_url)
    }

    /// Creates a new feature branch from the current branch.
    async fn create_feature_branch(&self) -> Result<(), PrError> {
        self.repo_manager
            .create_branch(&self.feature_branch)
            .await?;
        self.repo_manager
            .checkout_branch(&self.feature_branch)
            .await?;

        info!(
            "Successfully created and checked out branch: {}",
            self.feature_branch
        );
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
                fix.line_number as usize,
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

        let commit_message = self.create_commit_message(changes);

        self.repo_manager
            .commit(&commit_message, &self.author_name, &self.author_email)
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
    async fn push_to_remote(&self) -> Result<(), PrError> {
        info!("Pushing branch {} to remote", self.feature_branch);

        self.repo_manager
            .push("origin", &self.feature_branch)
            .await?;

        info!("Successfully pushed branch to remote");
        Ok(())
    }

    /// Creates a pull request via the GitHub API.
    pub async fn create_pull_request_via_api(&self) -> Result<String, PrError> {
        info!("Creating pull request via GitHub API");

        let repo_url = self.get_repo_url()?;
        let api_url = format!("{repo_url}/pulls");

        let pr_data = GitHubPullRequest {
            title: "fix: Update broken links".to_string(),
            body: self.create_pr_description(),
            head: self.feature_branch.clone(),
            base: self.base_branch.clone(),
        };

        let response = self
            .http_client
            .post(&api_url)
            .header("Authorization", format!("token {}", self.github_token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "queensac")
            .json(&pr_data)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(PrError::GitHub(format!(
                "Failed to create PR: {status} - {error_text}"
            )));
        }

        let pr_response: GitHubPullRequestResponse = response.json().await?;

        info!("Successfully created PR #{}", pr_response.number);
        Ok(pr_response.html_url)
    }

    /// Gets the GitHub API URL for the repository.
    fn get_repo_url(&self) -> Result<String, PrError> {
        let repo_path = self.repo_manager.get_repo_path();

        let path_components: Vec<&str> = repo_path.to_str().unwrap().split('/').collect();
        if path_components.len() < 2 {
            return Err(PrError::Config("Invalid repository path".to_string()));
        }

        let owner = path_components[path_components.len() - 2];
        let repo = path_components[path_components.len() - 1];

        Ok(format!("https://api.github.com/repos/{owner}/{repo}"))
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
*This PR was generated by the queens.ac*"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDirGuard;
    use std::env;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    async fn create_test_pr_generator() -> PullRequestGenerator {
        let (_temp_guard, repo_manager) = create_test_repo().await;
        PullRequestGenerator::new(
            repo_manager,
            "test_token".to_string(),
            "main".to_string(),
            "fix-links".to_string(),
            "Test User".to_string(),
            "test@example.com".to_string(),
            Client::new(),
        )
    }

    async fn create_test_repo() -> (TempDirGuard, RepoManager) {
        let temp_dir = env::temp_dir().join(format!(
            "test_repo_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let temp_dir_guard = TempDirGuard::new(temp_dir.clone()).unwrap();
        let repo_path = temp_dir_guard.get_path();

        // Initialize a git repository
        let repo = git2::Repository::init(repo_path).unwrap();

        // Create a test file
        let test_file = repo_path.join("test.md");
        tokio::fs::write(
            &test_file,
            "# Test\n\nThis is a [broken link](https://broken-url.com)\n",
        )
        .await
        .unwrap();

        // Add and commit the file
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.md")).unwrap();
        index.write().unwrap();

        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        repo.commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )
        .unwrap();

        let repo_manager =
            RepoManager::clone_repo(&repo_path.to_str().unwrap(), Some("main")).unwrap();

        (temp_dir_guard, repo_manager)
    }

    #[tokio::test]
    async fn test_replace_line_content() {
        let generator = create_test_pr_generator();

        let content = "Line 1\nLine 2 with https://old-url.com\nLine 3";
        let new_content = generator
            .await
            .replace_line_content(content, 2, "https://old-url.com", "https://new-url.com")
            .unwrap();

        assert!(new_content.contains("https://new-url.com"));
        assert!(!new_content.contains("https://old-url.com"));
    }

    #[tokio::test]
    async fn test_create_commit_message() {
        let generator = create_test_pr_generator();

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

        let message = generator.await.create_commit_message(&changes);

        assert!(message.contains("fix: Update broken links"));
        assert!(message.contains("test.md:3"));
        assert!(message.contains("readme.md:10"));
    }

    #[tokio::test]
    async fn test_create_pull_request_via_api_success() {
        let mock_server = MockServer::start().await;

        let generator = create_test_pr_generator().await;

        Mock::given(method("POST"))
            .and(path("/repos/test-owner/test-repo/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "html_url": "https://github.com/test-owner/test-repo/pull/123",
                "number": 123
            })))
            .mount(&mock_server)
            .await;

        let mock_url = format!("{}/repos/test-owner/test-repo", mock_server.uri());

        let api_url = format!("{}/pulls", mock_url);
        let pr_data = GitHubPullRequest {
            title: "fix: Update broken links".to_string(),
            body: generator.create_pr_description(),
            head: generator.feature_branch.clone(),
            base: generator.base_branch.clone(),
        };

        let response = generator
            .http_client
            .post(&api_url)
            .header("Authorization", format!("token {}", generator.github_token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "queensac-link-fixer")
            .json(&pr_data)
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        let pr_response: GitHubPullRequestResponse = response.json().await.unwrap();
        assert_eq!(
            pr_response.html_url,
            "https://github.com/test-owner/test-repo/pull/123"
        );
    }

    #[tokio::test]
    async fn test_create_pull_request_via_api_failure() {
        let mock_server = MockServer::start().await;

        let generator = create_test_pr_generator().await;

        Mock::given(method("POST"))
            .and(path("/repos/test-owner/test-repo/pulls"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Bad credentials",
                "documentation_url": "https://docs.github.com/rest"
            })))
            .mount(&mock_server)
            .await;

        let mock_url = format!("{}/repos/test-owner/test-repo", mock_server.uri());
        let api_url = format!("{}/pulls", mock_url);
        let pr_data = GitHubPullRequest {
            title: "fix: Update broken links".to_string(),
            body: generator.create_pr_description(),
            head: generator.feature_branch.clone(),
            base: generator.base_branch.clone(),
        };

        let response = generator
            .http_client
            .post(&api_url)
            .header("Authorization", format!("token {}", generator.github_token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "queensac-link-fixer")
            .json(&pr_data)
            .send()
            .await
            .unwrap();

        assert!(!response.status().is_success());
        assert_eq!(response.status().as_u16(), 401);

        let error_text = response.text().await.unwrap();
        assert!(error_text.contains("Bad credentials"));
    }

    #[tokio::test]
    async fn test_create_fix_pr_integration() {
        let mock_server = MockServer::start().await;

        let generator = create_test_pr_generator().await;

        Mock::given(method("POST"))
            .and(path("/repos/test-owner/test-repo/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "html_url": "https://github.com/test-owner/test-repo/pull/456",
                "number": 456
            })))
            .mount(&mock_server)
            .await;

        // Create test fixes
        let fixes = vec![FileChange {
            file_path: "test.md".to_string(),
            line_number: 3,
            old_content: "https://broken-url.com".to_string(),
            new_content: "https://working-url.com".to_string(),
        }];

        let result = generator.create_fix_pr(fixes).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_pr_description() {
        let generator = create_test_pr_generator().await;

        let description = generator.create_pr_description();

        assert!(description.contains("## 🔗 Link Fixes"));
        assert!(description.contains("This pull request was automatically generated"));
        assert!(description.contains("queens.ac"));
    }
}
