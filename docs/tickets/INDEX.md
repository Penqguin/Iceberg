# Tickets — docs/specs/review-fixes.md

Local task graph (no external tracker). Status lives here; a ticket is **done**
when: checklist boxes in the spec are ticked, `cargo test` passes, and the work
is merged into the integration branch `spec/review-fixes`.

| Ticket | Title | Spec § | Blocked by | Status |
|--------|-------|--------|------------|--------|
| T1 | Cache failures degrade gracefully | §1 | — | open |
| T2 | Timeout budget covers the retry cycle | §5 | — | **implemented** |
| T3 | Param plumbing: language_limit, no clamp-up, suspicious-input logging | §3, §6, §9 | — | **implemented** |
| T4 | Response integrity: remove redistribute_languages, UUID X-Request-ID | §2, §4 | — | **implemented** |
| T5 | Test suite honesty: mockito integration test, real bench | §7, §8 | T2 | open |
| T6 | Docs & polish: dedupe handlers, document contract, GitHubCtx | §10, §11, §12 | T3, T4 | open |
