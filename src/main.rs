use clap::Parser;
use queensac::{KoreanTime, check_links};
use tracing::{Level, error};

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

    rt.block_on(async {
        if args.dry_run {
            let result = check_links(args.repo, args.branch).await;
            if let Ok(_invalid_links) = result {
                if !args.dry_run {
                    todo!("Create pull request");
                }
            } else if let Err(e) = result {
                error!("Failed to stream link checks: {}", e);
            }
        }
    });
}
