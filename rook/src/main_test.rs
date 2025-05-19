#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_repository_url_deserialization() {
        // Valid URLs
        let valid_url = json!({ "url": "https://github.com/owner/repo" });
        assert!(serde_json::from_value::<RepositoryURL>(valid_url).is_ok());

        let valid_url = json!({ "url": "https://github.com/rust-lang/rust" });
        assert!(serde_json::from_value::<RepositoryURL>(valid_url).is_ok());

        // Invalid URLs
        let invalid_url = json!({ "url": "https://gitlab.com/owner/repo" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());

        let invalid_url = json!({ "url": "https://github.com/" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());

        let invalid_url = json!({ "url": "https://github.com/owner" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());

        let invalid_url = json!({ "url": "https://github.com/owner/" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());

        let invalid_url = json!({ "url": "http://github.com/owner/repo" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());

        let invalid_url = json!({ "url": "https://github.com//repo" });
        assert!(serde_json::from_value::<RepositoryURL>(invalid_url).is_err());
    }
}
