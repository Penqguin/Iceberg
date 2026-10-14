# Glossary

Domain and architecture vocabulary for iceberg. Use these terms in code, docs, and reviews.

## Domain

**Profile** — a GitHub user's public activity as served by this API: recent commits, language roll-up, and streak, shaped by [shaping rules](#shaping-rules) before it leaves the edge.

**Shaping rule** — a product decision applied to raw GitHub data: excluding curated repos and choosing which commits feed the stats window. Language totals are a raw roll-up — never re-shaped or redistributed. All shaping lives in `src/shaping.rs` — nowhere else.

**Exclusion list** — the repos hidden from an owner profile's response (default: `penqguin/penqguin`, `penqguin/ONotes`; env `EXCLUDED_REPOS`). Applies only to whitelisted (owner) users. Standing assumption: the whitelist is composed of the owner's own accounts — if it ever grows to include someone else, exclusions must be pinned to explicit owner usernames (e.g. a `WHITELIST_OWNERS` env) instead of whitelist membership.

**Stats window** — the most recent `history_limit` of the profile's commits; `total_additions`, `total_deletions`, and the language roll-up count only within it. `total_commits` counts all collected commits.

**Streak** — consecutive days (ending today or yesterday) with ≥1 contribution. A zero-contribution *today* does not break a streak that ends yesterday; a zero-*yesterday* does.

**Query cost** — GitHub's reported GraphQL cost of a request, surfaced as `X-Query-Cost`.

## Architecture

**Request cache** — the edge cache keyed by `(endpoint, username, limit, history_limit, language_limit)`. Key derivation lives in `src/cache.rs` (`cache_key`) and must never be duplicated elsewhere — the pipeline and the cron warmer both derive keys through it. Storage swaps at the `CacheStore` seam (`WorkersCache` adapter today; R2 adapter previously attempted and reverted, so the seam is real). The Workers Cache API only accepts fully-qualified URLs as keys, so `cache_key` materializes the tuple as a synthetic `https://cache.iceberg/**` URL; a non-URL key makes every cache op fail and caching silently never happens.

**Request pipeline** — the per-request choreography in `src/pipeline.rs` (`run_pipeline`): config → params → cache → auth → handler → response → cache-store. Owns the `X-Cache` and `Cache-Control` headers.

**HttpFetch seam** — the one-method interface (`post_json`) between GitHub logic and the network, with `RealHttpFetch` and a test fake as adapters, and retry/timeout/error classification hidden behind `execute_graphql`. The model seam in this codebase.
