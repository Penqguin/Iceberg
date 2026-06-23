use crate::error::AppError;
use crate::github::client::{execute_graphql, HttpFetch};
use crate::github::streak::{calculate_current_streak, calculate_highest_streak, is_active};
use crate::github::types::*;
use crate::models::{CommitItem, CommitsListResponse, CommitsListStats, Language, StreakInfo};
use serde_json::json;
use std::collections::{HashMap, HashSet};

// --- Internal helpers ---

pub(crate) fn get_excluded_repos(username: &str) -> Vec<&'static str> {
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

pub(crate) fn parse_languages(languages: &Option<GitLanguages>) -> Vec<Language> {
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

pub(crate) fn matches_author(commit: &GqlCommit, username: &str) -> bool {
    commit
        .author
        .as_ref()
        .and_then(|a| a.user.as_ref())
        .map(|u| u.login.eq_ignore_ascii_case(username))
        .unwrap_or(false)
}

pub(crate) fn flatten_contribution_days(cal: &ContributionCalendar) -> Vec<ContributionDay> {
    cal.weeks
        .iter()
        .flat_map(|w| w.contribution_days.iter())
        .cloned()
        .collect()
}

pub(crate) fn redistribute_languages(languages: &mut Vec<Language>) {
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
    let mut seen_repos = HashSet::new();

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
