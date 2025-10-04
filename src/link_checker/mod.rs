mod link;
mod service;

pub use link::{LinkCheckResult, check_link};
pub use service::{InvalidLinkInfo, LinkCheckEvent, check_links};
