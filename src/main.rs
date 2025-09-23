use clap::Parser;
use queensac::{KoreanTime, stream_link_checks};
use tracing::{Level, error};

#[derive(Debug, Parser)]
#[command(name = "queensac", about = "Link checker for a GitHub repo")]
struct Args {
    #[arg(long = "repo", short = 'r', help = "GitHub repository URL")]
    repo: String,
    #[arg(long = "branch", short = 'b', help = "Target branch to check")]
    branch: Option<String>,
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
        if let Err(e) = stream_link_checks(args.repo, args.branch).await {
            error!("Failed to stream link checks: {}", e);
        }
    });
}
