# T1 — Cache failures degrade gracefully

**Spec**: `docs/specs/review-fixes.md` §1. **Blocked by**: nothing.

Cache is an optimization; a cache outage must never fail a request.

## Work
- `src/pipeline.rs` `run_pipeline`: cache GET failure → log warning, treat as miss,
  proceed uncached; cache PUT failure → log warning, still return the response.
  No `?` on any cache call in the pipeline.
- `src/lib.rs` `scheduled`: the warmer's `cache.get`/`cache.put` already use
  `if let Ok(...)` — verify they stay non-fatal.
- Add a `FailingCache: CacheStore` test double in `src/pipeline.rs` tests whose
  `get`/`put` return `Err`; assert a full `run_pipeline` request still succeeds with
  `X-Cache: MISS`. (See existing test seams for how the pipeline is unit-tested.)
- Tick the §1 checkboxes in the spec when done.
