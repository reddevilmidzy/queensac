mod git;
mod link;
mod schedule;

use crate::schedule::check_repository_links;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let repo_url = "https://github.com/reddevilmidzy/redddy-action";
    let interval_duration = Duration::from_secs(60);
    check_repository_links(repo_url, interval_duration).await;
}

#[test]
fn fail_test() {
    assert!(false);
}
