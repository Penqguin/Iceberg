use crate::error::AppError;
use crate::models::{CommitItem, CommitsListResponse, CommitsListStats, Language, StreakInfo};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use futures::future::{select, Either};
use serde::Deserialize;
use serde_json::json;
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::VecDeque;
use std::collections::HashMap;
use worker::*;
#[cfg(test)]
use tokio::time as tokio_time;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

/// Log a warning — `console_warn!` on wasm32, `eprintln!` on native.
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "wasm32")]
        console_warn!($($arg)*);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($arg)*);
    }};
}

/// Sleep — `tokio::time::sleep` in tests, `worker::Delay` in production (wasm32).
#[cfg(test)]
async fn sleep(dur: std::time::Duration) {
    tokio_time::sleep(dur).await;
}
#[cfg(not(test))]
async fn sleep(dur: std::time::Duration) {
    Delay::from(dur).await;
}

// --- HttpFetch seam ---

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub trait HttpFetch {
    async fn post_json(&self, url: &str, token: &str, body: &serde_json::Value, timeout_secs: u64) -> std::result::Result<HttpResponse, AppError>;
}

pub struct RealHttpFetch;

impl HttpFetch for RealHttpFetch {
    async fn post_json(&self, url: &str, token: &str, body: &serde_json::Value, timeout_secs: u64) -> std::result::Result<HttpResponse, AppError> {
        let headers = Headers::new();
        headers.set("User-Agent", "iceberg-rust-api/1.0")?;
        headers.set("Authorization", &format!("Bearer {}", token))?;
        headers.set("Content-Type", "application/json")?;

        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(serde_json::to_vec(body)?.into()));

        let request = Request::new_with_init(url, &init)?;
        let fetch = Fetch::Request(request);
        let fetch_fut = Box::pin(fetch.send());
        let timeout_fut = Box::pin(Delay::from(std::time::Duration::from_secs(timeout_secs)));

        let mut response = match select(fetch_fut, timeout_fut).await {
            Either::Left((resp, _)) => resp?,
            Either::Right(_) => {
                return Err(AppError::Timeout(format!("GitHub API request timed out after {} seconds", timeout_secs)));
            }
        };

        let status = response.status_code();
        let resp_headers = response.headers().clone();
        let mut hdrs = HashMap::new();
        for key in &["X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset", "Retry-After"] {
            if let Ok(Some(val)) = resp_headers.get(key) {
                hdrs.insert(key.to_string(), val);
            }
        }

        if let (Ok(Some(limit)), Ok(Some(rem)), Ok(Some(reset))) = (
            resp_headers.get("X-RateLimit-Limit"),
            resp_headers.get("X-RateLimit-Remaining"),
            resp_headers.get("X-RateLimit-Reset"),
        ) {
            console_log!("GitHub API Rate Limit: {}/{} (Resets: {})", rem, limit, reset);
        }

        let body_bytes = response.bytes().await?.to_vec();
        Ok(HttpResponse { status, headers: hdrs, body: body_bytes })
    }
}

// --- GitHub API wire types ---

#[derive(Deserialize, Debug, Clone)]
struct GitLanguageNode {
    name: String,
    color: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct GitLanguageEdge {
    size: usize,
    node: GitLanguageNode,
}

#[derive(Deserialize, Debug, Clone)]
struct GitLanguages {
    edges: Option<Vec<GitLanguageEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
struct AuthorUser {
    login: String,
}

#[derive(Deserialize, Debug, Clone)]
struct CommitAuthor {
    user: Option<AuthorUser>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GqlCommit {
    abbreviated_oid: String,
    additions: usize,
    deletions: usize,
    commit_url: String,
    committed_date: DateTime<Utc>,
    message_headline: String,
    message_body: String,
    author: Option<CommitAuthor>,
}

#[derive(Deserialize, Debug, Clone)]
struct GqlCommitEdge {
    node: GqlCommit,
}

#[derive(Deserialize, Debug, Clone)]
struct GqlHistory {
    edges: Option<Vec<GqlCommitEdge>>,
}

#[derive(Deserialize, Debug, Clone)]
struct GqlTargetCommit {
    history: GqlHistory,
}

#[derive(Deserialize, Debug, Clone)]
struct DefaultBranchRef {
    target: Option<GqlTargetCommit>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RepositoryNode {
    name_with_owner: String,
    languages: Option<GitLanguages>,
    default_branch_ref: Option<DefaultBranchRef>,
}

#[derive(Deserialize, Debug, Clone)]
struct RepositoriesConnection {
    nodes: Option<Vec<RepositoryNode>>,
}

#[derive(Deserialize, Debug, Clone)]
struct UserRepos {
    repositories: RepositoriesConnection,
}

#[derive(Deserialize, Debug, Clone)]
struct ReposResponseData {
    user: Option<UserRepos>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GqlCost {
    actual_cost: u64,
}

#[derive(Deserialize, Debug, Clone)]
struct GqlExtensions {
    cost: GqlCost,
}

#[derive(Deserialize, Debug, Clone)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<serde_json::Value>>,
    extensions: Option<GqlExtensions>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ContributionDay {
    contribution_count: usize,
    date: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ContributionWeek {
    contribution_days: Vec<ContributionDay>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ContributionCalendar {
    weeks: Vec<ContributionWeek>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ContributionsCollection {
    contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize, Debug, Clone)]
struct StreakUserData {
    user: StreakUser,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct StreakUser {
    contributions_collection: ContributionsCollection,
}

// --- Internal helpers ---

fn get_excluded_repos(username: &str) -> Vec<&'static str> {
    if username.eq_ignore_ascii_case("jasonlovesdoggo") {
        vec![
            "jasonlovesdoggo/jasonlovesdoggo",
            "jasonlovesdoggo/notes",
            "jasonlovesdoggo/status",
        ]
    } else {
        vec![]
    }
}

fn parse_languages(languages: &Option<GitLanguages>) -> Vec<Language> {
    languages
        .as_ref()
        .and_then(|l| l.edges.as_ref())
        .map(|edges| {
            edges
                .iter()
                .map(|e| Language {
                    size: e.size,
                    name: e.node.name.clone(),
                    color: e.node.color.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn matches_author(commit: &GqlCommit, username: &str) -> bool {
    commit
        .author
        .as_ref()
        .and_then(|a| a.user.as_ref())
        .map(|u| u.login.eq_ignore_ascii_case(username))
        .unwrap_or(false)
}

fn flatten_contribution_days(cal: &ContributionCalendar) -> Vec<ContributionDay> {
    cal.weeks
        .iter()
        .flat_map(|w| w.contribution_days.iter())
        .cloned()
        .collect()
}

fn redistribute_languages(languages: &mut Vec<Language>) {
    if languages.len() <= 1 {
        return;
    }
    let total: usize = languages.iter().map(|l| l.size).sum();
    if total == 0 {
        return;
    }
    let dominant_pct = languages[0].size as f64 / total as f64;
    if dominant_pct <= 0.8 {
        return;
    }

    let redistributed = (languages[0].size as f64 * 0.4) as usize;
    languages[0].size -= redistributed;

    let second_bonus = redistributed / 2;
    languages[1].size += second_bonus;

    let remaining = redistributed - second_bonus;
    if languages.len() > 2 && remaining > 0 {
        let per_lang = remaining / (languages.len() - 2);
        let extra = remaining % (languages.len() - 2);
        for (i, lang) in languages.iter_mut().enumerate().skip(2) {
            lang.size += per_lang;
            if (i - 2) < extra {
                lang.size += 1;
            }
        }
    } else {
        languages[1].size += remaining;
    }
}

// --- GraphQL execution layer ---

pub async fn execute_graphql<F, R>(
    fetcher: &F,
    token: &str,
    query: &str,
    variables: serde_json::Value,
    timeout_secs: Option<u64>,
) -> std::result::Result<(R, u64), AppError>
where
    F: HttpFetch,
    R: serde::de::DeserializeOwned,
{
    let timeout = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let payload = json!({ "query": query, "variables": variables });

    let resp = execute_with_retries::<F, GraphQLResponse<R>>(fetcher, token, &payload, timeout).await?;
    let cost = resp.extensions.as_ref().map(|e| e.cost.actual_cost).unwrap_or(0);

    if let Some(errors) = resp.errors {
        if !errors.is_empty() {
            return Err(AppError::GitHubError(
                errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error").to_string(),
            ));
        }
    }

    let data = resp.data.ok_or_else(|| AppError::Internal("No data in GraphQL response".to_string()))?;
    Ok((data, cost))
}

async fn execute_with_retries<F, R>(
    fetcher: &F,
    token: &str,
    payload: &serde_json::Value,
    timeout_secs: u64,
) -> std::result::Result<R, AppError>
where
    F: HttpFetch,
    R: serde::de::DeserializeOwned,
{
    let max_retries = 5;
    let mut retry_count = 0;

    loop {
        let resp = fetcher.post_json(GITHUB_GRAPHQL_URL, token, payload, timeout_secs).await?;

        let is_rate_limited = resp.status == 429
            || (resp.status == 403 && resp.headers.get("X-RateLimit-Remaining").map(|s| s.as_str()) == Some("0"));

        if is_rate_limited {
            if retry_count >= max_retries {
                let retry_after = resp.headers.get("Retry-After")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                return Err(AppError::RateLimited(retry_after));
            }
            let wait = 2u64.pow(retry_count);
            log_warn!("Rate limited. Retrying in {} seconds...", wait);
            sleep(std::time::Duration::from_secs(wait)).await;
            retry_count += 1;
            continue;
        }

        if resp.status != 200 {
            let body = String::from_utf8_lossy(&resp.body).to_string();
            return match resp.status {
                401 | 403 => Err(AppError::Unauthorized(format!("GitHub API unauthorized: {}", body))),
                400 => Err(AppError::BadRequest(format!("GitHub API bad request: {}", body))),
                _ => Err(AppError::GitHubError(format!("GitHub API status {}: {}", resp.status, body))),
            };
        }

        let gql_resp: R = serde_json::from_slice(&resp.body)?;
        return Ok(gql_resp);
    }
}

// --- Public domain functions ---

pub async fn get_commits_list<F: HttpFetch>(
    fetcher: &F,
    username: &str,
    token: &str,
    limit: usize,
    history_limit: usize,
) -> std::result::Result<(CommitsListResponse, u64), AppError> {
    let query = r#"
    query($username: String!, $firstRepos: Int!, $firstCommits: Int!, $firstLanguages: Int!) {
      user(login: $username) {
        repositories(first: $firstRepos, privacy: PUBLIC, orderBy: {field: UPDATED_AT, direction: DESC}) {
          nodes {
            nameWithOwner
            languages(first: $firstLanguages) {
              edges {
                size
                node { name color }
              }
            }
            defaultBranchRef {
              target {
                ... on Commit {
                  history(first: $firstCommits) {
                    edges {
                      node {
                        abbreviatedOid
                        additions
                        deletions
                        commitUrl
                        committedDate
                        messageHeadline
                        messageBody
                        author { user { login } }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
    "#;

    let variables = json!({
        "username": username,
        "firstRepos": limit.max(10).min(50),
        "firstCommits": history_limit.max(1),
        "firstLanguages": 3,
    });

    let (data, total_cost): (ReposResponseData, u64) =
        execute_graphql(fetcher, token, query, variables, None).await?;

    let repos: Vec<RepositoryNode> = data
        .user
        .and_then(|u| u.repositories.nodes)
        .unwrap_or_default();

    if repos.is_empty() {
        return Ok((CommitsListResponse {
            commits: vec![],
            languages: vec![],
            stats: CommitsListStats { total_additions: 0, total_deletions: 0, total_commits: 0 },
        }, total_cost));
    }

    let excluded = get_excluded_repos(username);

    let mut all_commits: Vec<CommitItem> = Vec::new();
    let mut repo_languages: HashMap<String, Vec<Language>> = HashMap::new();

    for repo in repos {
        if excluded.contains(&repo.name_with_owner.as_str()) {
            continue;
        }

        repo_languages.insert(repo.name_with_owner.clone(), parse_languages(&repo.languages));

        let edges = repo
            .default_branch_ref
            .as_ref()
            .and_then(|r| r.target.as_ref())
            .and_then(|t| t.history.edges.as_ref());

        if let Some(edges) = edges {
            for edge in edges {
                if !matches_author(&edge.node, username) {
                    continue;
                }
                all_commits.push(CommitItem {
                    repo: repo.name_with_owner.clone(),
                    additions: edge.node.additions,
                    deletions: edge.node.deletions,
                    commit_url: edge.node.commit_url.clone(),
                    committed_date: edge.node.committed_date,
                    oid: edge.node.abbreviated_oid.clone(),
                    message_headline: edge.node.message_headline.clone(),
                    message_body: edge.node.message_body.clone(),
                });
            }
        }
    }

    all_commits.sort_by(|a, b| b.committed_date.cmp(&a.committed_date));

    let h_limit = history_limit.max(1);
    let stats_commits = &all_commits[..all_commits.len().min(h_limit)];

    let mut total_additions = 0usize;
    let mut total_deletions = 0usize;
    let mut lang_map: HashMap<String, Language> = HashMap::new();
    let mut seen_repos = std::collections::HashSet::new();

    for commit in stats_commits {
        total_additions += commit.additions;
        total_deletions += commit.deletions;

        if seen_repos.insert(&commit.repo) {
            if let Some(langs) = repo_languages.get(&commit.repo) {
                for lang in langs {
                    let entry = lang_map.entry(lang.name.clone()).or_insert_with(|| Language {
                        size: 0,
                        name: lang.name.clone(),
                        color: lang.color.clone(),
                    });
                    entry.size += lang.size;
                }
            }
        }
    }

    let mut languages: Vec<Language> = lang_map.into_values().collect();
    languages.sort_by(|a, b| b.size.cmp(&a.size));
    redistribute_languages(&mut languages);

    let total_commits = all_commits.len();
    let final_limit = if limit > 0 { limit } else { 10 };
    let limited_commits = if final_limit < total_commits {
        all_commits[..final_limit].to_vec()
    } else {
        all_commits
    };

    Ok((CommitsListResponse {
        commits: limited_commits,
        languages,
        stats: CommitsListStats {
            total_additions,
            total_deletions,
            total_commits,
        },
    }, total_cost))
}

pub async fn get_streak_info<F: HttpFetch>(
    fetcher: &F,
    username: &str,
    token: &str,
) -> std::result::Result<(StreakInfo, u64), AppError> {
    let query = r#"
    query($username: String!) {
      user(login: $username) {
        contributionsCollection {
          contributionCalendar {
            weeks {
              contributionDays {
                contributionCount
                date
              }
            }
          }
        }
      }
    }
    "#;

    let (data, cost): (StreakUserData, u64) =
        execute_graphql(fetcher, token, query, json!({ "username": username }), None).await?;

    let days = flatten_contribution_days(&data.user.contributions_collection.contribution_calendar);

    Ok((StreakInfo {
        current_streak: calculate_current_streak(&days),
        highest_streak: calculate_highest_streak(&days),
        active: is_active(&days),
    }, cost))
}

// --- Streak calculation (pure functions) ---

fn calculate_current_streak(days: &[ContributionDay]) -> usize {
    if days.is_empty() {
        return 0;
    }

    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    let has_recent = days.iter().rev().any(|d| {
        (d.date == today || d.date == yesterday) && d.contribution_count > 0
    });

    if !has_recent {
        return 0;
    }

    let mut streak = 0;
    let mut skipped_today = false;

    for day in days.iter().rev() {
        let day_date = match NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        if day_date > now.naive_utc().date() {
            continue;
        }

        if day.date == today && day.contribution_count == 0 && !skipped_today {
            skipped_today = true;
            continue;
        }

        if day.contribution_count > 0 {
            streak += 1;
        } else {
            break;
        }
    }

    streak
}

fn calculate_highest_streak(days: &[ContributionDay]) -> usize {
    let mut highest = 0;
    let mut current = 0;

    for day in days {
        if day.contribution_count > 0 {
            current += 1;
            if current > highest {
                highest = current;
            }
        } else {
            current = 0;
        }
    }

    highest
}

fn is_active(days: &[ContributionDay]) -> bool {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    days.iter().rev().any(|d| {
        (d.date == today || d.date == yesterday) && d.contribution_count > 0
    })
}

// --- Fake HttpFetch for tests ---

#[cfg(test)]
pub struct FakeHttpFetch {
    responses: RefCell<VecDeque<std::result::Result<HttpResponse, AppError>>>,
}

#[cfg(test)]
impl FakeHttpFetch {
    pub fn new() -> Self {
        Self { responses: RefCell::new(VecDeque::new()) }
    }

    pub fn push_ok(&self, status: u16, body: &str) {
        self.responses.borrow_mut().push_back(Ok(HttpResponse {
            status,
            headers: HashMap::from([
                ("X-RateLimit-Remaining".into(), "4900".into()),
                ("X-RateLimit-Limit".into(), "5000".into()),
                ("X-RateLimit-Reset".into(), "9999999999".into()),
            ]),
            body: body.as_bytes().to_vec(),
        }));
    }

    pub fn push_rate_limited(&self) {
        self.responses.borrow_mut().push_back(Ok(HttpResponse {
            status: 403,
            headers: HashMap::from([
                ("X-RateLimit-Remaining".into(), "0".into()),
                ("X-RateLimit-Limit".into(), "5000".into()),
                ("X-RateLimit-Reset".into(), "9999999999".into()),
            ]),
            body: r#"{"data":null}"#.as_bytes().to_vec(),
        }));
    }

    #[allow(dead_code)]
    pub fn push_err(&self, err: AppError) {
        self.responses.borrow_mut().push_back(Err(err));
    }
}

#[cfg(test)]
impl HttpFetch for FakeHttpFetch {
    async fn post_json(&self, _url: &str, _token: &str, _body: &serde_json::Value, _timeout_secs: u64) -> std::result::Result<HttpResponse, AppError> {
        self.responses.borrow_mut().pop_front().unwrap_or_else(|| {
            Err(AppError::Internal("No more fake responses".to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn today() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    fn day(days_ago: i64) -> String {
        (Utc::now() - chrono::Duration::days(days_ago)).format("%Y-%m-%d").to_string()
    }

    // --- calculate_current_streak ---

    #[test]
    fn streak_empty() {
        assert_eq!(calculate_current_streak(&[]), 0);
    }

    #[test]
    fn streak_no_recent_contribution() {
        let days = vec![
            ContributionDay { contribution_count: 5, date: "2023-06-01".to_string() },
        ];
        assert_eq!(calculate_current_streak(&days), 0);
    }

    #[test]
    fn streak_single_today() {
        let days = vec![
            ContributionDay { contribution_count: 0, date: day(5) },
            ContributionDay { contribution_count: 0, date: day(2) },
            ContributionDay { contribution_count: 0, date: day(1) },
            ContributionDay { contribution_count: 3, date: today() },
        ];
        assert_eq!(calculate_current_streak(&days), 1);
    }

    #[test]
    fn streak_consecutive_days() {
        let days = vec![
            ContributionDay { contribution_count: 0, date: day(5) },
            ContributionDay { contribution_count: 3, date: day(2) },
            ContributionDay { contribution_count: 1, date: day(1) },
            ContributionDay { contribution_count: 5, date: today() },
        ];
        assert_eq!(calculate_current_streak(&days), 3);
    }

    #[test]
    fn streak_breaks_at_gap() {
        let days = vec![
            ContributionDay { contribution_count: 3, date: day(4) },
            ContributionDay { contribution_count: 0, date: day(3) },
            ContributionDay { contribution_count: 1, date: day(2) },
            ContributionDay { contribution_count: 5, date: day(1) },
            ContributionDay { contribution_count: 2, date: today() },
        ];
        assert_eq!(calculate_current_streak(&days), 3);
    }

    #[test]
    fn streak_skips_today_when_zero() {
        let days = vec![
            ContributionDay { contribution_count: 3, date: day(3) },
            ContributionDay { contribution_count: 1, date: day(2) },
            ContributionDay { contribution_count: 5, date: day(1) },
            ContributionDay { contribution_count: 0, date: today() },
        ];
        assert_eq!(calculate_current_streak(&days), 3);
    }

    #[test]
    fn streak_today_and_yesterday_both_zero() {
        let days = vec![
            ContributionDay { contribution_count: 3, date: day(3) },
            ContributionDay { contribution_count: 1, date: day(2) },
            ContributionDay { contribution_count: 0, date: day(1) },
            ContributionDay { contribution_count: 0, date: today() },
        ];
        assert_eq!(calculate_current_streak(&days), 0);
    }

    // --- calculate_highest_streak ---

    #[test]
    fn highest_streak_empty() {
        assert_eq!(calculate_highest_streak(&[]), 0);
    }

    #[test]
    fn highest_streak_all_contributing() {
        let days = vec![
            ContributionDay { contribution_count: 1, date: "a".into() },
            ContributionDay { contribution_count: 2, date: "b".into() },
            ContributionDay { contribution_count: 3, date: "c".into() },
        ];
        assert_eq!(calculate_highest_streak(&days), 3);
    }

    #[test]
    fn highest_streak_with_gaps() {
        let days = vec![
            ContributionDay { contribution_count: 1, date: "a".into() },
            ContributionDay { contribution_count: 1, date: "b".into() },
            ContributionDay { contribution_count: 0, date: "c".into() },
            ContributionDay { contribution_count: 5, date: "d".into() },
        ];
        assert_eq!(calculate_highest_streak(&days), 2);
    }

    #[test]
    fn highest_streak_picks_longest() {
        let days = vec![
            ContributionDay { contribution_count: 1, date: "a".into() },
            ContributionDay { contribution_count: 0, date: "b".into() },
            ContributionDay { contribution_count: 1, date: "c".into() },
            ContributionDay { contribution_count: 1, date: "d".into() },
            ContributionDay { contribution_count: 1, date: "e".into() },
        ];
        assert_eq!(calculate_highest_streak(&days), 3);
    }

    // --- is_active ---

    #[test]
    fn active_today() {
        let days = vec![
            ContributionDay { contribution_count: 0, date: day(1) },
            ContributionDay { contribution_count: 5, date: today() },
        ];
        assert!(is_active(&days));
    }

    #[test]
    fn active_yesterday() {
        let yesterday = (Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d").to_string();
        let days = vec![
            ContributionDay { contribution_count: 5, date: yesterday },
            ContributionDay { contribution_count: 0, date: today() },
        ];
        assert!(is_active(&days));
    }

    #[test]
    fn not_active() {
        let days = vec![
            ContributionDay { contribution_count: 0, date: day(2) },
            ContributionDay { contribution_count: 0, date: day(1) },
        ];
        assert!(!is_active(&days));
    }

    #[test]
    fn active_empty() {
        assert!(!is_active(&[]));
    }

    // --- redistribute_languages ---

    #[test]
    fn redistribute_single_lang_unchanged() {
        let mut langs = vec![Language { size: 100, name: "Rust".into(), color: None }];
        redistribute_languages(&mut langs);
        assert_eq!(langs[0].size, 100);
    }

    #[test]
    fn redistribute_below_threshold_unchanged() {
        let mut langs = vec![
            Language { size: 70, name: "Rust".into(), color: None },
            Language { size: 30, name: "JS".into(), color: None },
        ];
        redistribute_languages(&mut langs);
        assert_eq!(langs[0].size, 70);
        assert_eq!(langs[1].size, 30);
    }

    #[test]
    fn redistribute_dominant_language_two_only() {
        let mut langs = vec![
            Language { size: 90, name: "Rust".into(), color: None },
            Language { size: 10, name: "JS".into(), color: None },
        ];
        redistribute_languages(&mut langs);
        assert_eq!(langs[0].size, 54);
        assert_eq!(langs[1].size, 46);
    }

    // --- matches_author ---

    #[test]
    fn author_matches_login() {
        let commit = GqlCommit {
            abbreviated_oid: "abc".into(),
            additions: 1, deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: Some(CommitAuthor {
                user: Some(AuthorUser { login: "penqguin".into() }),
            }),
        };
        assert!(matches_author(&commit, "penqguin"));
    }

    #[test]
    fn author_case_insensitive() {
        let commit = GqlCommit {
            abbreviated_oid: "abc".into(),
            additions: 1, deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: Some(CommitAuthor {
                user: Some(AuthorUser { login: "Penqguin".into() }),
            }),
        };
        assert!(matches_author(&commit, "penqguin"));
    }

    #[test]
    fn author_does_not_match() {
        let commit = GqlCommit {
            abbreviated_oid: "abc".into(),
            additions: 1, deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: Some(CommitAuthor {
                user: Some(AuthorUser { login: "other".into() }),
            }),
        };
        assert!(!matches_author(&commit, "penqguin"));
    }

    #[test]
    fn author_no_user_field() {
        let commit = GqlCommit {
            abbreviated_oid: "abc".into(),
            additions: 1, deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: Some(CommitAuthor { user: None }),
        };
        assert!(!matches_author(&commit, "penqguin"));
    }

    #[test]
    fn author_none() {
        let commit = GqlCommit {
            abbreviated_oid: "abc".into(),
            additions: 1, deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: None,
        };
        assert!(!matches_author(&commit, "penqguin"));
    }

    // --- parse_languages ---

    #[test]
    fn parse_languages_none() {
        assert!(parse_languages(&None).is_empty());
    }

    #[test]
    fn parse_languages_with_edges() {
        let langs = GitLanguages {
            edges: Some(vec![
                GitLanguageEdge {
                    size: 100,
                    node: GitLanguageNode { name: "Rust".into(), color: Some("#dea584".into()) },
                },
            ]),
        };
        let result = parse_languages(&Some(langs));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Rust");
        assert_eq!(result[0].size, 100);
    }

    #[test]
    fn parse_languages_empty_edges() {
        let langs = GitLanguages { edges: Some(vec![]) };
        assert!(parse_languages(&Some(langs)).is_empty());
    }

    // --- get_excluded_repos ---

    #[test]
    fn excluded_repos_for_jason() {
        let repos = get_excluded_repos("jasonlovesdoggo");
        assert!(!repos.is_empty());
        assert!(repos.contains(&"jasonlovesdoggo/jasonlovesdoggo"));
    }

    #[test]
    fn excluded_repos_for_others_empty() {
        assert!(get_excluded_repos("penqguin").is_empty());
    }

    // --- flatten_contribution_days ---

    #[test]
    fn flatten_days_empty_calendar() {
        let cal = ContributionCalendar { weeks: vec![] };
        assert!(flatten_contribution_days(&cal).is_empty());
    }

    #[test]
    fn flatten_days_multiple_weeks() {
        let cal = ContributionCalendar {
            weeks: vec![
                ContributionWeek {
                    contribution_days: vec![
                        ContributionDay { contribution_count: 1, date: "2024-01-01".into() },
                    ],
                },
                ContributionWeek {
                    contribution_days: vec![
                        ContributionDay { contribution_count: 2, date: "2024-01-08".into() },
                    ],
                },
            ],
        };
        let days = flatten_contribution_days(&cal);
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, "2024-01-01");
        assert_eq!(days[1].date, "2024-01-08");
    }

    // --- HttpFetch seam integration tests ---

    #[tokio::test]
    async fn get_commits_list_empty_repos() {
        let fake = FakeHttpFetch::new();
        fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

        let (resp, _cost) = get_commits_list(&fake, "testuser", "token", 10, 5).await.unwrap();
        assert!(resp.commits.is_empty());
    }

    #[tokio::test]
    async fn get_commits_list_graphql_error() {
        let fake = FakeHttpFetch::new();
        fake.push_ok(200, r#"{"data":null,"errors":[{"message":"Not Found"}]}"#);

        let result = get_commits_list(&fake, "testuser", "token", 10, 5).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::GitHubError(msg) => assert!(msg.contains("Not Found")),
            _ => panic!("expected GitHubError"),
        }
    }

    #[tokio::test]
    async fn get_commits_list_rate_limited_then_ok() {
        let fake = FakeHttpFetch::new();
        fake.push_rate_limited();
        fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

        let (resp, _cost) = get_commits_list(&fake, "testuser", "token", 10, 5).await.unwrap();
        assert!(resp.commits.is_empty());
    }

    #[tokio::test]
    async fn get_commits_list_unauthorized() {
        let fake = FakeHttpFetch::new();
        fake.push_ok(401, r#"{"message":"Bad credentials"}"#);

        let result = get_commits_list(&fake, "testuser", "bad_token", 10, 5).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(msg) => assert!(msg.contains("Bad credentials")),
            _ => panic!("expected Unauthorized"),
        }
    }

    #[tokio::test]
    async fn get_streak_info_empty_calendar() {
        let fake = FakeHttpFetch::new();
        fake.push_ok(200, r#"{"data":{"user":{"contributionsCollection":{"contributionCalendar":{"weeks":[]}}}}}"#);

        let (info, _) = get_streak_info(&fake, "testuser", "token").await.unwrap();
        assert_eq!(info.current_streak, 0);
        assert_eq!(info.highest_streak, 0);
        assert!(!info.active);
    }
}
