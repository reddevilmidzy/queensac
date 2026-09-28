use std::{env, time};

use clap::Parser;
use git2::Repository;

#[derive(Debug, Parser, Clone)]
#[command(name = "queensac", about = "Link checker for a GitHub repo")]
struct Args {
    #[arg(long = "repo", short = 'r', help = "GitHub repository URL")]
    repo: String,
    #[arg(long = "branch", short = 'b', help = "Target branch to check")]
    branch: Option<String>,
}

fn main() {
    let args = Args::parse();
    // let temp_dir = env::temp_dir().join(format!(
    //     "github_repo_temp/{}/{}",
    //     args.repo.clone(),
    //     time::SystemTime::now()
    //         .duration_since(time::UNIX_EPOCH)
    //         .unwrap()
    //         .as_nanos()
    // ));
    let temp_dir = "/gh-reddeilmidzy/queensac/github_test";
    eprintln!("{:?}", args);
    let repo = match Repository::clone(&args.repo, temp_dir) {
        Ok(repo) => repo,
        Err(e) => panic!("failed to clone: {}", e),
    };
}
