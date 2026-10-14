# T2 — Timeout budget covers the retry cycle

**Spec**: `docs/specs/review-fixes.md` §5. **Blocked by**: nothing.

The 15s timeout wraps one fetch attempt; with 5 retries + backoff (1+2+4+8s) the
request's total wall time is unbounded.

## Work
- `src/pipeline.rs` `get_config`: read `REQUEST_TIMEOUT_SECS` env var (default 15,
  max 30 — clamp, don't reject) into `AppConfig`; thread it through `run_pipeline`
  to the handler layer.
- `src/github/client.rs`: `execute_graphql` already takes `timeout_secs: Option<u64>`
  — make `get_commits_list`/`get_streak_info` pass it through instead of `None`.
- Enforce a **deadline over the whole retry loop** in `execute_with_retries`: track
  elapsed time (attempt timeouts + backoff sleeps); once the budget is spent, return
  `AppError::Timeout` immediately, no further attempts.
- Unit tests (native, tokio-time based — see existing `#[cfg(test)] sleep` seam):
  - a fetcher that always rate-limits exhausts the deadline → `AppError::Timeout`
    (not an endless retry);
  - a valid response after 2 rate-limited attempts still succeeds;
  - env clamping logic (0/negative → default; >30 → 30) at the config level.
- Tick the §5 checkboxes in the spec when done.
