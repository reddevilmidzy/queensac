use crate::domain::{NewSubscriber, RepositoryURL};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct CheckRequest {
    pub subscriber: NewSubscriber,
}

#[derive(Deserialize, Serialize)]
pub struct CancelRequest {
    pub subscriber: NewSubscriber,
}

#[derive(Deserialize)]
pub struct StreamRequest {
    pub repo_url: RepositoryURL,
    pub branch: Option<String>,
}
