use crate::error::AppError;
use crate::models::*;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use futures::future::{select, Either};
use serde_json::json;
use std::collections::HashMap;
use worker::*;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

fn get_excluded_repos(username: &str) -> Vec<&'static str> {
    if username.to_lowercase() == "jasonlovesdoggo" {
        vec![
            "jasonlovesdoggo/jasonlovesdoggo",
            "jasonlovesdoggo/notes",
            "jasonlovesdoggo/status",
        ]
    } else {
        vec![]
    }
}

pub async fn execute_graphql<R>(
    token: &str,
    query: &str,
    variables: serde_json::Value,
    timeout_secs: Option<u64>,
) -> std::result::Result<(R, u64), AppError>
where
    R: serde::de::DeserializeOwned,
{
    let timeout = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let payload = json!({
        "query": query,
        "variables": variables
    });

    let (resp, _headers) = execute_with_retries::<GraphQLResponse<R>>(token, &payload, timeout).await?;
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

#[derive(serde::Serialize)]
pub struct GraphQLBatchOperation {
    pub query: String,
    pub variables: serde_json::Value,
}

pub async fn execute_graphql_batch<R>(
    token: &str,
    operations: Vec<GraphQLBatchOperation>,
    timeout_secs: Option<u64>,
) -> std::result::Result<(Vec<R>, u64), AppError>
where
    R: serde::de::DeserializeOwned,
{
    let timeout = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let payload = serde_json::to_value(&operations)?;

    let (resp, _headers) = execute_with_retries::<Vec<GraphQLResponse<R>>>(token, &payload, timeout).await?;
    
    let mut results = Vec::new();
    let mut total_cost = 0;

    for r in resp {
        total_cost += r.extensions.as_ref().map(|e| e.cost.actual_cost).unwrap_or(0);
        if let Some(errors) = r.errors {
            if !errors.is_empty() {
                // For batch, we might want to continue or fail. Requirements say handle partial failures.
                console_error!("Partial batch failure: {}", errors[0]["message"]);
                continue;
            }
        }
        if let Some(data) = r.data {
            results.push(data);
        }
    }

    Ok((results, total_cost))
}

async fn execute_with_retries<R>(
    token: &str,
    payload: &serde_json::Value,
    timeout_secs: u64,
) -> std::result::Result<(R, Headers), AppError>
where
    R: serde::de::DeserializeOwned,
{
    let mut retry_count = 0;
    let max_retries = 5;

    loop {
        let headers = Headers::new();
        headers.set("User-Agent", "iceberg-rust-api/1.0")?;
        headers.set("Authorization", &format!("Bearer {}", token))?;
        headers.set("Content-Type", "application/json")?;

        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(serde_json::to_vec(payload)?.into()));

        let request = Request::new_with_init(GITHUB_GRAPHQL_URL, &init)?;
        let fetch = Fetch::Request(request);
        let fetch_fut = fetch.send();
        let timeout_fut = Delay::from(std::time::Duration::from_secs(timeout_secs));

        let fetch_fut = Box::pin(fetch_fut);
        let timeout_fut = Box::pin(timeout_fut);

        let mut response = match select(fetch_fut, timeout_fut).await {
            Either::Left((resp_res, _)) => resp_res?,
            Either::Right(_) => {
                return Err(AppError::Timeout(format!("GitHub API request timed out after {} seconds", timeout_secs)));
            }
        };

        let status = response.status_code();
        let headers = response.headers().clone();

        // Extract rate limit headers for logging
        if let (Ok(Some(limit)), Ok(Some(rem)), Ok(Some(reset))) = (
            headers.get("X-RateLimit-Limit"),
            headers.get("X-RateLimit-Remaining"),
            headers.get("X-RateLimit-Reset"),
        ) {
            console_log!("GitHub API Rate Limit: {}/{} (Resets: {})", rem, limit, reset);
        }

        if status == 429 || (status == 403 && headers.get("X-RateLimit-Remaining")?.unwrap_or_default() == "0") {
            if retry_count >= max_retries {
                let retry_after = headers.get("Retry-After")?
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                return Err(AppError::RateLimited(retry_after));
            }

            let wait_secs = 2u64.pow(retry_count);
            console_warn!("Rate limited. Retrying in {} seconds...", wait_secs);
            Delay::from(std::time::Duration::from_secs(wait_secs)).await;
            retry_count += 1;
            continue;
        }

        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return match status {
                401 | 403 => Err(AppError::Unauthorized(format!("GitHub API unauthorized: {}", body))),
                400 => Err(AppError::BadRequest(format!("GitHub API bad request: {}", body))),
                _ => Err(AppError::GitHubError(format!("GitHub API status {}: {}", status, body))),
            };
        }

        let gql_resp: R = response.json().await?;
        return Ok((gql_resp, headers));
    }
}

pub async fn get_most_recent_commit(
    username: &str,
    token: &str,
) -> std::result::Result<(MostRecentCommit, u64), AppError> {
    let query = r#"
    query($username: String!, $firstRepos: Int!, $firstCommits: Int!, $firstLanguages: Int!) {
      user(login: $username) {
        repositories(first: $firstRepos, privacy: PUBLIC, orderBy: {field: UPDATED_AT, direction: DESC}) {
          nodes {
            nameWithOwner
            languages(first: $firstLanguages) {
              edges {
                size
                node {
                  name
                  color
                }
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
                        author {
                          user {
                            login
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
      }
    }
    "#;

    let variables = json!({
        "username": username,
        "firstRepos": 10,
        "firstCommits": 5,
        "firstLanguages": 5
    });

    let (user_repos_data, cost): (ReposResponseData, u64) = execute_graphql(token, query, variables, None).await?;

    let user_repos = user_repos_data.user
        .ok_or_else(|| AppError::Internal(format!("User {} not found on GitHub", username)))?;

    let nodes = user_repos.repositories.nodes.unwrap_or_default();
    let excluded = get_excluded_repos(username);

    let mut most_recent = MostRecentCommit {
        repo: String::new(),
        additions: 0,
        deletions: 0,
        commit_url: String::new(),
        committed_date: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
        oid: String::new(),
        message_headline: "Something went wrong".to_string(),
        message_body: "Please try again later".to_string(),
        languages: vec![],
        parent_commits: Some(vec![]),
    };

    for repo in nodes {
        if excluded.contains(&repo.name_with_owner.as_str()) {
            continue;
        }

        let history_edges = repo
            .default_branch_ref
            .and_then(|r| r.target)
            .and_then(|t| t.history.edges)
            .unwrap_or_default();

        for edge in history_edges {
            let commit = edge.node;

            let matches_author = commit
                .author
                .as_ref()
                .and_then(|a| a.user.as_ref())
                .map(|u| u.login.eq_ignore_ascii_case(username))
                .unwrap_or(false);

            if !matches_author {
                continue;
            }

            if repo.name_with_owner == most_recent.repo && commit.committed_date < most_recent.committed_date {
                if let Some(ref mut parents) = most_recent.parent_commits {
                    parents.push(ParentCommit {
                        additions: commit.additions,
                        deletions: commit.deletions,
                        commit_url: commit.commit_url.clone(),
                        committed_date: commit.committed_date,
                        message_headline: commit.message_headline.clone(),
                    });
                }
            }

            if commit.committed_date > most_recent.committed_date {
                let mut languages = vec![];
                if let Some(ref langs) = repo.languages {
                    if let Some(ref edges) = langs.edges {
                        for e in edges {
                            languages.push(Language {
                                size: e.size,
                                name: e.node.name.clone(),
                                color: e.node.color.clone(),
                            });
                        }
                    }
                }

                most_recent = MostRecentCommit {
                    repo: repo.name_with_owner.clone(),
                    additions: commit.additions,
                    deletions: commit.deletions,
                    commit_url: commit.commit_url,
                    committed_date: commit.committed_date,
                    oid: commit.abbreviated_oid,
                    message_headline: commit.message_headline,
                    message_body: commit.message_body,
                    languages,
                    parent_commits: Some(vec![]),
                };
            }
        }
    }

    Ok((most_recent, cost))
}

pub async fn get_commits_list(
    username: &str,
    token: &str,
    limit: usize,
    history_limit: usize,
) -> std::result::Result<(CommitsListResponse, u64), AppError> {
    // 1. Fetch the list of repository names
    let list_query = r#"
    query($username: String!, $firstRepos: Int!) {
      user(login: $username) {
        repositories(first: $firstRepos, privacy: PUBLIC, orderBy: {field: UPDATED_AT, direction: DESC}) {
          nodes {
            nameWithOwner
          }
        }
      }
    }
    "#;

    let variables = json!({
        "username": username,
        "firstRepos": limit.max(10).min(50)
    });

    let (list_resp_data, mut total_cost): (ReposResponseData, u64) = execute_graphql(token, list_query, variables, None).await?;
    
    let repos = list_resp_data.user
        .map(|u| u.repositories.nodes.unwrap_or_default())
        .unwrap_or_default();

    if repos.is_empty() {
        return Ok((CommitsListResponse {
            commits: vec![],
            languages: vec![],
            stats: CommitsListStats {
                total_additions: 0,
                total_deletions: 0,
                total_commits: 0,
            },
        }, total_cost));
    }

    let excluded = get_excluded_repos(username);
    let mut futures = futures::stream::FuturesUnordered::new();
    
    // Max concurrency: 5
    let max_concurrency = 5;
    let mut repo_iter = repos.into_iter();

    // 2. Fetch details for each repo in parallel
    for repo in repo_iter.by_ref().take(max_concurrency) {
        if excluded.contains(&repo.name_with_owner.as_str()) {
            continue;
        }
        futures.push(fetch_repo_details(token, repo.name_with_owner, history_limit));
    }

    let mut all_commits = vec![];
    let mut repo_languages: HashMap<String, Vec<Language>> = HashMap::new();
    let mut _partial_results = false;

    use futures::StreamExt;
    while let Some(res) = futures.next().await {
        match res {
            Ok((repo_node, cost)) => {
                total_cost += cost;
                if let Some(repo) = repo_node {
                    // Store languages
                    let mut languages = vec![];
                    if let Some(langs) = &repo.languages {
                        if let Some(edges) = &langs.edges {
                            for edge in edges {
                                languages.push(Language {
                                    size: edge.size,
                                    name: edge.node.name.clone(),
                                    color: edge.node.color.clone(),
                                });
                            }
                        }
                    }
                    repo_languages.insert(repo.name_with_owner.clone(), languages);

                    let history_edges = repo
                        .default_branch_ref
                        .as_ref()
                        .and_then(|r| r.target.as_ref())
                        .and_then(|t| t.history.edges.as_ref());

                    if let Some(edges) = history_edges {
                        for edge in edges {
                            let commit = &edge.node;
                            let matches_author = commit
                                .author
                                .as_ref()
                                .and_then(|a| a.user.as_ref())
                                .map(|u| u.login.eq_ignore_ascii_case(username))
                                .unwrap_or(false);

                            if !matches_author {
                                continue;
                            }

                            all_commits.push(CommitItem {
                                repo: repo.name_with_owner.clone(),
                                additions: commit.additions,
                                deletions: commit.deletions,
                                commit_url: commit.commit_url.clone(),
                                committed_date: commit.committed_date,
                                oid: commit.abbreviated_oid.clone(),
                                message_headline: commit.message_headline.clone(),
                                message_body: commit.message_body.clone(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                console_error!("Failed to fetch repo details: {}", e);
                _partial_results = true;
            }
        }

        // Keep pushing more repos to maintain concurrency
        if let Some(next_repo) = repo_iter.next() {
            if !excluded.contains(&next_repo.name_with_owner.as_str()) {
                futures.push(fetch_repo_details(token, next_repo.name_with_owner, history_limit));
            }
        }
    }

    // Sort all commits globally by date (descending)
    all_commits.sort_by(|a, b| b.committed_date.cmp(&a.committed_date));

    // Calculate stats and languages based ONLY on the most recent history_limit commits
    let h_limit = if history_limit == 0 { 5 } else { history_limit };
    let stats_commits = if h_limit < all_commits.len() {
        &all_commits[..h_limit]
    } else {
        &all_commits
    };

    let mut total_additions = 0;
    let mut total_deletions = 0;
    let mut language_map: HashMap<String, Language> = HashMap::new();
    let mut seen_repos_for_langs = std::collections::HashSet::new();

    for commit in stats_commits {
        total_additions += commit.additions;
        total_deletions += commit.deletions;

        if seen_repos_for_langs.insert(&commit.repo) {
            if let Some(langs) = repo_languages.get(&commit.repo) {
                for lang in langs {
                    let entry = language_map.entry(lang.name.clone()).or_insert(Language {
                        size: 0,
                        name: lang.name.clone(),
                        color: lang.color.clone(),
                    });
                    entry.size += lang.size;
                }
            }
        }
    }

    let mut languages: Vec<Language> = language_map.into_values().collect();
    let total_language_size: usize = languages.iter().map(|l| l.size).sum();

    languages.sort_by(|a, b| b.size.cmp(&a.size));

    // Language redistribution logic (kept same)
    if languages.len() > 1 && total_language_size > 0 {
        let dominant_percentage = languages[0].size as f64 / total_language_size as f64;
        if dominant_percentage > 0.8 {
            let redistributed = (languages[0].size as f64 * 0.4) as usize;
            languages[0].size -= redistributed;
            let second_bonus = redistributed / 2;
            languages[1].size += second_bonus;
            let remaining_redistributed = redistributed - second_bonus;

            if languages.len() > 2 && remaining_redistributed > 0 {
                let bonus_per_lang = remaining_redistributed / (languages.len() - 2);
                let remainder = remaining_redistributed % (languages.len() - 2);
                for i in 2..languages.len() {
                    languages[i].size += bonus_per_lang;
                    if (i - 2) < remainder {
                        languages[i].size += 1;
                    }
                }
            } else if languages.len() == 2 {
                languages[1].size += remaining_redistributed;
            }
        }
    }

    let total_commits = all_commits.len();
    let final_limit = if limit > 0 { limit } else { 10 };
    let limited_commits = if final_limit < all_commits.len() {
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

async fn fetch_repo_details(
    token: &str,
    name_with_owner: String,
    history_limit: usize,
) -> std::result::Result<(Option<RepositoryNode>, u64), AppError> {
    let parts: Vec<&str> = name_with_owner.split('/').collect();
    if parts.len() != 2 {
        return Ok((None, 0));
    }

    let query = r#"
    query($owner: String!, $name: String!, $firstCommits: Int!, $firstLanguages: Int!) {
      repository(owner: $owner, name: $name) {
        nameWithOwner
        languages(first: $firstLanguages) {
          edges {
            size
            node {
              name
              color
            }
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
                    author {
                      user {
                        login
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
        "owner": parts[0],
        "name": parts[1],
        "firstCommits": if history_limit == 0 { 5 } else { history_limit },
        "firstLanguages": 3
    });

    #[derive(serde::Deserialize)]
    struct RepoData {
        repository: Option<RepositoryNode>,
    }

    let (data, cost): (RepoData, u64) = execute_graphql(token, query, variables, None).await?;
    Ok((data.repository, cost))
}

pub async fn get_streak_info(
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

    let variables = json!({
        "username": username
    });

    let (user_contribs_data, cost): (UserContributions, u64) = execute_graphql(token, query, variables, None).await?;

    let user_contribs = user_contribs_data;

    let mut contribution_days = vec![];
    for week in user_contribs.contributions_collection.contribution_calendar.weeks {
        contribution_days.extend(week.contribution_days);
    }

    let current_streak = calculate_current_streak(&contribution_days);
    let highest_streak = calculate_highest_streak(&contribution_days);
    let active = is_active(&contribution_days);

    Ok((StreakInfo {
        current_streak,
        highest_streak,
        active,
    }, cost))
}

fn calculate_current_streak(contribution_days: &[ContributionDay]) -> usize {
    if contribution_days.is_empty() {
        return 0;
    }

    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    let mut has_recent_activity = false;
    for day in contribution_days.iter().rev() {
        if (day.date == today || day.date == yesterday) && day.contribution_count > 0 {
            has_recent_activity = true;
            break;
        }
    }

    if !has_recent_activity {
        return 0;
    }

    let mut current_streak = 0;
    let mut skipped_today = false;

    for day in contribution_days.iter().rev() {
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
            current_streak += 1;
        } else {
            break;
        }
    }

    current_streak
}

fn calculate_highest_streak(contribution_days: &[ContributionDay]) -> usize {
    let mut highest_streak = 0;
    let mut current_streak = 0;

    for day in contribution_days {
        if day.contribution_count > 0 {
            current_streak += 1;
            if current_streak > highest_streak {
                highest_streak = current_streak;
            }
        } else {
            current_streak = 0;
        }
    }

    highest_streak
}

fn is_active(contribution_days: &[ContributionDay]) -> bool {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    for day in contribution_days.iter().rev() {
        if (day.date == today || day.date == yesterday) && day.contribution_count > 0 {
            return true;
        }
    }

    false
}
