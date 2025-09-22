use queensac::{KoreanTime, stream_link_checks};
use tracing::{Level, error, info};

fn main() {
    // Initialize tracing subscriber
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

    info!("Starting queensac service...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    rt.block_on(async {
        if let Err(e) = stream_link_checks(
            "https://github.com/reddevilmidzy/queensac".to_string(),
            None,
        )
        .await
        {
            error!("Failed to stream link checks: {}", e);
        }
    });
}
