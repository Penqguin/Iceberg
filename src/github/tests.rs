use super::client::FakeHttpFetch;
use super::streak::{calculate_current_streak, calculate_highest_streak, is_active};
use super::types::*;
use crate::error::AppError;
use crate::shaping::{matches_author, parse_languages, redistribute_languages, ShapingRules};
use chrono::{Duration, NaiveDate, Utc};

fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn day(days_ago: i64) -> String {
    (Utc::now() - Duration::days(days_ago)).format("%Y-%m-%d").to_string()
}

fn as_of() -> NaiveDate {
    Utc::now().naive_utc().date()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

// --- calculate_current_streak ---

#[test]
fn streak_empty() {
    assert_eq!(calculate_current_streak(&[], as_of()), 0);
}

#[test]
fn streak_no_recent_contribution() {
    let days = vec![
        ContributionDay { contribution_count: 5, date: "2023-06-01".to_string() },
    ];
    assert_eq!(calculate_current_streak(&days, as_of()), 0);
}

#[test]
fn streak_single_today() {
    let days = vec![
        ContributionDay { contribution_count: 0, date: day(5) },
        ContributionDay { contribution_count: 0, date: day(2) },
        ContributionDay { contribution_count: 0, date: day(1) },
        ContributionDay { contribution_count: 3, date: today() },
    ];
    assert_eq!(calculate_current_streak(&days, as_of()), 1);
}

#[test]
fn streak_consecutive_days() {
    let days = vec![
        ContributionDay { contribution_count: 0, date: day(5) },
        ContributionDay { contribution_count: 3, date: day(2) },
        ContributionDay { contribution_count: 1, date: day(1) },
        ContributionDay { contribution_count: 5, date: today() },
    ];
    assert_eq!(calculate_current_streak(&days, as_of()), 3);
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
    assert_eq!(calculate_current_streak(&days, as_of()), 3);
}

#[test]
fn streak_skips_today_when_zero() {
    let days = vec![
        ContributionDay { contribution_count: 3, date: day(3) },
        ContributionDay { contribution_count: 1, date: day(2) },
        ContributionDay { contribution_count: 5, date: day(1) },
        ContributionDay { contribution_count: 0, date: today() },
    ];
    assert_eq!(calculate_current_streak(&days, as_of()), 3);
}

#[test]
fn streak_today_and_yesterday_both_zero() {
    let days = vec![
        ContributionDay { contribution_count: 3, date: day(3) },
        ContributionDay { contribution_count: 1, date: day(2) },
        ContributionDay { contribution_count: 0, date: day(1) },
        ContributionDay { contribution_count: 0, date: today() },
    ];
    assert_eq!(calculate_current_streak(&days, as_of()), 0);
}

// --- fixed-date boundary tests (clock seam) ---

#[test]
fn streak_fixed_date_yesterday_active_today_zero() {
    // as_of = Mar 15; yesterday (Mar 14) active, today zero → streak of 1.
    let days = vec![
        ContributionDay { contribution_count: 0, date: "2024-03-15".into() },
        ContributionDay { contribution_count: 4, date: "2024-03-14".into() },
    ];
    assert_eq!(calculate_current_streak(&days, date(2024, 3, 15)), 1);
    assert!(is_active(&days, date(2024, 3, 15)));
}

#[test]
fn streak_fixed_date_two_days_back_is_not_recent() {
    // Last contribution Mar 13, as_of Mar 15 → gap of 2 days: not recent.
    let days = vec![
        ContributionDay { contribution_count: 4, date: "2024-03-13".into() },
        ContributionDay { contribution_count: 0, date: "2024-03-14".into() },
        ContributionDay { contribution_count: 0, date: "2024-03-15".into() },
    ];
    assert_eq!(calculate_current_streak(&days, date(2024, 3, 15)), 0);
    assert!(!is_active(&days, date(2024, 3, 15)));
}

#[test]
fn streak_fixed_date_future_days_ignored() {
    // Calendar rows dated after as_of are skipped, not counted as a break.
    let days = vec![
        ContributionDay { contribution_count: 9, date: "2024-03-16".into() },
        ContributionDay { contribution_count: 2, date: "2024-03-15".into() },
        ContributionDay { contribution_count: 1, date: "2024-03-14".into() },
    ];
    assert_eq!(calculate_current_streak(&days, date(2024, 3, 15)), 2);
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
    assert!(is_active(&days, as_of()));
}

#[test]
fn active_yesterday() {
    let days = vec![
        ContributionDay { contribution_count: 5, date: day(1) },
        ContributionDay { contribution_count: 0, date: today() },
    ];
    assert!(is_active(&days, as_of()));
}

#[test]
fn not_active() {
    let days = vec![
        ContributionDay { contribution_count: 0, date: day(2) },
        ContributionDay { contribution_count: 0, date: day(1) },
    ];
    assert!(!is_active(&days, as_of()));
}

#[test]
fn active_empty() {
    assert!(!is_active(&[], as_of()));
}

// --- redistribute_languages ---

#[test]
fn redistribute_single_lang_unchanged() {
    let mut langs = vec![crate::models::Language { size: 100, name: "Rust".into(), color: None }];
    redistribute_languages(&mut langs);
    assert_eq!(langs[0].size, 100);
}

#[test]
fn redistribute_below_threshold_unchanged() {
    let mut langs = vec![
        crate::models::Language { size: 70, name: "Rust".into(), color: None },
        crate::models::Language { size: 30, name: "JS".into(), color: None },
    ];
    redistribute_languages(&mut langs);
    assert_eq!(langs[0].size, 70);
    assert_eq!(langs[1].size, 30);
}

#[test]
fn redistribute_dominant_language_two_only() {
    let mut langs = vec![
        crate::models::Language { size: 90, name: "Rust".into(), color: None },
        crate::models::Language { size: 10, name: "JS".into(), color: None },
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

// --- ShapingRules ---

#[test]
fn shaping_rules_exclusion_membership() {
    let rules = ShapingRules::new(vec!["me/private".into(), "me/other".into()]);
    assert!(rules.excludes("me/private"));
    assert!(rules.excludes("me/other"));
    assert!(!rules.excludes("me/public"));
    assert!(!ShapingRules::none().excludes("me/private"));
}

// --- flatten_contribution_days ---

#[test]
fn flatten_days_empty_calendar() {
    let cal = ContributionCalendar { weeks: vec![] };
    assert!(super::domain::flatten_contribution_days(&cal).is_empty());
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
    let days = super::domain::flatten_contribution_days(&cal);
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].date, "2024-01-01");
    assert_eq!(days[1].date, "2024-01-08");
}

// --- GraphQL variable plumbing (spec §3, §6): requested limits are used exactly ---

#[tokio::test]
async fn get_commits_list_uses_requested_limits_exactly() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

    let (_resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 5, 5, 3, &[]).await.unwrap();
    let variables = &fake.last_request_body()["variables"];
    assert_eq!(variables["firstRepos"], 5, "limit=5 must fetch exactly 5 repos — no clamp-up");
    assert_eq!(variables["firstCommits"], 5);
    assert_eq!(variables["firstLanguages"], 3);
}

#[tokio::test]
async fn get_commits_list_plumbs_language_limit() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

    let (_resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 10, 5, 7, &[]).await.unwrap();
    let variables = &fake.last_request_body()["variables"];
    assert_eq!(variables["firstRepos"], 10);
    assert_eq!(variables["firstCommits"], 5);
    assert_eq!(variables["firstLanguages"], 7);
}

#[tokio::test]
async fn get_commits_list_caps_repos_at_50_for_safety() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

    let (_resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 60, 5, 3, &[]).await.unwrap();
    assert_eq!(fake.last_request_body()["variables"]["firstRepos"], 50);
}

// --- HttpFetch seam integration tests ---

#[tokio::test]
async fn get_commits_list_empty_repos() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[]}}}}"#);

    let (resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 10, 5, 3, &[]).await.unwrap();
    assert!(resp.commits.is_empty());
}

#[tokio::test]
async fn get_commits_list_graphql_error() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":null,"errors":[{"message":"Not Found"}]}"#);

    let result = super::get_commits_list(&fake, "testuser", "token", 10, 5, 3, &[]).await;
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

    let (resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 10, 5, 3, &[]).await.unwrap();
    assert!(resp.commits.is_empty());
}

#[tokio::test]
async fn get_commits_list_unauthorized() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(401, r#"{"message":"Bad credentials"}"#);

    let result = super::get_commits_list(&fake, "testuser", "bad_token", 10, 5, 3, &[]).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::Unauthorized(crate::error::Unauthorized::GitHubApi(msg)) => assert!(msg.contains("Bad credentials")),
        _ => panic!("expected Unauthorized(GitHubApi)"),
    }
}

#[tokio::test]
async fn get_commits_list_exclusions_flow_through() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"repositories":{"nodes":[
        {"nameWithOwner":"me/private"},
        {"nameWithOwner":"me/public"}
    ]}}}}"#);

    let excluded = vec!["me/private".to_string()];
    let (resp, _cost) = super::get_commits_list(&fake, "testuser", "token", 10, 5, 3, &excluded).await.unwrap();
    assert!(resp.commits.is_empty());
    assert_eq!(resp.stats.total_commits, 0);
}

#[tokio::test]
async fn get_streak_info_empty_calendar() {
    let fake = FakeHttpFetch::new();
    fake.push_ok(200, r#"{"data":{"user":{"contributionsCollection":{"contributionCalendar":{"weeks":[]}}}}}"#);

    let (info, _) = super::get_streak_info(&fake, "testuser", "token").await.unwrap();
    assert_eq!(info.current_streak, 0);
    assert_eq!(info.highest_streak, 0);
    assert!(!info.active);
}
