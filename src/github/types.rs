use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GitLanguageNode {
    pub(crate) name: String,
    pub(crate) color: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GitLanguageEdge {
    pub(crate) size: usize,
    pub(crate) node: GitLanguageNode,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GitLanguages {
    pub(crate) edges: Option<Vec<GitLanguageEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct AuthorUser {
    pub(crate) login: String,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct CommitAuthor {
    pub(crate) user: Option<AuthorUser>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GqlCommit {
    pub(crate) abbreviated_oid: String,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) commit_url: String,
    pub(crate) committed_date: DateTime<Utc>,
    pub(crate) message_headline: String,
    pub(crate) message_body: String,
    pub(crate) author: Option<CommitAuthor>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GqlCommitEdge {
    pub(crate) node: GqlCommit,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GqlHistory {
    pub(crate) edges: Option<Vec<GqlCommitEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GqlTargetCommit {
    pub(crate) history: GqlHistory,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct DefaultBranchRef {
    pub(crate) target: Option<GqlTargetCommit>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositoryNode {
    pub(crate) name_with_owner: String,
    pub(crate) languages: Option<GitLanguages>,
    pub(crate) default_branch_ref: Option<DefaultBranchRef>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct RepositoriesConnection {
    pub(crate) nodes: Option<Vec<RepositoryNode>>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct UserRepos {
    pub(crate) repositories: RepositoriesConnection,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct ReposResponseData {
    pub(crate) user: Option<UserRepos>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GqlCost {
    pub(crate) actual_cost: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GqlExtensions {
    pub(crate) cost: GqlCost,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct GraphQLResponse<T> {
    pub(crate) data: Option<T>,
    pub(crate) errors: Option<Vec<serde_json::Value>>,
    pub(crate) extensions: Option<GqlExtensions>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContributionDay {
    pub(crate) contribution_count: usize,
    pub(crate) date: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContributionWeek {
    pub(crate) contribution_days: Vec<ContributionDay>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContributionCalendar {
    pub(crate) weeks: Vec<ContributionWeek>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContributionsCollection {
    pub(crate) contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct StreakUserData {
    pub(crate) user: StreakUser,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreakUser {
    pub(crate) contributions_collection: ContributionsCollection,
}
