# T3 — Param plumbing: language_limit, no clamp-up, suspicious-input logging

**Spec**: `docs/specs/review-fixes.md` §3, §6, §9. **Blocked by**: nothing.

## Work
- `src/models.rs`: add `language_limit: usize` to `QueryParams`.
- `src/pipeline.rs` `parse_and_validate_query_params`: parse `language_limit`
  (default 3, reject 0 or >10 with `AppError::ValidationError`); extract the shared
  parse-validate shape for limit/history_limit/language_limit into one helper
  (three copy-pasted blocks today). Also log a warning when any validation rejects
  input (field, reason, and raw value length for usernames > 39 chars — never log
  Authorization headers).
- `EndpointConfig.fixed_params`: becomes a 3-tuple (or small struct) so v1 fixes
  `language_limit = 3`; update the routes in `src/lib.rs`.
- `src/cache.rs` `cache_key`: include `language_limit`.
- `src/github/domain.rs` `get_commits_list`: take `language_limit` param;
  `"firstRepos": limit.min(50)` — drop the `.max(10)` clamp-up entirely so the
  validated limit (v1: 5) is used exactly; `"firstLanguages": language_limit`.
- Update `src/github/tests.rs` to assert GraphQL variables match requested limits
  exactly (including a test that limit=5 produces `firstRepos: 5`).
- Tick §3, §6, §9 checkboxes in the spec when done.
