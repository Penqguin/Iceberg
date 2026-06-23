use worker::*;
use once_cell::sync::Lazy;
use regex::Regex;

mod error;
mod github;
mod models;

use crate::error::AppError;
use crate::models::{ResolvedAuth, QueryParams};

static CACHE: Lazy<Cache> = Lazy::new(|| Cache::default());
static USERNAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap());

struct AppConfig {
    github_token: String,
    whitelist: Vec<String>,
}

fn get_config(env: &Env) -> std::result::Result<AppConfig, AppError> {
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

    Ok(AppConfig { github_token, whitelist })
}

fn parse_and_validate_query_params(url: &Url) -> std::result::Result<QueryParams, AppError> {
    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    
    let username = query.get("username")
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| AppError::ValidationError("username".to_string(), "is required".to_string()))?;

    if !USERNAME_REGEX.is_match(&username) {
        return Err(AppError::ValidationError("username".to_string(), "contains invalid characters".to_string()));
    }

    let limit = query.get("limit")
        .map(|v| v.parse::<usize>().map_err(|_| AppError::ValidationError("limit".to_string(), "must be a number".to_string())))
        .transpose()?
        .unwrap_or(10);
    
    if limit == 0 || limit > 20 {
        return Err(AppError::ValidationError("limit".to_string(), "must be between 1 and 50".to_string()));
    }

    let history_limit = query.get("history_limit")
        .map(|v| v.parse::<usize>().map_err(|_| AppError::ValidationError("history_limit".to_string(), "must be a number".to_string())))
        .transpose()?
        .unwrap_or(5);
    
    if history_limit == 0 || history_limit > 75 {
        return Err(AppError::ValidationError("history_limit".to_string(), "must be between 1 and 20".to_string()));
    }

    Ok(QueryParams { username, limit, history_limit })
}

// --- Response / CORS seam ---

pub struct ResponseBuilder {
    status: u16,
    extra_headers: Vec<(String, String)>,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        Self {
            status: 200,
            extra_headers: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn with_header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }

    pub fn set_cors_headers(headers: &mut Headers) -> Result<()> {
        headers.set("Access-Control-Allow-Origin", "*")?;
        headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
        headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
        headers.set("Access-Control-Max-Age", "86400")?;
        headers.set("X-Request-ID", &format!("req-{}", worker::js_sys::Math::random()))?;
        Ok(())
    }

    /// Build a success JSON response with global + per-response headers.
    pub fn json<D: serde::Serialize>(self, data: &D) -> Result<Response> {
        let mut resp = Response::from_json(data)?.with_status(self.status);
        let headers = resp.headers_mut();
        Self::set_cors_headers(headers)?;
        for (key, value) in &self.extra_headers {
            headers.set(key, value)?;
        }
        Ok(resp)
    }

    /// Build an error response from an AppError.
    pub fn from_error(err: &AppError) -> Result<Response> {
        let (status, body) = err.to_error_parts();
        let mut resp = Response::from_json(&body)?.with_status(status);
        let headers = resp.headers_mut();
        Self::set_cors_headers(headers)?;
        headers.set("X-Error-Code", err.error_code())?;
        if let AppError::RateLimited(retry_after) = err {
            headers.set("X-Retry-After", &retry_after.to_string())?;
        }
        Ok(resp)
    }
}

// --- Pipeline types ---

pub struct CachePolicy {
    pub ttl_secs: u64,
}

pub struct EndpointConfig {
    pub cache: CachePolicy,
    pub param_override: Option<fn(&mut QueryParams)>,
}

pub struct RequestContext {
    pub auth: ResolvedAuth,
    pub params: QueryParams,
}

/// Core pipeline: config → params → cache-check → auth → handler → response → cache-store.
///
/// Header ownership:
///   - Global/Structural (Access-Control-*, X-Request-ID) — ResponseBuilder
///   - Lifecycle/State (X-Cache, Cache-Control) — this function
///   - Domain/Business (X-Query-Cost) — passed from handler via with_header
async fn run_pipeline<H, Fut, D>(
    req: &Request,
    env: &Env,
    endpoint: &EndpointConfig,
    handler: H,
) -> Result<Response>
where
    H: FnOnce(RequestContext) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(D, u64), AppError>>,
    D: serde::Serialize,
{
    let url = req.url()?;

    // 1. Load configuration
    let config = match get_config(env) {
        Ok(c) => c,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 2. Parse and validate query parameters
    let mut params = match parse_and_validate_query_params(&url) {
        Ok(p) => p,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 3. Apply endpoint-specific parameter overrides
    if let Some(override_fn) = endpoint.param_override {
        override_fn(&mut params);
    }

    // 4. Resolve authentication
    let auth = match resolve_auth(req, &config, &params.username).await {
        Ok(a) => a,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 5. Derive cache key from URL + auth identity
    let cache_key = format!("{}:user={}", url, auth.username);

    // 6. Check cache
    if let Some(mut cached) = CACHE.get(cache_key.clone(), true).await? {
        let mut resp = cached.cloned()?;
        resp.headers_mut().set("X-Cache", "HIT")?;
        return Ok(resp);
    }

    // 7. Execute domain handler
    let rctx = RequestContext { auth, params };
    let (data, query_cost) = match handler(rctx).await {
        Ok(result) => result,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 8. Build response
    let mut builder = ResponseBuilder::new()
        .with_header("X-Query-Cost", query_cost.to_string());

    builder = builder.with_header("Cache-Control", format!("s-maxage={}", endpoint.cache.ttl_secs));

    let mut resp = builder.json(&data)?;

    // 9. Store in cache
    let mut cache_resp = resp.cloned()?;
    cache_resp.headers_mut().set("X-Cache", "HIT")?;
    CACHE.put(cache_key, cache_resp).await?;
    resp.headers_mut().set("X-Cache", "MISS")?;

    Ok(resp)
}

// --- Test response seam ---

#[cfg(test)]
pub struct TestResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

#[cfg(test)]
impl TestResponse {
    pub fn assert_status(&self, expected: u16) {
        assert_eq!(self.status, expected, "expected status {}, got {}", expected, self.status);
    }

    pub fn header_value(&self, key: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn assert_header(&self, key: &str, expected: &str) {
        let actual = self.header_value(key);
        assert_eq!(actual, Some(expected), "expected header {} = {}", key, expected);
    }

    pub fn body_json(&self) -> Option<&serde_json::Value> {
        self.body.as_ref()
    }
}

#[cfg(test)]
impl ResponseBuilder {
    /// Produce a TestResponse instead of a worker Response, for unit tests
    /// that don't need the full worker runtime.
    pub fn into_test_response<D: serde::Serialize>(self, data: &D) -> TestResponse {
        let body = serde_json::to_value(data).ok();
        let mut headers = vec![
            ("Access-Control-Allow-Origin".into(), "*".into()),
            ("Access-Control-Allow-Methods".into(), "GET, OPTIONS".into()),
            ("Access-Control-Allow-Headers".into(), "Authorization, Content-Type".into()),
            ("Access-Control-Max-Age".into(), "86400".into()),
        ];
        headers.extend(self.extra_headers);
        TestResponse {
            status: self.status,
            headers,
            body,
        }
    }
}

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let config = match get_config(&env) {
        Ok(c) => c,
        Err(_) => return,
    };

    let futs: Vec<_> = config.whitelist.iter().map(|username| {
        let token = config.github_token.clone();
        let url = format!("https://iceberg.penqguin.com/v2/commits/latest?username={}", username);
        async move {
            if let Ok(Some(_)) = CACHE.get(&url, true).await {
                return;
            }

            let fetcher = github::RealHttpFetch;
            if let Ok((commits, cost)) = github::get_commits_list(&fetcher, username, &token, 10, 5).await {
                if let Ok(mut resp) = ResponseBuilder::new()
                    .with_header("Cache-Control", "s-maxage=300")
                    .with_header("X-Query-Cost", &cost.to_string())
                    .json(&commits)
                {
                    let _ = resp.headers_mut().set("X-Cache", "HIT");
                    let _ = CACHE.put(url, resp).await;
                }
            }
        }
    }).collect();

    futures::future::join_all(futs).await;
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let res = Router::new()
        // OPTIONS is universal — ResponseBuilder sets the same headers on all responses
        .options("/*path", |_, _| {
            let mut resp = Response::empty()?;
            ResponseBuilder::set_cors_headers(resp.headers_mut())?;
            Ok(resp)
        })
        .get("/", |_, _| {
            let mut resp = Response::from_html(include_str!("../static/docs.html"))?;
            ResponseBuilder::set_cors_headers(resp.headers_mut())?;
            Ok(resp)
        })
        .get("/healthcheck", |_, _| {
            let mut resp = Response::from_json(&serde_json::json!({ "status": "ok" }))?;
            ResponseBuilder::set_cors_headers(resp.headers_mut())?;
            Ok(resp)
        })
        // v1 commits — adapter with hardcoded limits, routes through get_commits_list
        .get_async("/commits/latest", |req, ctx| async move {
            let endpoint = EndpointConfig {
                cache: CachePolicy { ttl_secs: 300 },
                param_override: Some(|params: &mut QueryParams| {
                    params.limit = 5;
                    params.history_limit = 5;
                }),
            };
            run_pipeline(&req, &ctx.env, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                let (data, cost) = github::get_commits_list(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                    rctx.params.limit,
                    rctx.params.history_limit,
                ).await?;
                Ok((data, cost))
            }).await
        })
        // v2 commits — full param support
        .get_async("/v2/commits/latest", |req, ctx| async move {
            let endpoint = EndpointConfig {
                cache: CachePolicy { ttl_secs: 300 },
                param_override: None,
            };
            run_pipeline(&req, &ctx.env, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                let (data, cost) = github::get_commits_list(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                    rctx.params.limit,
                    rctx.params.history_limit,
                ).await?;
                Ok((data, cost))
            }).await
        })
        // Streak
        .get_async("/streak", |req, ctx| async move {
            let endpoint = EndpointConfig {
                cache: CachePolicy { ttl_secs: 300 },
                param_override: None,
            };
            run_pipeline(&req, &ctx.env, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                let (data, cost) = github::get_streak_info(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                ).await?;
                Ok((data, cost))
            }).await
        })
        .run(req, env)
        .await;

    res
}

async fn resolve_auth(
    req: &Request,
    config: &AppConfig,
    username: &str,
) -> std::result::Result<ResolvedAuth, AppError> {
    let is_whitelisted = config.whitelist.contains(&username.to_lowercase());

    let token = if is_whitelisted {
        config.github_token.clone()
    } else {
        let auth_header = req.headers().get("Authorization")?
            .ok_or_else(|| AppError::Unauthorized("GitHub Personal Access Token required for non-whitelisted users".to_string()))?;

        let token = if auth_header.starts_with("Bearer ") {
            &auth_header[7..]
        } else {
            &auth_header
        };

        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::Unauthorized("Invalid GitHub Personal Access Token".to_string()));
        }
        token.to_string()
    };

    Ok(ResolvedAuth { username: username.to_string(), token })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_and_validate_query_params_valid() {
        let url = Url::parse("https://api.example.com/v2/commits/latest?username=test-user&limit=20&history_limit=10").unwrap();
        let params = parse_and_validate_query_params(&url).unwrap();
        assert_eq!(params.username, "test-user");
        assert_eq!(params.limit, 20);
        assert_eq!(params.history_limit, 10);
    }

    #[test]
    fn test_parse_and_validate_query_params_invalid_username() {
        let url = Url::parse("https://api.example.com/v2/commits/latest?username=test_user").unwrap();
        let result = parse_and_validate_query_params(&url);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_and_validate_query_params_out_of_bounds() {
        let url = Url::parse("https://api.example.com/v2/commits/latest?username=test&limit=100").unwrap();
        let result = parse_and_validate_query_params(&url);
        assert!(result.is_err());
    }

    #[test]
    fn test_response_builder_success() {
        let data = json!({ "status": "ok" });
        let tr = ResponseBuilder::new()
            .with_status(200)
            .with_header("X-Custom", "value")
            .into_test_response(&data);

        tr.assert_status(200);
        tr.assert_header("Access-Control-Allow-Origin", "*");
        tr.assert_header("X-Custom", "value");
        assert_eq!(tr.body_json(), Some(&data));
    }

    #[test]
    fn test_response_builder_with_extra_headers() {
        let data = json!({ "key": 42 });
        let tr = ResponseBuilder::new()
            .with_header("X-Query-Cost", "5")
            .with_header("Cache-Control", "s-maxage=300")
            .into_test_response(&data);

        tr.assert_header("X-Query-Cost", "5");
        tr.assert_header("Cache-Control", "s-maxage=300");
        tr.assert_header("Access-Control-Allow-Origin", "*");
    }

    #[test]
    fn test_endpoint_config_param_override() {
        let mut params = QueryParams {
            username: "test".into(),
            limit: 10,
            history_limit: 10,
        };

        let endpoint = EndpointConfig {
            cache: CachePolicy { ttl_secs: 300 },
            param_override: Some(|p: &mut QueryParams| {
                p.limit = 5;
                p.history_limit = 5;
            }),
        };

        if let Some(override_fn) = endpoint.param_override {
            override_fn(&mut params);
        }

        assert_eq!(params.limit, 5);
        assert_eq!(params.history_limit, 5);
    }
}

