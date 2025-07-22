use std::fmt;

use chrono::{FixedOffset, Utc};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

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
/// use queensac::KoreanTime;
/// use tracing_subscriber::fmt::time::FormatTime;
///
/// let formatter = KoreanTime;
/// // Will output something like: 2024-02-14T15:30:45.123+09:00
/// ```
pub struct KoreanTime;

impl FormatTime for KoreanTime {
    fn format_time(&self, w: &mut Writer<'_>) -> Result<(), fmt::Error> {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(KST_OFFSET).unwrap());
        write!(w, "{}", now.format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}
