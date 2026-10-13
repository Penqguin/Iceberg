use crate::error::AppError;
use crate::github::client::{execute_graphql, HttpFetch};
use crate::github::streak::{calculate_current_streak, calculate_highest_streak, is_active};
use crate::github::types::*;
use crate::models::{StreakInfo};
use crate::shaping::{shape_commits, ShapingRules};
use chrono::Utc;
use serde_json::json;

// --- Commits (GitHub adapter: query + parse; shaping happens in crate::shaping) ---

/// `timeout_secs` is the whole-cycle budget passed to `execute_graphql` (see
/// `execute_with_retries` for how the deadline is enforced).
pub async fn get_commits_list<F: HttpFetch>(
    fetcher: &F,
    username: &str,
    token: &str,
    limit: usize,
    history_limit: usize,
    language_limit: usize,
    excluded_repos: &[String],
    timeout_secs: u64,
) -> std::result::Result<(crate::models::CommitsListResponse, u64), AppError> {
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
        // The validated limit is used exactly (capped at 50 for safety) —
        // never clamped up, which would defeat the quota-reduction goal.
        "firstRepos": limit.min(50),
        "firstCommits": history_limit.max(1),
        "firstLanguages": language_limit,
    });

    let (data, total_cost): (ReposResponseData, u64) =
        execute_graphql(fetcher, token, query, variables, Some(timeout_secs)).await?;

    let repos: Vec<RepositoryNode> = data
        .user
        .and_then(|u| u.repositories.nodes)
        .unwrap_or_default();

    let rules = ShapingRules::new(excluded_repos.to_vec());
    Ok((shape_commits(repos, username, limit, history_limit, &rules), total_cost))
}

// --- Streak ---

pub async fn get_streak_info<F: HttpFetch>(
    fetcher: &F,
    username: &str,
    token: &str,
    timeout_secs: u64,
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
        execute_graphql(fetcher, token, query, json!({ "username": username }), Some(timeout_secs)).await?;

    let days = flatten_contribution_days(&data.user.contributions_collection.contribution_calendar);
    let as_of = Utc::now().naive_utc().date();

    Ok((StreakInfo {
        current_streak: calculate_current_streak(&days, as_of),
        highest_streak: calculate_highest_streak(&days),
        active: is_active(&days, as_of),
    }, cost))
}

pub(crate) fn flatten_contribution_days(cal: &ContributionCalendar) -> Vec<ContributionDay> {
    cal.weeks
        .iter()
        .flat_map(|w| w.contribution_days.iter())
        .cloned()
        .collect()
}
