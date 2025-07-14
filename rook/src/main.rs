// todo 어떤게 깔끔한 import 구조인지, 조사하기. 베스트 쁘락띠쓰 찾기.
use queensac::Application;
use queensac::configuration::get_configuration_with_secrets;

use chrono::{FixedOffset, Utc};
use shuttle_runtime::SecretStore;
use sqlx::PgPool;
use std::fmt;
use tracing::{Level, info};
use tracing_subscriber::{FmtSubscriber, fmt::format::Writer, fmt::time::FormatTime};

#[shuttle_runtime::main]
async fn main(
    #[shuttle_shared_db::Postgres] pool: PgPool,
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> shuttle_axum::ShuttleAxum {
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
    let configuration =
        get_configuration_with_secrets(Some(&secrets)).expect("Failed to read configuration.");

    let app = Application::build(configuration, pool)
        .await
        .expect("Failed to build application.");

    Ok(app.router.into())
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
/// # Example
/// ```
/// use tracing_subscriber::fmt::time::FormatTime;
///
/// let formatter = KoreanTime;
/// // Will output something like: 2024-02-14T15:30:45.123+09:00
/// ```
struct KoreanTime;

impl FormatTime for KoreanTime {
    fn format_time(&self, w: &mut Writer<'_>) -> Result<(), fmt::Error> {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(KST_OFFSET).unwrap());
        write!(w, "{}", now.format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}
