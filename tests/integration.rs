//! The `integration` test target: real-HTTP tests against a mockito server.
//! Test files live in `integration/` and are pulled in as modules here.
#[path = "integration/api_tests.rs"]
mod api_tests;
