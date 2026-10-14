# Review Fixes Spec — Iceberg API

**Source**: two-axis code review (Standards + Spec) of `git diff e2f8f5e...HEAD`, adjusted
for the uncommitted refactor in the working tree. Several review findings were already
resolved by that refactor and are listed under "Resolved — do not re-implement".

**Rule**: this file stays in the repo. The previous spec
(`OPTIMIZATION_REQUIREMENTS.md`) was deleted mid-branch while its checklist was largely
unimplemented; that churn is the first thing this spec fixes.

---

## Resolved — do not re-implement

- ~~Lying bounds messages~~ — `parse_and_validate_query_params` now reports bounds that
  match its checks (limit 1–20, history_limit 1–75).
- ~~Hardcoded per-user exclusions in `get_excluded_repos`~~ — moved to config
  (`EXCLUDED_REPOS` env var, `OWNER_EXCLUDED_REPOS_DEFAULT`, `rules_for`).
- ~~Redundant query parsing / cache-key divergence between warmer and pipeline~~ —
  single `cache_key()` in `src/cache.rs` used by both.
- ~~God-module `lib.rs`~~ — split into `pipeline.rs`, `cache.rs`, `shaping.rs`, `error.rs`.
- ~~Parallel repo processing (old spec item)~~ — moot: the current architecture issues
  **one** GraphQL query per request; repos are never processed serially.
- ~~Cache-key whitelist bypass~~ — analysed: response content depends only on
  (endpoint, username, params), so sharing cache entries across auth modes is correct.
- ~~v1 schema change~~ — decided: keep the unified `CommitsListResponse` shape; v1 is a
  fixed-limits adapter (documented in this spec, item 11).

---

## P0 — Correctness & data integrity

### 1. Cache failures must degrade gracefully
**Where**: `src/pipeline.rs` `run_pipeline` step 8; `src/cache.rs`.

Currently `cache.get(&key).await?` (line 161) and `cache.put(...).await?` (line 186)
propagate failures and fail the whole request. The cache is an optimization; a cache
outage must never turn a working endpoint into a 500.

- [x] On cache GET failure: log a warning, treat as a miss, proceed uncached.
- [x] On cache PUT failure: log a warning, still return the response to the caller.
- [x] No error path in `run_pipeline` may originate from the cache store.
- [x] Unit test: a `FailingCache: CacheStore` whose `get`/`put` both return `Err` —
      request must still succeed with `X-Cache: MISS`.

### 2. Remove `redistribute_languages()` (response data falsification)
**Where**: `src/shaping.rs` lines 57–89, called at line 174.

`redistribute_languages()` silently moves ~40% of a dominant language's bytes onto other
languages when one is >80% dominant. Nothing in any spec asked for altering returned
data; it makes `/v2/commits/latest` report language sizes GitHub never returned.

- [x] Delete `redistribute_languages` and its call site.
- [x] Delete/adjust tests that assert on redistributed sizes.
- [x] `languages` in the response reflect raw GitHub roll-up only.

### 3. Stop silently clamping repo count up
**Where**: `src/github/domain.rs` line 60.

`"firstRepos": limit.max(10).min(50)` — a user passing `limit=5` still fetches 10 repos,
defeating the quota-reduction goal and breaking the v1 contract (v1 fixes `limit=5`).

- [x] `firstRepos` = the validated `limit` exactly (no `.max(10)`), capped at 50 for
      safety: `limit.min(50)`.
- [x] Update `src/github/tests.rs` to assert the GraphQL variables match the requested
      limits exactly.

### 4. `X-Request-ID` must be a UUID
**Where**: `src/lib.rs` line 45.

`format!("req-{}", worker::js_sys::Math::random())` is neither unique, unpredictable,
nor UUID-shaped. `Math::random()` can also repeat within a warm isolate.

- [x] Use the `uuid` crate (`v4` feature) to generate a real UUID per response.
- [x] Same value shared across all headers of one response (single generation point in
      `ResponseBuilder`).

### 5. Timeout budget must cover the whole retry cycle
**Where**: `src/github/client.rs` (`DEFAULT_TIMEOUT_SECS`, `execute_with_retries`).

The 15s timeout wraps only a single fetch attempt. With 5 retries and exponential
backoff (waits 1+2+4+8 = 15s plus per-attempt fetch time), total wall time is unbounded.

- [x] Add `REQUEST_TIMEOUT_SECS` env var (default 15, max 30) read in `get_config` and
      threaded to `execute_graphql` (the `timeout_secs: Option<u64>` param already
      exists — use it).
- [x] Enforce a **deadline** over the whole retry loop: once
      (attempt timeout + backoff elapsed) exceeds the budget, return
      `AppError::Timeout` immediately — no further attempts.
- [x] Unit test: a fetcher that always rate-limits exhausts the deadline and returns
      `Timeout`, not a retry that never ends.

### 6. `language_limit` query parameter
**Where**: `src/models.rs`, `src/pipeline.rs`, `src/github/domain.rs` line 62.

`firstLanguages` is hardcoded to 3 and unreachable by callers.

- [x] Add `language_limit: usize` to `QueryParams` (default 3, max 10, reject 0 or >10
      with the same `ValidationError` shape).
- [x] Plumb through `EndpointConfig.fixed_params` (v1: fixed at 3), the pipeline,
      `get_commits_list`, and the GraphQL variables.
- [x] Include it in `cache_key()` so differently-scoped requests don't collide.
- [x] Document in `static/docs.html` parameter table.

---

## P1 — Test suite honesty

### 7. Replace the stub integration test
**Where**: `tests/integration/api_tests.rs` (asserts `assert!(true)`).

`mockito` is a dependency but unused; `GITHUB_GRAPHQL_URL` is a hard const, so nothing
can point the client at the mock server.

- [x] Make the GraphQL URL overridable (env var or a parameter on
      `execute_with_retries` — one seam, not a global mutable).
- [x] Rewrite the test to build `RealHttpFetch`-equivalent requests against
      `server.url()`: assert a well-formed GraphQL request is sent and the response
      parses into `ReposResponseData`.
- [x] Add a partial-failure test: GraphQL `errors` array with partial `data` — must
      return `AppError::GitHubError`, matching `execute_graphql` behaviour.

### 8. Delete the placeholder benchmark
**Where**: `benches/performance.rs` — benches `black_box(2 + 2)`.

- [x] Delete it.
- [x] Add a real criterion bench for the pure hot path: `shape_commits` over ~10 repos
      × 5 commits of synthetic data (pure fn, no runtime needed).

---

## P2 — Observability & polish

### 9. Suspicious-input logging
**Where**: `src/pipeline.rs` `parse_and_validate_query_params`.

- [x] Log (warn-level) when validation rejects input: field name, reason, and the raw
      value's length for long usernames (> 39 chars, GitHub's login cap). Never log the
      Authorization header.

### 10. Deduplicate the v1/v2 handler closures
**Where**: `src/lib.rs` lines 142–178.

The two `get_async` closures are byte-identical apart from `fixed_params`.

- [x] Extract one handler fn taking `&EndpointConfig`; both routes call it.

### 11. Document the actual API contract
- [x] `static/docs.html`: state the enforced bounds (limit 1–20, history_limit 1–75,
      language_limit 1–10) and that v1 returns the same `CommitsListResponse` shape as
      v2 with fixed `limit=5, history_limit=5, language_limit=3`.
- [x] `GLOSSARY.md`: remove any entry describing removed shaping behaviour.

### 12. Bundle the GitHub context data clump (optional refactor)
`(..., username, token, ...)` travels through every `github::` fn signature. A small
`GitHubCtx<'a> { username, token }` (fetcher stays a separate generic param) would
shrink every signature. Judgement call — do it opportunistically, not as its own PR.

- [x] Done in T6: `GitHubCtx<'a>` lives in `src/github/domain.rs`, re-exported via
      `github::`. All call sites (3 route handlers, cron warmer, github tests)
      updated; `client.rs` (`execute_graphql`) untouched.

---

## Deferred (explicitly out of scope for now)

Reviewed 2026-10-14 against the shipped code (`main` @ `a4a8ae2`, deployed and
live-verified):

- **Batch GraphQL (`execute_graphql_batch()`)** — the old spec's premise (one request
  firing several queries) no longer holds: every request issues exactly ONE GraphQL
  query today, so request-path batching has no caller. The one place multiple queries
  still occur is the **cron warmer**, which fires one query per whitelisted user
  (`scheduled` → `join_all` over `config.whitelist`). Those could alias-batch into a
  single round trip (`u0: user(login:"a") { … } u1: user(login:"b") { … }`), but with
  the current whitelist of **one** user it buys nothing — not one round trip saved.
  *Trigger to re-open:* WHITELIST grows to ~2+ users (warmer does N round trips every
  5 min), or an endpoint that genuinely needs two datasets (e.g. commits + streak in
  one response). Until then: don't build it.
- **Cache hit/miss metrics** — code belongs in `src/pipeline.rs` and `src/logging.rs`,
  config switch in `wrangler.toml`. Built in two phases; **as of 2026-10-14, phase 1 is implemented** and shipped:
  1. *✅ Implemented*: one structured `log_info!` line per cache outcome in
     `run_pipeline` (`cache outcome=hit|miss endpoint=… username=…`), and cache
     failures now log as `cache outcome=error op=get|put key=…`. Workers
     Observability enabled in `wrangler.toml`, so the dashboard query API counts
     cache outcomes with no new bindings. No quota concerns.
  2. *Not built*: Workers Analytics Engine binding (`writeInflux` per request).
     Only if phase 1's log counts feel too coarse — it adds a dataset + binding
     dependency.
- **`AppError::CacheError` variant** — closed as dead by design, not merely deferred:
  the spec (T1) requires cache failures to degrade gracefully, so such an error is
  never allowed to reach a response. It would only ever classify a log line, which
  the existing `log_warn!("cache … failed for {key} …")` already does. Remove it
  from future considerations.
