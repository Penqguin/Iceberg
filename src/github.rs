use crate::error::AppError;
use crate::models::*;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use serde_json::json;
use std::collections::HashMap;
use worker::*;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";

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
) -> std::result::Result<R, AppError>
where
    R: serde::de::DeserializeOwned,
{
    let headers = Headers::new();
    headers.set("User-Agent", "iceberg-rust-api/1.0")?;
    headers.set("Authorization", &format!("Bearer {}", token))?;
    headers.set("Content-Type", "application/json")?;

    let request_payload = GraphQLRequest {
        query: query.to_string(),
        variables,
    };

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(serde_json::to_vec(&request_payload)?.into()));

    let mut response = Fetch::Request(Request::new_with_init(GITHUB_GRAPHQL_URL, &init)?)
        .send()
        .await?;

    if response.status_code() != 200 {
        let status = response.status_code();
        let body = response.text().await.unwrap_or_default();
        return match status {
            401 | 403 => Err(AppError::Unauthorized(format!("GitHub API unauthorized: {}", body))),
            400 => Err(AppError::BadRequest(format!("GitHub API bad request: {}", body))),
            _ => Err(AppError::GitHubError(format!("GitHub API status {}: {}", status, body))),
        };
    }

    let gql_resp: R = response.json().await?;

    Ok(gql_resp)
}

pub async fn get_most_recent_commit(
    username: &str,
    token: &str,
) -> std::result::Result<MostRecentCommit, AppError> {
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

    let resp: ReposGraphQLResponse = execute_graphql(token, query, variables).await?;

    if let Some(errors) = resp.errors {
        if !errors.is_empty() {
            return Err(AppError::GitHubError(
                errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error").to_string(),
            ));
        }
    }

    let user_repos = resp
        .data
        .and_then(|d| d.user)
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

    Ok(most_recent)
}

pub async fn get_commits_list(
    username: &str,
    token: &str,
    limit: usize,
) -> std::result::Result<CommitsListResponse, AppError> {
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
        "firstRepos": 20,
        "firstCommits": 10,
        "firstLanguages": 10
    });

    let resp: ReposGraphQLResponse = execute_graphql(token, query, variables).await?;

    if let Some(errors) = resp.errors {
        if !errors.is_empty() {
            return Err(AppError::GitHubError(
                errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error").to_string(),
            ));
        }
    }

    let user_repos = resp
        .data
        .and_then(|d| d.user)
        .ok_or_else(|| AppError::Internal(format!("User {} not found on GitHub", username)))?;

    let nodes = user_repos.repositories.nodes.unwrap_or_default();
    let excluded = get_excluded_repos(username);

    let mut all_commits = vec![];
    let mut language_map: HashMap<String, Language> = HashMap::new();
    let mut total_additions = 0;
    let mut total_deletions = 0;

    for repo in nodes {
        if excluded.contains(&repo.name_with_owner.as_str()) {
            continue;
        }

        let history_edges = repo
            .default_branch_ref
            .as_ref()
            .and_then(|r| r.target.as_ref())
            .and_then(|t| t.history.edges.as_ref());

        let mut has_valid_commit = false;
        if let Some(edges) = history_edges {
            for edge in edges {
                let matches_author = edge
                    .node
                    .author
                    .as_ref()
                    .and_then(|a| a.user.as_ref())
                    .map(|u| u.login.eq_ignore_ascii_case(username))
                    .unwrap_or(false);

                if matches_author {
                    has_valid_commit = true;
                    break;
                }
            }
        }

        if has_valid_commit {
            if let Some(langs) = &repo.languages {
                if let Some(edges) = &langs.edges {
                    for edge in edges {
                        let name = edge.node.name.clone();
                        let entry = language_map.entry(name.clone()).or_insert(Language {
                            size: 0,
                            name,
                            color: edge.node.color.clone(),
                        });
                        entry.size += edge.size;
                    }
                }
            }
        }

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

                total_additions += commit.additions;
                total_deletions += commit.deletions;

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

    all_commits.sort_by(|a, b| b.committed_date.cmp(&a.committed_date));

    let mut languages: Vec<Language> = language_map.into_values().collect();
    let total_language_size: usize = languages.iter().map(|l| l.size).sum();

    languages.sort_by(|a, b| b.size.cmp(&a.size));

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
    let limited_commits = if limit > 0 && limit < all_commits.len() {
        all_commits[..limit].to_vec()
    } else {
        all_commits
    };

    Ok(CommitsListResponse {
        commits: limited_commits,
        languages,
        stats: CommitsListStats {
            total_additions,
            total_deletions,
            total_commits,
        },
    })
}

pub async fn get_streak_info(
    username: &str,
    token: &str,
) -> std::result::Result<StreakInfo, AppError> {
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

    let resp: StreakGraphQLResponse = execute_graphql(token, query, variables).await?;

    if let Some(errors) = resp.errors {
        if !errors.is_empty() {
            return Err(AppError::GitHubError(
                errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error").to_string(),
            ));
        }
    }

    let user_contribs = resp
        .data
        .and_then(|d| d.user)
        .ok_or_else(|| AppError::Internal(format!("User {} not found on GitHub", username)))?;

    let mut contribution_days = vec![];
    for week in user_contribs.contributions_collection.contribution_calendar.weeks {
        contribution_days.extend(week.contribution_days);
    }

    let current_streak = calculate_current_streak(&contribution_days);
    let highest_streak = calculate_highest_streak(&contribution_days);
    let active = is_active(&contribution_days);

    Ok(StreakInfo {
        current_streak,
        highest_streak,
        active,
    })
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
