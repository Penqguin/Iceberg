//! Criterion bench for the pure hot path: `shape_commits` over ~10 repos ×
//! 5 commits of synthetic data (no worker runtime, no I/O).

use chrono::{TimeZone, Utc};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iceberg::github::types::{
    AuthorUser, CommitAuthor, DefaultBranchRef, GitLanguageEdge, GitLanguages, GitLanguageNode,
    GqlCommit, GqlCommitEdge, GqlHistory, GqlTargetCommit, RepositoryNode,
};
use iceberg::shaping::{shape_commits, ShapingRules};

/// A repo with 3 languages and `commit_count` commits by the bench author.
fn synthetic_repo(index: usize, commit_count: usize) -> RepositoryNode {
    let commits: Vec<GqlCommit> = (0..commit_count)
        .map(|i| {
            GqlCommit {
                abbreviated_oid: format!("{:07}", index * 1_000 + i),
                additions: 10 * (i % 7),
                deletions: 3 * (i % 5),
                commit_url: format!("https://github.com/bench-user/repo-{}/commit/{}", index, i),
                committed_date: Utc.timestamp_opt(1_700_000_000 + 3_600 * (commit_count as i64 - i as i64), 0).unwrap(),
                message_headline: format!("commit headline {}", i),
                message_body: format!("commit body {}", i),
                author: Some(CommitAuthor { user: Some(AuthorUser { login: "bench-user".into() }) }),
            }
        })
        .collect();

    RepositoryNode {
        name_with_owner: format!("bench-user/repo-{}", index),
        languages: Some(GitLanguages {
            edges: Some(vec![
                GitLanguageEdge { size: 60, node: GitLanguageNode { name: "Rust".into(), color: None } },
                GitLanguageEdge { size: 30, node: GitLanguageNode { name: "TypeScript".into(), color: None } },
                GitLanguageEdge { size: 10, node: GitLanguageNode { name: "Python".into(), color: None } },
            ]),
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

fn bench_shape_commits(c: &mut Criterion) {
    let rules = ShapingRules::none();
    let repos: Vec<RepositoryNode> = (0..10)
        .map(|i| synthetic_repo(i, 5))
        .collect();

    c.bench_function("shape_commits/10_repos_x_5_commits", |b| {
        b.iter(|| {
            shape_commits(
                black_box(repos.clone()),
                black_box("bench-user"),
                black_box(20),
                black_box(75),
                black_box(&rules),
            )
        })
    });
}

criterion_group!(benches, bench_shape_commits);
criterion_main!(benches);
