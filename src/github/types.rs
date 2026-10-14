use chrono::{DateTime, Utc};
use serde::Deserialize;

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
#[serde(rename_all = "camelCase")]
pub struct GqlCost {
    pub actual_cost: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GqlExtensions {
    pub cost: GqlCost,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<serde_json::Value>>,
    pub extensions: Option<GqlExtensions>,
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
pub struct StreakUserData {
    pub user: StreakUser,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StreakUser {
    pub contributions_collection: ContributionsCollection,
}
