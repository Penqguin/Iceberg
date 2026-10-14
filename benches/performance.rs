//! Criterion bench for the pure hot path: `shape_commits` over ~10 repos ×
//! 5 commits of synthetic data (no worker runtime, no I/O).
//!
//! Uses the shared fixture module (`src/test_fixtures.rs`), which requires the
//! `bench` feature: run with `cargo bench --features bench`.

use chrono::{TimeZone, Utc};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iceberg::github::types::RepositoryNode;
use iceberg::shaping::{shape_commits, ShapingRules};
use iceberg::test_fixtures::{commit, repo};

/// A repo with 3 languages and `commit_count` commits by the bench author.
fn synthetic_repo(index: usize, commit_count: usize) -> RepositoryNode {
    let commits = (0..commit_count)
        .map(|i| {
            commit()
                .abbreviated_oid(format!("{:07}", index * 1_000 + i))
                .additions(10 * (i % 7))
                .deletions(3 * (i % 5))
                .commit_url(format!("https://github.com/bench-user/repo-{}/commit/{}", index, i))
                .at(Utc.timestamp_opt(1_700_000_000 + 3_600 * (commit_count as i64 - i as i64), 0).unwrap())
                .message_headline(format!("commit headline {}", i))
                .message_body(format!("commit body {}", i))
                .login("bench-user")
                .build()
        })
        .collect();

    repo(&format!("bench-user/repo-{}", index))
        .language("Rust", 60)
        .language("TypeScript", 30)
        .language("Python", 10)
        .commits(commits)
        .build()
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
