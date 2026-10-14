# T4 — Response integrity: remove redistribute_languages, UUID X-Request-ID

**Spec**: `docs/specs/review-fixes.md` §2, §4. **Blocked by**: nothing.

## Work
- `src/shaping.rs`: delete `redistribute_languages()` (lines ~57–89) and its call
  site in `shape_commits`; delete/adjust tests asserting redistributed sizes.
  `languages` must reflect the raw GitHub roll-up only.
- `src/lib.rs` `ResponseBuilder::set_cors_headers`: replace
  `format!("req-{}", Math::random())` with a real UUID via the `uuid` crate
  (add `uuid = { version = "1", features = ["v4", "js"] }` to Cargo.toml — the `js`
  feature matters for wasm32). Generate once per response; all headers of one
  response share the value.
- Note: `set_cors_headers` is called per-response in several places; ensure each
  response gets a fresh UUID.
- Tick §2, §4 checkboxes in the spec when done.
