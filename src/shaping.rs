use crate::github::types::*;
use crate::models::{CommitItem, CommitsListResponse, CommitsListStats, Language};
use std::collections::{HashMap, HashSet};

// --- Profile shaping: pure, no worker runtime, struct-in → struct-out ---

/// Product rules applied to raw GitHub data before it's served.
/// Only whitelisted (owner) profiles carry an exclusion list.
/// Public so external benches/tests can shape synthetic repos.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapingRules {
    pub(crate) excluded_repos: Vec<String>,
}

impl ShapingRules {
    pub fn none() -> Self {
        Self { excluded_repos: Vec::new() }
    }

    pub fn new(excluded_repos: Vec<String>) -> Self {
        Self { excluded_repos }
    }

    pub(crate) fn excludes(&self, repo: &str) -> bool {
        self.excluded_repos.iter().any(|r| r == repo)
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

/// Shape raw repository nodes into the served profile: filter exclusions,
/// collect the author's commits, roll up languages, compute stats over the
/// history window, trim to `limit`. Pure: struct-in → struct-out, no runtime.
pub fn shape_commits(
    repos: Vec<RepositoryNode>,
    username: &str,
    limit: usize,
    history_limit: usize,
    rules: &ShapingRules,
) -> CommitsListResponse {
    if repos.is_empty() {
        return CommitsListResponse {
            commits: vec![],
            languages: vec![],
            stats: CommitsListStats { total_additions: 0, total_deletions: 0, total_commits: 0 },
        };
    }

    let mut all_commits: Vec<CommitItem> = Vec::new();
    let mut repo_languages: HashMap<String, Vec<Language>> = HashMap::new();

    for repo in repos {
        if rules.excludes(&repo.name_with_owner) {
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

    let total_commits = all_commits.len();
    let final_limit = if limit > 0 { limit } else { 10 };
    let limited_commits = if final_limit < total_commits {
        all_commits[..final_limit].to_vec()
    } else {
        all_commits
    };

    CommitsListResponse {
        commits: limited_commits,
        languages,
        stats: CommitsListStats {
            total_additions,
            total_deletions,
            total_commits,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn repo(name: &str, langs: Vec<(&str, usize)>, commits: Vec<GqlCommit>) -> RepositoryNode {
        RepositoryNode {
            name_with_owner: name.into(),
            languages: Some(GitLanguages {
                edges: Some(langs.into_iter().map(|(name, size)| GitLanguageEdge {
                    size,
                    node: GitLanguageNode { name: name.into(), color: None },
                }).collect()),
            }),
            default_branch_ref: Some(DefaultBranchRef {
                target: Some(GqlTargetCommit {
                    history: GqlHistory {
                        edges: Some(commits.into_iter().map(|node| GqlCommitEdge { node }).collect()),
                    },
                }),
            }),
        }
    }

    fn commit(login: &str, date: &str, additions: usize) -> GqlCommit {
        GqlCommit {
            abbreviated_oid: "abc".into(),
            additions,
            deletions: 0,
            commit_url: "url".into(),
            committed_date: chrono::DateTime::parse_from_rfc3339(&format!("{}T12:00:00Z", date)).unwrap().with_timezone(&Utc),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: Some(CommitAuthor { user: Some(AuthorUser { login: login.into() }) }),
        }
    }

    #[test]
    fn excluded_repos_are_filtered() {
        let rules = ShapingRules::new(vec!["me/private".into()]);
        let repos = vec![
            repo("me/private", vec![], vec![commit("me", "2024-01-02", 10)]),
            repo("me/public", vec![], vec![commit("me", "2024-01-01", 5)]),
        ];
        let out = shape_commits(repos, "me", 10, 5, &rules);
        assert_eq!(out.commits.len(), 1);
        assert_eq!(out.commits[0].repo, "me/public");
        assert_eq!(out.stats.total_commits, 1);
    }

    #[test]
    fn foreign_authors_filtered() {
        let rules = ShapingRules::none();
        let repos = vec![repo("me/repo", vec![], vec![
            commit("me", "2024-01-02", 10),
            commit("bot", "2024-01-03", 99),
        ])];
        let out = shape_commits(repos, "me", 10, 5, &rules);
        assert_eq!(out.commits.len(), 1);
        assert_eq!(out.stats.total_additions, 10);
    }

    #[test]
    fn stats_window_respects_history_limit() {
        let rules = ShapingRules::none();
        let commits: Vec<GqlCommit> = (1..=5)
            .map(|i| commit("me", &format!("2024-01-0{}", i), 10))
            .collect();
        let repos = vec![repo("me/repo", vec![], commits)];
        // history_limit 2 → stats over the 2 most recent commits only
        let out = shape_commits(repos, "me", 10, 2, &rules);
        assert_eq!(out.stats.total_commits, 5); // total counted before trim
        assert_eq!(out.stats.total_additions, 20); // 2 commits × 10
        assert_eq!(out.commits.len(), 5); // response still carries all
    }

    #[test]
    fn languages_roll_up_by_repo() {
        let rules = ShapingRules::none();
        let repos = vec![repo("me/rust-repo", vec![("Rust", 80), ("JS", 20)], vec![
            commit("me", "2024-01-01", 1),
        ])];
        let out = shape_commits(repos, "me", 10, 5, &rules);
        assert_eq!(out.languages.len(), 2);
        assert_eq!(out.languages[0].name, "Rust");
        assert_eq!(out.languages[0].size, 80);
    }

    #[test]
    fn languages_are_raw_rollup_no_redistribution() {
        let rules = ShapingRules::none();
        // Rust is dominant (>80% of the total); its size must be the raw
        // GitHub roll-up, untouched by any presentation-shaping rule.
        let repos = vec![
            repo("me/a", vec![("Rust", 90), ("JS", 10)], vec![
                commit("me", "2024-01-01", 1),
            ]),
            repo("me/b", vec![("Rust", 90)], vec![
                commit("me", "2024-01-02", 1),
            ]),
        ];
        let out = shape_commits(repos, "me", 10, 5, &rules);
        let rust = out.languages.iter().find(|l| l.name == "Rust").unwrap();
        let js = out.languages.iter().find(|l| l.name == "JS").unwrap();
        assert_eq!(rust.size, 180); // 90 + 90, exactly as GitHub reported
        assert_eq!(js.size, 10);
    }

    #[test]
    fn empty_repos_shape_to_empty_response() {
        let out = shape_commits(vec![], "me", 10, 5, &ShapingRules::none());
        assert!(out.commits.is_empty());
        assert!(out.languages.is_empty());
        assert_eq!(out.stats.total_commits, 0);
    }

    #[test]
    fn zero_limit_falls_back_to_ten() {
        let rules = ShapingRules::none();
        let commits: Vec<GqlCommit> = (1..=15)
            .map(|i| commit("me", &format!("2024-01-{:02}", i), 1))
            .collect();
        let repos = vec![repo("me/repo", vec![], commits)];
        let out = shape_commits(repos, "me", 0, 5, &rules);
        assert_eq!(out.commits.len(), 10);
    }
}
