use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub size: usize,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitItem {
    pub repo: String,
    pub additions: usize,
    pub deletions: usize,
    pub commit_url: String,
    pub committed_date: DateTime<Utc>,
    pub oid: String,
    pub message_headline: String,
    pub message_body: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitsListStats {
    pub total_additions: usize,
    pub total_deletions: usize,
    pub total_commits: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitsListResponse {
    pub commits: Vec<CommitItem>,
    pub languages: Vec<Language>,
    pub stats: CommitsListStats,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StreakInfo {
    pub current_streak: usize,
    pub highest_streak: usize,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedAuth {
    pub username: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryParams {
    pub username: String,
    pub limit: usize,
    pub history_limit: usize,
}
