mod link;
mod sse;

pub use link::{LinkCheckResult, check_link};
pub use sse::{LinkCheckEvent, check_links};
