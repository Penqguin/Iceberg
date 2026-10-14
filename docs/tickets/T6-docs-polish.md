# T6 — Docs & polish: dedupe handlers, document contract, GitHubCtx

**Spec**: `docs/specs/review-fixes.md` §10, §11, §12. **Blocked by**: T3 (docs table
needs language_limit; handler dedupe must match new fixed_params shape), T4 (lib.rs
overlap).

## Work
- `src/lib.rs`: the two `get_async` closures for `/commits/latest` and
  `/v2/commits/latest` are identical apart from `fixed_params` — extract one shared
  handler fn taking `&EndpointConfig`.
- `static/docs.html`: document enforced bounds (limit 1–20, history_limit 1–75,
  language_limit 1–10) and that v1 returns the same `CommitsListResponse` shape as
  v2 with fixed `limit=5, history_limit=5, language_limit=3`.
- `GLOSSARY.md`: remove/adjust any entry describing the removed language-shaping
  behaviour (see T4).
- Optional (§12): bundle `(username, token)` into a small `GitHubCtx<'a>` passed
  through the `github::` fns — do it only if it stays a small diff; otherwise leave
  a note in the ticket and skip.
- Tick §10, §11 (§12 if done) checkboxes in the spec when done.
