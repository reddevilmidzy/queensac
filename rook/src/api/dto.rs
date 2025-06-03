use crate::domain::{NewSubscriber, RepositoryURL};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CheckRequest {
    pub subscriber: NewSubscriber,
    pub interval_secs: Option<u64>,
}

#[derive(Deserialize)]
pub struct CancelRequest {
    pub subscriber: NewSubscriber,
}

#[derive(Deserialize)]
pub struct StreamRequest {
    pub repo_url: RepositoryURL,
    pub branch: Option<String>,
}
