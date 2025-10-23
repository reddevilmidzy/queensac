use clap::Parser;
use queensac::{
    FileChange, GitHubAppConfig, GitHubUrl, InvalidLinkInfo, KoreanTime, PullRequestGenerator,
    RepoManager, check_links,
};
use tracing::{Level, error, info};

#[derive(Debug, Parser)]
#[command(name = "queensac", about = "Link checker for a GitHub repo")]
struct Args {
    #[arg(long = "repo", short = 'r', help = "GitHub repository URL")]
    repo: String,
    #[arg(long = "branch", short = 'b', help = "Target branch to check")]
    branch: Option<String>,
    #[arg(
        long = "dry-run",
        short = 'd',
        default_value_t = false,
        help = "Dry run mode"
    )]
    dry_run: bool,
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_level(true)
        .with_ansi(true)
        .with_timer(KoreanTime)
        .pretty()
        .init();

    let args = Args::parse();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // TODO: refactor this to use a more idiomatic way
    rt.block_on(async {
        let github_url = GitHubUrl::parse(&args.repo).unwrap_or_else(|| {
            error!("Failed to parse GitHub URL: {}", args.repo);
            std::process::exit(1);
        });
        let repo_manager = RepoManager::from(&github_url).unwrap_or_else(|e| {
            error!("Failed to clone repository: {}", e);
            std::process::exit(1);
        });
        let result = check_links(&repo_manager).await;
        match result {
            Ok(invalid_links) => {
                if invalid_links.is_empty() {
                    info!("All links are valid");
                    return;
                }
                if args.dry_run {
                    info!("Dry run mode, skipping pull request creation");
                    return;
                }

                let app_config = GitHubAppConfig::from_env().unwrap_or_else(|e| {
                    error!("GitHub App configuration not found: {}. Please set GITHUB_APP_ID and GITHUB_APP_PRIVATE_KEY environment variables.", e);
                    std::process::exit(1);
                });

                // TODO find base branch from repository. 
                let base_branch = args.branch.unwrap_or("main".to_string());

                let pr_generator = PullRequestGenerator::new(repo_manager, app_config, base_branch).await.unwrap_or_else(|e| {
                    error!("Failed to create PR generator: {}", e);
                    std::process::exit(1);
                });
                let fixes = find_valid_links(invalid_links).await;
                let pr_url = pr_generator.create_fix_pr(fixes).await;
                match pr_url {
                    Ok(url) => {
                        info!("Successfully created PR: {}", url);
                    }
                    Err(e) => {
                        error!("Failed to create PR: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                error!("Failed to check links: {}", e);
                std::process::exit(1);
            }
        }
    });
}

async fn find_valid_links(invalid_links: Vec<InvalidLinkInfo>) -> Vec<FileChange> {
    let mut fixes = Vec::new();

    // TODO: Replace with actual valid link
    for invalid_link in invalid_links {
        fixes.push(FileChange {
            file_path: invalid_link.file_path,
            old_content: invalid_link.url,
            new_content: "https://correct_url.com".to_string(),
            line_number: invalid_link.line_number,
        });
    }

    fixes
}
