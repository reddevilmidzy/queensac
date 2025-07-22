use serde::{Deserialize, Serialize};

const GITHUB_BASE_URL: &str = "https://github.com/";
const GITHUB_URL_FORMAT: &str = "https://github.com/{owner}/{repo_name}";

/// Represents a GitHub repository URL.
///
/// This struct ensures that the URL is valid and follows the format
/// `https://github.com/{owner}/{repo_name}`. It includes validation logic
/// to enforce this format.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct RepositoryURL {
    /// The URL of the repository.
    url: String,
}

impl RepositoryURL {
    /// Creates a new `RepositoryURL` instance.
    ///
    /// # Arguments
    ///
    /// * `url` - The GitHub repository URL to validate and store.
    ///
    /// # Returns
    ///
    /// Returns `Ok(RepositoryURL)` if the URL is valid, or `Err(String)` if the URL is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use queensac::domain::RepositoryURL;
    ///
    /// let url = RepositoryURL::new("https://github.com/owner/repo").unwrap();
    /// ```
    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        let repo = RepositoryURL { url: url.into() };
        repo.validate()?;
        Ok(repo)
    }

    /// Returns a reference to the repository URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with(GITHUB_BASE_URL) {
            return Err(format!("URL must start with {GITHUB_BASE_URL}"));
        }
        let parts: Vec<&str> = self
            .url
            .trim_start_matches(GITHUB_BASE_URL)
            .split('/')
            .collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(format!("URL must be in format {GITHUB_URL_FORMAT}"));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RepositoryURL {
    /// Custom deserialization logic for `RepositoryURL`.
    ///
    /// This implementation ensures that the URL is validated during
    /// deserialization. If the URL is invalid, an error is returned.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let url = String::deserialize(deserializer)?;
        RepositoryURL::new(url).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_url_creation() {
        // Valid URLs
        assert!(RepositoryURL::new("https://github.com/owner/repo").is_ok());
        assert!(RepositoryURL::new("https://github.com/rust-lang/rust").is_ok());

        // Invalid URLs
        assert!(RepositoryURL::new("https://gitlab.com/owner/repo").is_err());
        assert!(RepositoryURL::new("https://github.com/").is_err());
        assert!(RepositoryURL::new("https://github.com/owner").is_err());
        assert!(RepositoryURL::new("https://github.com/owner/").is_err());
        assert!(RepositoryURL::new("http://github.com/owner/repo").is_err());
        assert!(RepositoryURL::new("https://github.com//repo").is_err());
    }

    #[test]
    fn test_repository_url_deserialization() {
        // Valid URLs
        assert!(serde_json::from_str::<RepositoryURL>("\"https://github.com/owner/repo\"").is_ok());
        assert!(
            serde_json::from_str::<RepositoryURL>("\"https://github.com/rust-lang/rust\"").is_ok()
        );

        // Invalid URLs
        assert!(
            serde_json::from_str::<RepositoryURL>("\"https://gitlab.com/owner/repo\"").is_err()
        );
        assert!(serde_json::from_str::<RepositoryURL>("\"https://github.com/\"").is_err());
        assert!(serde_json::from_str::<RepositoryURL>("\"https://github.com/owner\"").is_err());
        assert!(serde_json::from_str::<RepositoryURL>("\"https://github.com/owner/\"").is_err());
        assert!(serde_json::from_str::<RepositoryURL>("\"http://github.com/owner/repo\"").is_err());
        assert!(serde_json::from_str::<RepositoryURL>("\"https://github.com//repo\"").is_err());
    }
}
