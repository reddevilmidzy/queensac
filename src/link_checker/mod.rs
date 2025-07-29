pub mod link;
pub mod sse;

pub use link::{LinkCheckResult, check_link};
pub use sse::stream_link_checks;
