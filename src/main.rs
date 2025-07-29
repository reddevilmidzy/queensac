use queensac::configuration::get_configuration;
use queensac::{Application, KoreanTime};

use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    FmtSubscriber::builder()
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
    let configuration = get_configuration().expect("Failed to read configuration.");

    let app = Application::build(configuration)
        .await
        .expect("Failed to build application.");

    Ok(app.router.into())
}
