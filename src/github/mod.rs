pub mod types;
pub mod client;
pub(crate) mod domain;
pub(crate) mod streak;
#[cfg(test)]
pub(crate) mod tests;

pub use client::{execute_graphql, HttpFetch, HttpResponse, RealHttpFetch};
pub use domain::{get_commits_list, get_streak_info, GitHubCtx};
