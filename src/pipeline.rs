use crate::cache::{cache_key, CacheStore};
use crate::error::AppError;
use crate::log_warn;
use crate::models::{QueryParams, ResolvedAuth};
use crate::shaping::ShapingRules;
use crate::ResponseBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use worker::*;

static USERNAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap());

const OWNER_EXCLUDED_REPOS_DEFAULT: &str = "penqguin/penqguin,penqguin/ONotes";

pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 15;
pub const MAX_REQUEST_TIMEOUT_SECS: u64 = 30;

pub struct AppConfig {
    pub github_token: String,
    pub whitelist: Vec<String>,
    /// Repos hidden from the profile of any whitelisted (owner) user.
    pub excluded_repos: Vec<String>,
    /// Whole-request timeout budget in seconds (see §5 of the review-fixes spec).
    pub request_timeout_secs: u64,
}

impl AppConfig {
    /// Shaping rules for a given profile username: only whitelisted (owner)
    /// users carry an exclusion list.
    pub fn rules_for(&self, username: &str) -> ShapingRules {
        if self.whitelist.contains(&username.to_lowercase()) {
            ShapingRules::new(self.excluded_repos.clone())
        } else {
            ShapingRules::none()
        }
    }
}

/// `REQUEST_TIMEOUT_SECS`: default 15, max 30 — clamp rather than reject.
/// Unset, unparseable, zero, or negative all fall back to the default.
pub fn parse_request_timeout_secs(raw: Option<&str>) -> u64 {
    match raw.map(|s| s.trim().parse::<i64>()) {
        Some(Ok(secs)) if secs > 0 => secs.min(MAX_REQUEST_TIMEOUT_SECS as i64) as u64,
        _ => DEFAULT_REQUEST_TIMEOUT_SECS,
    }
}

pub fn get_config(env: &Env) -> std::result::Result<AppConfig, AppError> {
    let github_token = env.var("GITHUB_TOKEN")
        .map(|v| v.to_string())
        .map_err(|_| AppError::Internal("System GITHUB_TOKEN is not configured".to_string()))?;

    let whitelist_str = env.var("WHITELIST")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "penqguin".to_string());

    let whitelist: Vec<String> = whitelist_str
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let excluded_repos: Vec<String> = env.var("EXCLUDED_REPOS")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| OWNER_EXCLUDED_REPOS_DEFAULT.to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let request_timeout_secs = parse_request_timeout_secs(
        env.var("REQUEST_TIMEOUT_SECS").ok().map(|v| v.to_string()).as_deref(),
    );

    Ok(AppConfig { github_token, whitelist, excluded_repos, request_timeout_secs })
}

pub fn parse_and_validate_query_params(url: &Url) -> std::result::Result<QueryParams, AppError> {
    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();

    let username = query.get("username")
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| rejected_input("username", "is required", None))?;

    if !USERNAME_REGEX.is_match(&username) {
        return Err(rejected_input("username", "contains invalid characters", Some(&username)));
    }

    let limit = parse_bounded_limit(&query, "limit", 10, 20)?;
    let history_limit = parse_bounded_limit(&query, "history_limit", 10, 75)?;
    let language_limit = parse_bounded_limit(&query, "language_limit", 3, 10)?;

    Ok(QueryParams { username, limit, history_limit, language_limit })
}

/// GitHub caps logins at 39 characters. Longer rejected usernames are logged
/// by length only, never by value.
const MAX_GITHUB_LOGIN_LEN: usize = 39;

/// Record a rejected query parameter and return its ValidationError. Logs the
/// field name and reason; for over-long usernames only the raw value's length
/// (never the value, and never any Authorization data).
fn rejected_input(field: &str, reason: &str, raw: Option<&str>) -> AppError {
    match (field, raw) {
        ("username", Some(value)) if value.len() > MAX_GITHUB_LOGIN_LEN => {
            log_warn!("Rejected query param: field=username reason={} (username length {} — value withheld)", reason, value.len());
        }
        _ => {
            log_warn!("Rejected query param: field={} reason={}", field, reason);
        }
    }
    AppError::ValidationError(field.to_string(), reason.to_string())
}

/// Shared parse-validate shape for the limit-style parameters (`limit`,
/// `history_limit`, `language_limit`): optional, must be a number, must be
/// in 1..=max. Absent means `default`.
fn parse_bounded_limit(
    query: &std::collections::HashMap<String, String>,
    field: &str,
    default: usize,
    max: usize,
) -> std::result::Result<usize, AppError> {
    match query.get(field) {
        None => Ok(default),
        Some(raw) => match raw.parse::<usize>() {
            Err(_) => Err(rejected_input(field, "must be a number", None)),
            Ok(n) if n == 0 || n > max => {
                Err(rejected_input(field, &format!("must be between 1 and {}", max), None))
            }
            Ok(n) => Ok(n),
        },
    }
}

// --- Pipeline types ---

/// Materialization of pipeline outcomes, split from the worker runtime so
/// unit tests can drive the full request choreography without a runtime
/// (runtime calls panic natively). The production impl builds `Response`;
/// the `#[cfg(test)]` impl builds `TestResponse` via the lib test seam.
pub(crate) trait PipelineOut {
    type Out;
    /// Success shell: X-Query-Cost + Cache-Control lifecycle headers.
    fn success<D: serde::Serialize>(data: &D, query_cost: u64, ttl_secs: u64) -> Result<Self::Out>;
    /// Error shell from an AppError.
    fn error(err: &AppError) -> Result<Self::Out>;
    /// Stamp a lifecycle header, overriding any existing value.
    fn stamp(out: &mut Self::Out, key: &str, value: &str) -> Result<()>;
    /// An unmodified copy (e.g. the `X-Cache: HIT` twin handed to the store).
    fn clone_copy(out: &mut Self::Out) -> Result<Self::Out>;
}

/// Production materialization: real worker `Response`s.
pub(crate) struct ResponseOut;

impl PipelineOut for ResponseOut {
    type Out = Response;

    fn success<D: serde::Serialize>(data: &D, query_cost: u64, ttl_secs: u64) -> Result<Response> {
        ResponseBuilder::new()
            .with_header("X-Query-Cost", query_cost.to_string())
            .with_header("Cache-Control", format!("s-maxage={}", ttl_secs))
            .json(data)
    }

    fn error(err: &AppError) -> Result<Response> {
        ResponseBuilder::from_error(err)
    }

    fn stamp(out: &mut Response, key: &str, value: &str) -> Result<()> {
        out.headers_mut().set(key, value)
    }

    fn clone_copy(out: &mut Response) -> Result<Response> {
        out.cloned()
    }
}

pub struct CachePolicy {
    pub ttl_secs: u64,
}

/// The limit triple pinned by username-only endpoints (v1), with one honest
/// name instead of a bare `(usize, usize, usize)` tuple.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedParams {
    pub limit: usize,
    pub history_limit: usize,
    pub language_limit: usize,
}

impl FixedParams {
    /// Overwrite the parsed query-param limits with the endpoint's fixed ones.
    fn apply_to(self, params: &mut QueryParams) {
        params.limit = self.limit;
        params.history_limit = self.history_limit;
        params.language_limit = self.language_limit;
    }
}

/// Endpoint configuration. `fixed_params` is for username-only endpoints
/// (v1) whose limits don't respond to query parameters; `None` means full
/// parameter support (v2).
pub struct EndpointConfig {
    /// Cache-key namespace, e.g. "/v2/commits/latest".
    pub name: &'static str,
    pub cache: CachePolicy,
    pub fixed_params: Option<FixedParams>,
}

pub struct RequestContext {
    pub auth: ResolvedAuth,
    pub params: QueryParams,
    pub rules: ShapingRules,
    /// Whole-request timeout budget passed through to the GitHub layer.
    pub timeout_secs: u64,
}

/// Core pipeline: config → params → cache-check → auth → handler → response → cache-store.
///
/// Header ownership:
///   - Global/Structural (Access-Control-*, X-Request-ID) — ResponseBuilder
///   - Lifecycle/State (X-Cache, Cache-Control) — this function
///   - Domain/Business (X-Query-Cost) — passed from handler via with_header
pub async fn run_pipeline<C, H, Fut, D>(
    req: &Request,
    env: &Env,
    cache: &C,
    endpoint: &EndpointConfig,
    handler: H,
) -> Result<Response>
where
    C: CacheStore<Response>,
    H: FnOnce(RequestContext) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(D, u64), AppError>>,
    D: serde::Serialize,
{
    // Thin runtime adapter: everything after these reads is testable without
    // a worker runtime.
    let url = req.url()?;
    let auth_header = req.headers().get("Authorization")?;

    let config = match get_config(env) {
        Ok(c) => c,
        Err(e) => return ResponseOut::error(&e),
    };

    run_pipeline_steps::<ResponseOut, _, _, _, _>(url, auth_header, config, cache, endpoint, handler).await
}

/// The request choreography, independent of the worker runtime. `Out`
/// materializes outcomes: `ResponseOut` produces real `Response`s; tests
/// drive `TestOut` (`TestResponse`) because runtime calls panic natively.
pub(crate) async fn run_pipeline_steps<Out, C, H, Fut, D>(
    url: Url,
    auth_header: Option<String>,
    config: AppConfig,
    cache: &C,
    endpoint: &EndpointConfig,
    handler: H,
) -> Result<Out::Out>
where
    Out: PipelineOut,
    C: CacheStore<Out::Out>,
    H: FnOnce(RequestContext) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(D, u64), AppError>>,
    D: serde::Serialize,
{
    // 2. Parse and validate query parameters
    let mut params = match parse_and_validate_query_params(&url) {
        Ok(p) => p,
        Err(e) => return Out::error(&e),
    };

    // 3. Apply endpoint-fixed parameters (username-only endpoints)
    if let Some(fixed) = endpoint.fixed_params {
        fixed.apply_to(&mut params);
    }

    // 4. Resolve authentication
    let auth = match resolve_auth(&config, &params.username, auth_header) {
        Ok(a) => a,
        Err(e) => return Out::error(&e),
    };

    // 5. Derive cache key (owned by the cache module) and check cache.
    //    The cache is an optimization only: a GET failure degrades to an
    //    uncached request rather than failing it.
    let key = cache_key(endpoint.name, &auth.username, &params);
    let hit = match cache.get(&key).await {
        Ok(Some(cached)) => Some(cached),
        Ok(None) => None,
        Err(e) => {
            log_warn!("cache GET failed for {} ({}); treating as miss, proceeding uncached", key, e);
            None
        }
    };
    if let Some(mut cached) = hit {
        Out::stamp(&mut cached, "X-Cache", "HIT")?;
        return Ok(cached);
    }

    // 6. Execute domain handler
    let rules = config.rules_for(&auth.username);
    let rctx = RequestContext { auth, params, rules, timeout_secs: config.request_timeout_secs };
    let (data, query_cost) = match handler(rctx).await {
        Ok(result) => result,
        Err(e) => return Out::error(&e),
    };

    // 7. Build response
    let mut resp = Out::success(&data, query_cost, endpoint.cache.ttl_secs)?;

    // 8. Store in cache — a PUT failure is logged and the caller still gets
    //    the (uncached) response. No error path here originates from the
    //    cache store.
    let mut cache_resp = Out::clone_copy(&mut resp)?;
    Out::stamp(&mut cache_resp, "X-Cache", "HIT")?;
    if let Err(e) = cache.put(&key, cache_resp).await {
        log_warn!("cache PUT failed for {} ({}); serving response uncached", key, e);
    }

    Out::stamp(&mut resp, "X-Cache", "MISS")?;

    Ok(resp)
}

/// Whitelist membership decides whether the system token may be used.
/// Pure: the Authorization header value is passed in rather than read from
/// the request, so tests can exercise it without the worker runtime.
pub fn resolve_auth(
    config: &AppConfig,
    username: &str,
    auth_header: Option<String>,
) -> std::result::Result<ResolvedAuth, AppError> {
    let is_whitelisted = config.whitelist.contains(&username.to_lowercase());

    let token = if is_whitelisted {
        config.github_token.clone()
    } else {
        let auth_header = auth_header
            .ok_or_else(|| AppError::Unauthorized(crate::error::Unauthorized::TokenRequired))?;

        let token = if auth_header.starts_with("Bearer ") {
            &auth_header[7..]
        } else {
            &auth_header
        };

        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::Unauthorized(crate::error::Unauthorized::Invalid("Invalid GitHub Personal Access Token".to_string())));
        }
        token.to_string()
    };

    Ok(ResolvedAuth { username: username.to_string(), token })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_with(query: &str) -> Url {
        Url::parse(&format!("https://api.example.com/v2/commits/latest?{}", query)).unwrap()
    }

    #[test]
    fn parse_valid_params() {
        let params = parse_and_validate_query_params(&url_with("username=test-user&limit=20&history_limit=10&language_limit=5")).unwrap();
        assert_eq!(params.username, "test-user");
        assert_eq!(params.limit, 20);
        assert_eq!(params.history_limit, 10);
        assert_eq!(params.language_limit, 5);
    }

    #[test]
    fn parse_defaults_match_documented_defaults() {
        let params = parse_and_validate_query_params(&url_with("username=test")).unwrap();
        assert_eq!(params.limit, 10);
        assert_eq!(params.history_limit, 10);
        assert_eq!(params.language_limit, 3);
    }

    #[test]
    fn parse_language_limit_out_of_bounds_rejected() {
        assert!(parse_and_validate_query_params(&url_with("username=test&language_limit=0")).is_err());
        assert!(parse_and_validate_query_params(&url_with("username=test&language_limit=11")).is_err());
        let ok = parse_and_validate_query_params(&url_with("username=test&language_limit=10")).unwrap();
        assert_eq!(ok.language_limit, 10);
    }

    #[test]
    fn parse_language_limit_non_numeric_rejected() {
        assert!(parse_and_validate_query_params(&url_with("username=test&language_limit=abc")).is_err());
    }

    #[test]
    fn parse_invalid_username_rejected() {
        assert!(parse_and_validate_query_params(&url_with("username=test_user")).is_err());
    }

    #[test]
    fn parse_missing_username_rejected() {
        assert!(parse_and_validate_query_params(&url_with("limit=5")).is_err());
    }

    #[test]
    fn parse_out_of_bounds_rejected() {
        assert!(parse_and_validate_query_params(&url_with("username=test&limit=100")).is_err());
        assert!(parse_and_validate_query_params(&url_with("username=test&history_limit=100")).is_err());
        assert!(parse_and_validate_query_params(&url_with("username=test&limit=0")).is_err());
    }

    /// A cache double that is always a miss and stores silently — for tests
    /// that exercise the pipeline choreography without cache noise.
    struct MissCache;

    impl CacheStore<crate::TestResponse> for MissCache {
        async fn get(&self, _key: &str) -> Result<Option<crate::TestResponse>> {
            Ok(None)
        }

        async fn put(&self, _key: &str, _response: crate::TestResponse) -> Result<()> {
            Ok(())
        }
    }

    /// Drive the real pipeline (`run_pipeline_steps` + `TestOut`) with a
    /// handler that echoes the resolved query params back in the body, so the
    /// test observes the params the handler actually received.
    async fn run_echo_pipeline(endpoint: EndpointConfig, query: &str) -> crate::TestResponse {
        let cache = MissCache;
        let cfg = AppConfig {
            github_token: "system-token".into(),
            whitelist: vec!["penqguin".into()],
            excluded_repos: vec![],
            request_timeout_secs: 15,
        };
        run_pipeline_steps::<TestOut, _, _, _, _>(
            url_with(query),
            None,
            cfg,
            &cache,
            &endpoint,
            |rctx| async move {
                Ok((serde_json::json!({
                    "limit": rctx.params.limit,
                    "history_limit": rctx.params.history_limit,
                    "language_limit": rctx.params.language_limit,
                }), 1u64))
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn fixed_params_override_parsed_values() {
        let out = run_echo_pipeline(
            EndpointConfig {
                name: "/commits/latest",
                cache: CachePolicy { ttl_secs: 300 },
                fixed_params: Some(FixedParams { limit: 5, history_limit: 5, language_limit: 3 }),
            },
            // Deliberately different from the fixed values: parsed values must lose.
            "username=penqguin&limit=20&history_limit=30&language_limit=8",
        )
        .await;

        out.assert_status(200);
        let body = out.body_json().unwrap();
        assert_eq!(body["limit"].as_u64(), Some(5));
        assert_eq!(body["history_limit"].as_u64(), Some(5));
        assert_eq!(body["language_limit"].as_u64(), Some(3));
    }

    #[tokio::test]
    async fn no_fixed_params_keeps_parsed_values() {
        let out = run_echo_pipeline(
            EndpointConfig {
                name: "/v2/commits/latest",
                cache: CachePolicy { ttl_secs: 300 },
                fixed_params: None,
            },
            "username=penqguin&limit=15&history_limit=25&language_limit=7",
        )
        .await;

        out.assert_status(200);
        let body = out.body_json().unwrap();
        assert_eq!(body["limit"].as_u64(), Some(15));
        assert_eq!(body["history_limit"].as_u64(), Some(25));
        assert_eq!(body["language_limit"].as_u64(), Some(7));
    }

    fn config() -> AppConfig {
        AppConfig {
            github_token: "system-token".into(),
            whitelist: vec!["penqguin".into()],
            excluded_repos: vec!["penqguin/penqguin".into(), "penqguin/ONotes".into()],
            request_timeout_secs: 15,
        }
    }

    #[test]
    fn resolve_auth_whitelisted_uses_system_token() {
        let auth = resolve_auth(&config(), "penqguin", None).unwrap();
        assert_eq!(auth.token, "system-token");
        assert_eq!(auth.username, "penqguin");
    }

    #[test]
    fn resolve_auth_non_whitelisted_requires_token() {
        let err = resolve_auth(&config(), "stranger", None).unwrap_err();
        match err {
            AppError::Unauthorized(crate::error::Unauthorized::TokenRequired) => {}
            _ => panic!("expected TokenRequired, got {:?}", err),
        }
    }

    #[test]
    fn resolve_auth_bearer_prefix_stripped() {
        let auth = resolve_auth(&config(), "stranger", Some("Bearer ghp_123".into())).unwrap();
        assert_eq!(auth.token, "ghp_123");
    }

    #[test]
    fn resolve_auth_bare_header_accepted() {
        let auth = resolve_auth(&config(), "stranger", Some("ghp_123".into())).unwrap();
        assert_eq!(auth.token, "ghp_123");
    }

    #[test]
    fn resolve_auth_blank_token_rejected() {
        let err = resolve_auth(&config(), "stranger", Some("   ".into())).unwrap_err();
        match err {
            AppError::Unauthorized(crate::error::Unauthorized::Invalid(_)) => {}
            _ => panic!("expected Invalid, got {:?}", err),
        }
    }

    #[test]
    fn rules_for_owner_carries_exclusions() {
        assert_eq!(config().rules_for("penqguin").excluded_repos.len(), 2);
        assert!(config().rules_for("stranger").excluded_repos.is_empty());
    }

    // --- REQUEST_TIMEOUT_SECS clamping (spec §5) ---

    #[test]
    fn timeout_unset_uses_default() {
        assert_eq!(parse_request_timeout_secs(None), 15);
    }

    #[test]
    fn timeout_unparseable_uses_default() {
        assert_eq!(parse_request_timeout_secs(Some("abc")), 15);
        assert_eq!(parse_request_timeout_secs(Some("")), 15);
    }

    #[test]
    fn timeout_zero_or_negative_uses_default() {
        assert_eq!(parse_request_timeout_secs(Some("0")), 15);
        assert_eq!(parse_request_timeout_secs(Some("-2")), 15);
    }

    #[test]
    fn timeout_above_max_clamped_to_thirty() {
        assert_eq!(parse_request_timeout_secs(Some("45")), 30);
        assert_eq!(parse_request_timeout_secs(Some("300")), 30);
    }

    #[test]
    fn timeout_in_bounds_kept() {
        assert_eq!(parse_request_timeout_secs(Some("1")), 1);
        assert_eq!(parse_request_timeout_secs(Some("20")), 20);
        assert_eq!(parse_request_timeout_secs(Some("30")), 30);
    }

    // --- Cache failure degradation (spec §1) ---

    /// Test materialization: produces `TestResponse`s without a worker
    /// runtime. Mirrors `ResponseOut` only where tests observe it (status,
    /// headers, JSON body).
    struct TestOut;

    impl PipelineOut for TestOut {
        type Out = crate::TestResponse;

        fn success<D: serde::Serialize>(data: &D, query_cost: u64, ttl_secs: u64) -> Result<crate::TestResponse> {
            Ok(ResponseBuilder::new()
                .with_header("X-Query-Cost", query_cost.to_string())
                .with_header("Cache-Control", format!("s-maxage={}", ttl_secs))
                .into_test_response(data))
        }

        fn error(err: &AppError) -> Result<crate::TestResponse> {
            let (status, body) = err.to_error_parts();
            let mut builder = ResponseBuilder::new()
                .with_status(status)
                .with_header("X-Error-Code", err.error_code());
            for (key, value) in err.extra_headers() {
                builder = builder.with_header(key, value);
            }
            Ok(builder.into_test_response(&body))
        }

        fn stamp(out: &mut crate::TestResponse, key: &str, value: &str) -> Result<()> {
            if let Some(slot) = out.headers.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
                slot.1 = value.to_string();
            } else {
                out.headers.push((key.to_string(), value.to_string()));
            }
            Ok(())
        }

        fn clone_copy(out: &mut crate::TestResponse) -> Result<crate::TestResponse> {
            Ok(crate::TestResponse {
                status: out.status,
                headers: out.headers.clone(),
                body: out.body.clone(),
            })
        }
    }

    /// Test double whose get/put both fail, standing in for a cache outage.
    /// Counts put attempts so tests can assert the pipeline still tried to
    /// warm the cache before treating the failure as harmless.
    struct FailingCache {
        puts: std::sync::atomic::AtomicU32,
    }

    impl CacheStore<crate::TestResponse> for FailingCache {
        async fn get(&self, _key: &str) -> Result<Option<crate::TestResponse>> {
            Err(Error::JsError("simulated cache outage".into()))
        }

        async fn put(&self, _key: &str, _response: crate::TestResponse) -> Result<()> {
            self.puts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(Error::JsError("simulated cache outage".into()))
        }
    }

    #[tokio::test]
    async fn failing_cache_request_still_succeeds_with_miss() {
        let cache = FailingCache { puts: Default::default() };
        let endpoint = EndpointConfig {
            name: "/v2/commits/latest",
            cache: CachePolicy { ttl_secs: 300 },
            fixed_params: None,
        };
        let cfg = AppConfig {
            github_token: "system-token".into(),
            whitelist: vec!["penqguin".into()],
            excluded_repos: vec![],
            request_timeout_secs: 15,
        };

        let out = run_pipeline_steps::<TestOut, _, _, _, _>(
            url_with("username=penqguin&limit=20&history_limit=10"),
            None,
            cfg,
            &cache,
            &endpoint,
            |rctx| async move {
                let data = serde_json::json!({ "username": rctx.auth.username });
                Ok((data, 5u64))
            },
        )
        .await
        .unwrap();

        // The request succeeds end to end and reports itself as a MISS.
        out.assert_status(200);
        out.assert_header("X-Cache", "MISS");
        out.assert_header("X-Query-Cost", "5");
        out.assert_header("Cache-Control", "s-maxage=300");
        assert_eq!(out.body_json().unwrap()["username"], "penqguin");

        // The pipeline still attempted the store before degrading.
        assert_eq!(
            cache.puts.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "cache put should still be attempted"
        );
    }
}
