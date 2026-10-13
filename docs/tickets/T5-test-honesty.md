# T5 — Test suite honesty: mockito integration test, real bench

**Spec**: `docs/specs/review-fixes.md` §7, §8. **Blocked by**: T2 (both touch
`src/github/client.rs`; rebase on T2's merged work).

## Work
- Make the GraphQL URL overridable through one seam (parameter on
  `execute_with_retries`/`execute_graphql`, or an env var read in production) — NOT
  a globally mutable static. `get_commits_list`/`get_streak_info` need to accept it
  or the test goes through a builder.
- Rewrite `tests/integration/api_tests.rs` to actually use the mockito server:
  point the client at `server.url()`, POST a batch-shaped body, assert a well-formed
  GraphQL request was sent and the response parses into `ReposResponseData`.
- Add a partial-failure test: GraphQL `errors` array alongside partial `data` →
  `AppError::GitHubError`, matching `execute_graphql` behaviour.
- Delete the placeholder bench (`black_box(2 + 2)`) in `benches/performance.rs`;
  add a criterion bench for `shape_commits` over ~10 repos × 5 commits of synthetic
  data (pure fn, no worker runtime needed).
- Tick §7, §8 checkboxes in the spec when done.
