use crate::domain::RepositoryURL;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct StreamRequest {
    pub repo_url: RepositoryURL,
    pub branch: Option<String>,
}
