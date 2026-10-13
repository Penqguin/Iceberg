# ADR-0001: Request cache stays on the Workers Cache API

**Status**: accepted · October 2026

## Context

The request cache (`src/cache.rs`) is a deep module: key derivation lives inside it
and storage is swappable at the `CacheStore` seam. An R2-backed adapter was
attempted once before (commit `9fac549`) and reverted (commit `8fab3c0`), which is
why the seam was built deliberately rather than assumed away.

## Decision

Keep a single runtime adapter: the Workers Cache API (`WorkersCache`). Do not add
an R2 adapter now.

## Rationale

- The Cache API does the job. The cron warmer (self-limiting via a warm check) plus
  a 300s edge TTL keeps whitelisted profiles continuously warm without paying for
  storage or keys.
- R2's advantage — persistence across deploys — has not shown up as a need. A
  deploy leaves a cold cache for a few minutes; the warmer refills it within one
  cron cycle.
- One adapter means the hinge is hypothetical for storage. The interface was worth
  *building* (the faster key divergence it prevents is a live bug class); the
  second adapter is not yet worth *feeding*.

## Consequences

- Deploys produce a short all-MISS window until the warmer runs.
- If edge-cache persistence ever becomes a requirement (e.g. deploy churn forcing
  repeated warm-fetching for many users), revisit and add an R2 adapter at the
  existing seam — no pipeline changes required.
