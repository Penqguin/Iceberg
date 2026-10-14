//! Shared fixtures: hand-rolled builders for the GitHub GraphQL domain types
//! (`GqlCommit`, `RepositoryNode`). One home instead of per-site copies in the
//! shaping unit tests, `benches/performance.rs`, and `src/github/tests.rs`.
//!
//! Compiled for lib unit tests (`cfg(test)`) and for the criterion bench via
//! the `bench` feature (`cargo bench --features bench`); invisible otherwise,
//! so nothing fixture-related ships in the production Worker.

use crate::github::types::*;
use chrono::{DateTime, Utc};

/// Author shape of a `GqlCommit`: absent author, present-but-empty user
/// field, or a user with a login.
enum AuthorSpec {
    None,
    UserField,
    Login(String),
}

/// Builder for `GqlCommit` with test-friendly defaults
/// (`oid "abc"`, additions 1, deletions 0, `commit_url "url"`, now,
/// headline "h", body "b", author "penqguin").
pub struct CommitFixture {
    abbreviated_oid: String,
    additions: usize,
    deletions: usize,
    commit_url: String,
    committed_date: DateTime<Utc>,
    message_headline: String,
    message_body: String,
    author: AuthorSpec,
}

impl CommitFixture {
    fn default() -> Self {
        Self {
            abbreviated_oid: "abc".into(),
            additions: 1,
            deletions: 0,
            commit_url: "url".into(),
            committed_date: Utc::now(),
            message_headline: "h".into(),
            message_body: "b".into(),
            author: AuthorSpec::Login("penqguin".into()),
        }
    }

    pub fn login(mut self, login: impl Into<String>) -> Self {
        self.author = AuthorSpec::Login(login.into());
        self
    }

    /// `author { user }` present but empty (no login).
    pub fn user_field_only(mut self) -> Self {
        self.author = AuthorSpec::UserField;
        self
    }

    pub fn author_none(mut self) -> Self {
        self.author = AuthorSpec::None;
        self
    }

    pub fn abbreviated_oid(mut self, oid: impl Into<String>) -> Self {
        self.abbreviated_oid = oid.into();
        self
    }

    pub fn additions(mut self, additions: usize) -> Self {
        self.additions = additions;
        self
    }

    pub fn deletions(mut self, deletions: usize) -> Self {
        self.deletions = deletions;
        self
    }

    pub fn commit_url(mut self, url: impl Into<String>) -> Self {
        self.commit_url = url.into();
        self
    }

    pub fn message_headline(mut self, headline: impl Into<String>) -> Self {
        self.message_headline = headline.into();
        self
    }

    pub fn message_body(mut self, body: impl Into<String>) -> Self {
        self.message_body = body.into();
        self
    }

    /// Explicit commit instant (the bench uses synthetic timestamps).
    pub fn at(mut self, when: DateTime<Utc>) -> Self {
        self.committed_date = when;
        self
    }

    /// Commit at noon UTC on a `YYYY-MM-DD` date (shaping tests).
    pub fn on(self, date: &str) -> Self {
        let dt = DateTime::parse_from_rfc3339(&format!("{}T12:00:00Z", date))
            .unwrap()
            .with_timezone(&Utc);
        self.at(dt)
    }

    pub fn build(self) -> GqlCommit {
        GqlCommit {
            abbreviated_oid: self.abbreviated_oid,
            additions: self.additions,
            deletions: self.deletions,
            commit_url: self.commit_url,
            committed_date: self.committed_date,
            message_headline: self.message_headline,
            message_body: self.message_body,
            author: match self.author {
                AuthorSpec::None => None,
                AuthorSpec::UserField => Some(CommitAuthor { user: None }),
                AuthorSpec::Login(login) => Some(CommitAuthor {
                    user: Some(AuthorUser { login }),
                }),
            },
        }
    }
}

/// A `GqlCommit` builder with defaults.
pub fn commit() -> CommitFixture {
    CommitFixture::default()
}

/// Builder for `RepositoryNode`: one repo, its language edges and the commit
/// history on its default branch. Languages and history are always present
/// (possibly empty), mirroring the GraphQL response shape.
pub struct RepoFixture {
    name_with_owner: String,
    languages: Vec<GitLanguageEdge>,
    commits: Vec<GqlCommit>,
}

/// A `RepositoryNode` builder with empty languages and history.
pub fn repo(name_with_owner: &str) -> RepoFixture {
    RepoFixture {
        name_with_owner: name_with_owner.into(),
        languages: Vec::new(),
        commits: Vec::new(),
    }
}

impl RepoFixture {
    pub fn language(mut self, name: &str, size: usize) -> Self {
        self.languages.push(GitLanguageEdge {
            size,
            node: GitLanguageNode {
                name: name.into(),
                color: None,
            },
        });
        self
    }

    pub fn commits(mut self, commits: Vec<GqlCommit>) -> Self {
        self.commits = commits;
        self
    }

    /// Convenience: push one authored commit (noon UTC on `date`).
    pub fn by(mut self, login: &str, date: &str, additions: usize) -> Self {
        self.commits
            .push(commit().login(login).on(date).additions(additions).build());
        self
    }

    pub fn build(self) -> RepositoryNode {
        RepositoryNode {
            name_with_owner: self.name_with_owner,
            languages: Some(GitLanguages {
                edges: Some(self.languages),
            }),
            default_branch_ref: Some(DefaultBranchRef {
                target: Some(GqlTargetCommit {
                    history: GqlHistory {
                        edges: Some(
                            self.commits
                                .into_iter()
                                .map(|node| GqlCommitEdge { node })
                                .collect(),
                        ),
                    },
                }),
            }),
        }
    }
}
