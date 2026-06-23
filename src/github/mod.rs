pub(crate) mod types;
pub(crate) mod client;
pub(crate) mod domain;
pub(crate) mod streak;
#[cfg(test)]
pub(crate) mod tests;

pub use client::RealHttpFetch;
pub use domain::{get_commits_list, get_streak_info};
