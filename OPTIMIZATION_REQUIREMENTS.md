# Iceberg API Performance Optimization & Testing Requirements

## Objective
Improve API response times and reliability while maintaining backward compatibility with existing endpoints.

## Current Performance Issues
1. **Inefficient GraphQL queries**: Fetching too much data per request (20 repos, 10 commits, 10 languages)
2. **Cache inefficiency**: New `Cache::default()` instantiated per request
3. **Sequential processing**: Repositories processed serially instead of in parallel
4. **Redundant operations**: Query parameter parsing repeated across handlers
5. **No timeout protection**: Long-running GitHub API calls can hang indefinitely

## Performance Improvements (Priority Order)

### 1. **Batch GraphQL Queries** (High Impact - 30-40% latency reduction)
**Goal**: Combine multiple separate GraphQL requests into a single batch query.

**Implementation Requirements**:
- [ ] Create new function `execute_graphql_batch()` that accepts multiple query operations
- [ ] Modify `/commits/latest`, `/v2/commits/latest`, and `/streak` endpoints to share results from a single batch query when possible
- [ ] For single-request endpoints, maintain backward compatibility but allow batch execution
- [ ] Add rate-limit header extraction and logging
- [ ] Implement exponential backoff for rate limiting (retry with 2^n seconds delay, max 5 retries)

**Error Handling**:
- [ ] Handle partial batch failures (one operation fails, others succeed)
- [ ] Return meaningful error messages indicating which operation failed
- [ ] Log batch operation metrics (total_time, queries_per_second consumed)
- [ ] Add error variant `AppError::RateLimited(retry_after_seconds)`

**Testing**:
- [ ] Unit test: Verify batch query construction and serialization
- [ ] Unit test: Verify partial failure handling (1 of 3 queries fails)
- [ ] Integration test: Mock GitHub API returning batch response
- [ ] Performance benchmark: Compare batch vs sequential request latency
  - Expected: batch 1/3 the latency of 3 sequential requests

---

### 2. **Reduce Query Scope** (High Impact - 20-30% API quota savings)
**Goal**: Fetch only necessary data from GitHub GraphQL API.

**Implementation Requirements**:
- [ ] Make query limits configurable via query parameters: `limit`, `history_limit`, `language_limit`
- [ ] Set safe defaults:
  - `firstRepos`: 10 (was 20) with max 50
  - `firstCommits`: 5 (was 10) with max 20
  - `firstLanguages`: 3 (was 10) with max 10
- [ ] Add validation: reject if limits exceed max or are negative
- [ ] For `/commits/latest` endpoint (v1), hardcode to `limit=5` (was 10)
- [ ] Return `X-Query-Cost` header indicating GitHub GraphQL cost units consumed

**Error Handling**:
- [ ] Return 400 with specific message: `"limit parameter must be between 1 and {max}"`
- [ ] Handle GitHub API errors for invalid query operations
- [ ] Log when queries are rejected due to invalid parameters

**Testing**:
- [ ] Unit test: Validate parameter bounds (0, 1, max, max+1)
- [ ] Unit test: Verify GraphQL query construction with different limits
- [ ] Integration test: Confirm response size reduction (measure bytes)
- [ ] Benchmark: Compare API quota usage before/after
  - Expected: 50-60% quota savings

---

### 3. **Optimize Cache Strategy** (Medium Impact - 10-15% latency reduction)
**Goal**: Reduce cache instantiation overhead and improve hit rates.

**Implementation Requirements**:
- [ ] Create static `CACHE` instance (thread-safe) to avoid repeated initialization
- [ ] Enhance cache key generation: include query parameters and user whitelist status
- [ ] Add cache invalidation headers: `Cache-Control: s-maxage=300` (was 60, increase to 5 min)
- [ ] Implement cache metrics:
  - Track hit/miss ratio per endpoint
  - Log cache operations in debug builds
- [ ] Add cache warming: scheduled event should pre-warm cache for all whitelisted users

**Error Handling**:
- [ ] Handle cache PUT failures gracefully (log but don't fail request)
- [ ] Add fallback if cache GET times out (>2 seconds, proceed with API call)
- [ ] Log cache errors with error variant `AppError::CacheError(String)`

**Testing**:
- [ ] Unit test: Cache hit/miss behavior with same/different query params
- [ ] Unit test: Cache key generation is deterministic
- [ ] Integration test: Verify cache persistence across requests
- [ ] Benchmark: Measure cache hit ratio after running 100 requests
  - Expected: >80% hit ratio for repeated queries

---

### 4. **Parallel Repository Processing** (Medium Impact - 15-25% latency reduction)
**Goal**: Process repositories concurrently instead of serially.

**Implementation Requirements**:
- [ ] Use `futures::stream::FuturesUnordered` or `tokio::task::join_all()` for parallel processing
- [ ] Process up to 5 repositories concurrently (configurable via env var `MAX_CONCURRENT_REPOS`)
- [ ] Add concurrency limit safety: fail gracefully if task spawning fails
- [ ] Maintain order of results in final output (sorted by date after parallel collection)

**Error Handling**:
- [ ] If one repo fetch fails, continue with others (don't cascade failure)
- [ ] Log which repos failed individually
- [ ] Return warning in response header if some repos failed: `X-Partial-Results: true`
- [ ] Count failed repos and report in logs

**Testing**:
- [ ] Unit test: Verify concurrent processing produces same results as serial
- [ ] Unit test: Handle failed repo fetch mid-stream
- [ ] Load test: Process user with 50+ repos, verify no timeouts
- [ ] Benchmark: Measure latency with concurrency level 1, 5, 10
  - Expected: Linear improvement up to 5, diminishing returns beyond

---

### 5. **Request Timeout Protection** (High Impact - Reliability)
**Goal**: Prevent indefinite hangs on slow GitHub API responses.

**Implementation Requirements**:
- [ ] Add `REQUEST_TIMEOUT_SECS` env var (default: 15s, max: 30s)
- [ ] Wrap all GitHub API calls in timeout wrapper
- [ ] Implement timeout as part of `execute_graphql()` function
- [ ] Add timeout error variant: `AppError::Timeout(String)`

**Error Handling**:
- [ ] Return 504 (Gateway Timeout) on GraphQL request timeout
- [ ] Return descriptive message: `"GitHub API request timed out after 15 seconds"`
- [ ] Log timeout occurrence with request context (operation, username)
- [ ] Track timeout metrics: count per hour, alert if >10 in 1 hour

**Testing**:
- [ ] Unit test: Mock slow GitHub API response, verify timeout fires
- [ ] Integration test: Verify timeout at 15s, not at 14s or 16s
- [ ] Load test: Concurrent requests with simulated slow API
  - Expected: Requests timeout cleanly, others unaffected

---

### 6. **Query Parameter Caching & Validation** (Low Impact - Code Quality)
**Goal**: Avoid re-parsing query parameters and centralizing validation.

**Implementation Requirements**:
- [ ] Create struct `QueryParams` with fields: `username`, `limit`, `history_limit`, `language_limit`
- [ ] Create function `parse_and_validate_query_params()` that:
  - Parses URL query string once
  - Validates all parameters
  - Returns `Result<QueryParams, AppError>`
- [ ] Use in all route handlers instead of inline parsing
- [ ] Add derivable traits: `Clone`, `Debug`, `Serialize`

**Error Handling**:
- [ ] Validate username is not empty and matches `[a-zA-Z0-9\-]+` regex
- [ ] Return 400 with specific parameter that failed
- [ ] Log suspicious patterns (very long usernames, negative numbers)

**Testing**:
- [ ] Unit test: Valid parameter combinations
- [ ] Unit test: Invalid parameters (negative, too long, special chars)
- [ ] Unit test: Missing required parameters
- [ ] Fuzz test: Random parameter values

---

## Error Handling Standards

All changes must implement consistent error handling:

### Error Response Format
```json
{
  "error": "Human-readable error message",
  "error_code": "SPECIFIC_ERROR_CODE",
  "retry_after": 60  // only for rate limits
}
```

### Required Error Variants
- `AppError::RateLimited(retry_after_secs)` → 429 status
- `AppError::Timeout(reason)` → 504 status
- `AppError::CacheError(msg)` → log but return 500 or proceed
- `AppError::ValidationError(field, reason)` → 400 status

### Error Response Headers
```
X-Error-Code: RATE_LIMITED
X-Retry-After: 60
X-Request-ID: {uuid} (for debugging)
```

### Logging
- All errors must log with context: username, endpoint, duration
- Debug logs for cache operations
- Warn logs for partial failures
- Error logs with stack traces for GitHub API errors

---

## Testing Requirements

### Unit Tests
**Location**: `tests/unit/` and inline `#[cfg(test)]` modules

**Coverage Targets**:
- [ ] `github.rs`: Query construction, parameter validation, response parsing
- [ ] `error.rs`: Error serialization, HTTP status mapping
- [ ] `models.rs`: Serialization/deserialization roundtrips

**Example Test Structure**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_query_parameter_validation_rejects_negative_limit() {
        let result = parse_and_validate_query_params("username=test&limit=-5");
        assert!(result.is_err());
    }
}
```

### Integration Tests
**Location**: `tests/integration/`

**Requirements**:
- [ ] Mock GitHub GraphQL API responses using `mockito` crate
- [ ] Test happy path: valid request → correct response
- [ ] Test error paths: GitHub API errors, timeouts, rate limits
- [ ] Test caching: second request returns cached response
- [ ] Test batch queries: confirm single batch uses less quota than sequential

**Example**:
```rust
#[tokio::test]
async fn test_commits_endpoint_caches_response() {
    let mock = server.mock(...)
        .expect_once()
        .with_body(...)
        .create();
    
    // First request hits API
    let resp1 = client.get("/v2/commits/latest?username=test").send().await;
    // Second request uses cache (no additional mock call)
    let resp2 = client.get("/v2/commits/latest?username=test").send().await;
    
    assert!(resp1.json::<CommitsListResponse>() == resp2.json::<CommitsListResponse>());
}
```

### Benchmarks
**Location**: `benches/`

**Benchmarks to Add**:
- [ ] Latency: `/v2/commits/latest` with 5 repos vs 20 repos
- [ ] Cache hit vs miss (same query twice)
- [ ] Batch query vs sequential (3 endpoints)
- [ ] Parallel vs serial repo processing

**Tool**: Use `criterion` crate

**Acceptance Criteria**:
- Batch queries: 2-3x faster than sequential
- Cache hits: <50ms response time
- Reduced scope: 30-40% fewer API calls
- Parallel processing: linear improvement up to 5 concurrent

---

## Backward Compatibility

### Guaranteed
- [ ] All existing endpoints (`/commits/latest`, `/v2/commits/latest`, `/streak`) remain unchanged
- [ ] Response schema unchanged (same fields, same types)
- [ ] Default parameter values maintain current behavior
- [ ] CORS headers unchanged

### Deprecation Path
If query limits need to be hardcoded later:
- Deprecation period: 6 weeks warning
- Response header: `Deprecation: true`, `Sunset: <date>`
- Docs updated with migration guide

---

## Deployment & Monitoring

### Pre-Deployment Checklist
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Benchmarks show expected improvements
- [ ] Code coverage >80% for new functions
- [ ] No breaking changes to API schema

### Monitoring (Post-Deployment)
- [ ] Track endpoint latency (p50, p95, p99) for 1 week
- [ ] Monitor cache hit ratio (target: >80%)
- [ ] Track GitHub API quota usage (should decrease)
- [ ] Alert on timeout rate >1% per endpoint
- [ ] Monitor error rates per endpoint

### Rollback Plan
- Keep previous version deployable for 2 weeks
- If latency regression >15%, rollback immediately
- Keep cache disabled flag for emergency toggle

---

## Success Metrics

| Metric | Current | Target | Measurement |
|--------|---------|--------|-------------|
| `/v2/commits/latest` p95 latency | ~2000ms | <600ms | CloudFlare analytics |
| GitHub API quota per request | 20-50 units | 5-15 units | GraphQL response headers |
| Cache hit ratio | 0% | >80% | Request logs |
| Timeout errors | N/A | <1% | Error tracking |
| Error rate | ~2% | <0.5% | HTTP 5xx count |

---

## Implementation Order
1. Add timeout protection (foundational, unblocks testing)
2. Add batch query support (biggest latency win)
3. Reduce query scope (biggest quota win)
4. Optimize caching (steady wins)
5. Parallel processing (complexity, less impact)
6. Parameter validation (polish)

---

## Code Review Checklist
- [ ] No `TODO` comments without issue reference
- [ ] Error handling covers all paths
- [ ] Tests provide >80% coverage for modified files
- [ ] No breaking API changes
- [ ] Logs include sufficient context for debugging
- [ ] Performance benchmarks show expected improvements
