use crate::cache::{cache_key, CacheStore};
use crate::error::AppError;
use crate::models::{QueryParams, ResolvedAuth};
use crate::shaping::ShapingRules;
use crate::ResponseBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use worker::*;

static USERNAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap());

const OWNER_EXCLUDED_REPOS_DEFAULT: &str = "penqguin/penqguin,penqguin/ONotes";

pub struct AppConfig {
    pub github_token: String,
    pub whitelist: Vec<String>,
    /// Repos hidden from the profile of any whitelisted (owner) user.
    pub excluded_repos: Vec<String>,
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

    Ok(AppConfig { github_token, whitelist, excluded_repos })
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
            crate::log_warn!("Rejected query param: field=username reason={} (username length {} — value withheld)", reason, value.len());
        }
        _ => {
            crate::log_warn!("Rejected query param: field={} reason={}", field, reason);
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

pub struct CachePolicy {
    pub ttl_secs: u64,
}

/// Endpoint configuration. `fixed_params` is for username-only endpoints
/// (v1) whose limits don't respond to query parameters; `None` means full
/// parameter support (v2). Tuple order: `(limit, history_limit, language_limit)`.
pub struct EndpointConfig {
    /// Cache-key namespace, e.g. "/v2/commits/latest".
    pub name: &'static str,
    pub cache: CachePolicy,
    pub fixed_params: Option<(usize, usize, usize)>,
}

pub struct RequestContext {
    pub auth: ResolvedAuth,
    pub params: QueryParams,
    pub rules: ShapingRules,
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
    C: CacheStore,
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

    // 3. Apply endpoint-fixed parameters (username-only endpoints)
    if let Some((limit, history_limit, language_limit)) = endpoint.fixed_params {
        params.limit = limit;
        params.history_limit = history_limit;
        params.language_limit = language_limit;
    }

    // 4. Resolve authentication
    let auth = match resolve_auth(&config, &params.username, req.headers().get("Authorization")?) {
        Ok(a) => a,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 5. Derive cache key (owned by the cache module) and check cache
    let key = cache_key(endpoint.name, &auth.username, &params);
    if let Some(mut cached) = cache.get(&key).await? {
        let mut resp = cached.cloned()?;
        resp.headers_mut().set("X-Cache", "HIT")?;
        return Ok(resp);
    }

    // 6. Execute domain handler
    let rules = config.rules_for(&auth.username);
    let rctx = RequestContext { auth, params, rules };
    let (data, query_cost) = match handler(rctx).await {
        Ok(result) => result,
        Err(e) => return ResponseBuilder::from_error(&e),
    };

    // 7. Build response
    let mut builder = ResponseBuilder::new()
        .with_header("X-Query-Cost", query_cost.to_string());

    builder = builder.with_header("Cache-Control", format!("s-maxage={}", endpoint.cache.ttl_secs));

    let mut resp = builder.json(&data)?;

    // 8. Store in cache
    let mut cache_resp = resp.cloned()?;
    cache_resp.headers_mut().set("X-Cache", "HIT")?;
    cache.put(&key, cache_resp).await?;
    resp.headers_mut().set("X-Cache", "MISS")?;

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

    #[test]
    fn fixed_params_override_parsed_values() {
        let mut params = parse_and_validate_query_params(&url_with("username=test&limit=15")).unwrap();
        let endpoint = EndpointConfig {
            name: "/commits/latest",
            cache: CachePolicy { ttl_secs: 300 },
            fixed_params: Some((5, 5, 3)),
        };
        if let Some((limit, history_limit, language_limit)) = endpoint.fixed_params {
            params.limit = limit;
            params.history_limit = history_limit;
            params.language_limit = language_limit;
        }
        assert_eq!(params.limit, 5);
        assert_eq!(params.history_limit, 5);
        assert_eq!(params.language_limit, 3);
    }

    #[test]
    fn no_fixed_params_keeps_parsed_values() {
        let params = parse_and_validate_query_params(&url_with("username=test&limit=15")).unwrap();
        let endpoint = EndpointConfig {
            name: "/v2/commits/latest",
            cache: CachePolicy { ttl_secs: 300 },
            fixed_params: None,
        };
        assert!(endpoint.fixed_params.is_none());
        assert_eq!(params.limit, 15);
    }

    fn config() -> AppConfig {
        AppConfig {
            github_token: "system-token".into(),
            whitelist: vec!["penqguin".into()],
            excluded_repos: vec!["penqguin/penqguin".into(), "penqguin/ONotes".into()],
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
}
