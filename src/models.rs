use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// --- DTOs (Outgoing API Payloads) ---

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub size: usize,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ParentCommit {
    pub additions: usize,
    pub deletions: usize,
    pub commit_url: String,
    pub committed_date: DateTime<Utc>,
    pub message_headline: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MostRecentCommit {
    pub repo: String,
    pub additions: usize,
    pub deletions: usize,
    pub commit_url: String,
    pub committed_date: DateTime<Utc>,
    pub oid: String,
    pub message_headline: String,
    pub message_body: String,
    pub languages: Vec<Language>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_commits: Option<Vec<ParentCommit>>,
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

// --- GitHub API Models (Incoming JSON) ---

#[derive(Serialize, Debug)]
pub struct GraphQLRequest {
    pub query: String,
    pub variables: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GitLanguageNode {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GitLanguageEdge {
    pub size: usize,
    pub node: GitLanguageNode,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GitLanguages {
    pub edges: Option<Vec<GitLanguageEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthorUser {
    pub login: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CommitAuthor {
    pub user: Option<AuthorUser>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GqlCommit {
    pub abbreviated_oid: String,
    pub additions: usize,
    pub deletions: usize,
    pub commit_url: String,
    pub committed_date: DateTime<Utc>,
    pub message_headline: String,
    pub message_body: String,
    pub author: Option<CommitAuthor>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GqlCommitEdge {
    pub node: GqlCommit,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GqlHistory {
    pub edges: Option<Vec<GqlCommitEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GqlTargetCommit {
    pub history: GqlHistory,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DefaultBranchRef {
    pub target: Option<GqlTargetCommit>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryNode {
    pub name_with_owner: String,
    pub languages: Option<GitLanguages>,
    pub default_branch_ref: Option<DefaultBranchRef>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RepositoriesConnection {
    pub nodes: Option<Vec<RepositoryNode>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UserRepos {
    pub repositories: RepositoriesConnection,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ReposResponseData {
    pub user: Option<UserRepos>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ReposGraphQLResponse {
    pub data: Option<ReposResponseData>,
    pub errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDay {
    pub contribution_count: usize,
    pub date: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContributionWeek {
    pub contribution_days: Vec<ContributionDay>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContributionCalendar {
    pub weeks: Vec<ContributionWeek>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContributionsCollection {
    pub contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserContributions {
    pub contributions_collection: ContributionsCollection,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StreakResponseData {
    pub user: Option<UserContributions>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StreakGraphQLResponse {
    pub data: Option<StreakResponseData>,
    pub errors: Option<Vec<serde_json::Value>>,
}
