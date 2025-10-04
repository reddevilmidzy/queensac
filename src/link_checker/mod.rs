mod checker;
mod service;

pub use checker::{LinkCheckResult, check_link};
pub use service::{InvalidLinkInfo, LinkCheckEvent, check_links};
