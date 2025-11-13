use chrono::{FixedOffset, Utc};
use clap::Parser;
use queensac::{
    FileChange, GitHubAppConfig, GitHubUrl, InvalidLinkInfo, PullRequestGenerator, RepoManager,
    check_links,
};
use std::fmt;
use tracing::{Level, error, info};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

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
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
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
                    error!("GitHub App configuration not found: {}. Please set QUEENSAC_APP_ID and QUEENSAC_APP_PRIVATE_KEY environment variables.", e);
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

    for invalid_link in invalid_links {
        if let Some(url) = invalid_link.collect_link {
            fixes.push(FileChange {
                file_path: invalid_link.file_path,
                old_content: invalid_link.url,
                new_content: url,
                line_number: invalid_link.line_number,
            });
        }
    }

    fixes
}

/// The offset in seconds for Korean Standard Time (UTC+9)
const KST_OFFSET: i32 = 9 * 3600;

/// A time formatter that outputs timestamps in Korean Standard Time (KST)
///
/// This struct implements the `FormatTime` trait to format timestamps in KST
/// with millisecond precision and timezone offset.
///
/// # Format
/// The output format is: `YYYY-MM-DDThh:mm:ss.sss+09:00`
///
/// It is used internally to format log timestamps in KST.
struct KoreanTime;

impl FormatTime for KoreanTime {
    fn format_time(&self, w: &mut Writer<'_>) -> Result<(), fmt::Error> {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(KST_OFFSET).unwrap());
        write!(w, "{}", now.format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}
