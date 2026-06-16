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
    request_timeout_secs: u64,
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

    let request_timeout_secs = env.var("REQUEST_TIMEOUT_SECS")
        .map(|v| v.to_string().parse::<u64>().unwrap_or(15))
        .unwrap_or(15)
        .min(30);
    
    Ok(AppConfig { github_token, whitelist, request_timeout_secs })
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
    
    if limit == 0 || limit > 50 {
        return Err(AppError::ValidationError("limit".to_string(), "must be between 1 and 50".to_string()));
    }

    let history_limit = query.get("history_limit")
        .map(|v| v.parse::<usize>().map_err(|_| AppError::ValidationError("history_limit".to_string(), "must be a number".to_string())))
        .transpose()?
        .unwrap_or(5);
    
    if history_limit == 0 || history_limit > 20 {
        return Err(AppError::ValidationError("history_limit".to_string(), "must be between 1 and 20".to_string()));
    }

    let language_limit = query.get("language_limit")
        .map(|v| v.parse::<usize>().map_err(|_| AppError::ValidationError("language_limit".to_string(), "must be a number".to_string())))
        .transpose()?
        .unwrap_or(3);
    
    if language_limit == 0 || language_limit > 10 {
        return Err(AppError::ValidationError("language_limit".to_string(), "must be between 1 and 10".to_string()));
    }

    Ok(QueryParams { username, limit, history_limit, language_limit })
}

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let config = match get_config(&env) {
        Ok(c) => c,
        Err(_) => return,
    };

    for username in config.whitelist {
        // We warm the most common endpoint (v2) with default limits
        let url = format!("https://iceberg.penqguin.com/v2/commits/latest?username={}", username);
        
        // Check if it's already in cache to avoid wasting GitHub API quota
        if let Ok(Some(_)) = CACHE.get(&url, true).await {
            continue;
        }

        if let Ok((commits, cost)) = github::get_commits_list(&username, &config.github_token, 10, 5).await {
            if let Ok(mut resp) = Response::from_json(&commits) {
                let headers = resp.headers_mut();
                let _ = headers.set("Cache-Control", "s-maxage=300");
                let _ = headers.set("Access-Control-Allow-Origin", "*");
                let _ = headers.set("Access-Control-Allow-Methods", "GET, OPTIONS");
                let _ = headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type");
                let _ = headers.set("Access-Control-Max-Age", "86400");
                let _ = headers.set("X-Cache", "HIT");
                let _ = headers.set("X-Query-Cost", &cost.to_string());
                let _ = CACHE.put(url, resp).await;
            }
        }
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Better panic reports in the console
    console_error_panic_hook::set_once();

    let res = Router::new()
        .options("/*path", |_, _| {
            let mut resp = Response::empty()?;
            let headers = resp.headers_mut();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(resp)
        })
        .get("/", |_, _| {
            let mut resp = Response::from_html(include_str!("../static/docs.html"))?;
            let headers = resp.headers_mut();
            headers.set("Access-Control-Allow-Origin", "*")?;
            Ok(resp)
        })
        .get("/healthcheck", |_, _| {
            let mut resp = Response::from_json(&serde_json::json!({ "status": "ok" }))?;
            let headers = resp.headers_mut();
            headers.set("Access-Control-Allow-Origin", "*")?;
            Ok(resp)
        })
        .get_async("/commits/latest", |req, ctx| async move {
            let url = req.url()?;
            let config = match get_config(&ctx.env) {
                Ok(c) => c,
                Err(e) => return e.to_response(),
            };
            
            let mut params = match parse_and_validate_query_params(&url) {
                Ok(p) => p,
                Err(e) => return e.to_response(),
            };
            
            // Hardcode limits for v1 backward compatibility
            params.limit = 5;
            params.history_limit = 5;

            let is_whitelisted = config.whitelist.contains(&params.username.to_lowercase());
            let cache_key = format!("{}:wl={}", url, is_whitelisted);

            if let Some(mut resp) = CACHE.get(cache_key.clone(), true).await? {
                console_log!("Cache HIT: {}", cache_key);
                resp.headers_mut().set("X-Cache", "HIT")?;
                return Ok(resp);
            }
            console_log!("Cache MISS: {}", cache_key);

            let auth = match resolve_auth(&req, &config, &params.username).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            match github::get_most_recent_commit(&auth.username, &auth.token).await {
                Ok((commit, cost)) => {
                    let mut resp = Response::from_json(&commit)?;
                    {
                        let headers = resp.headers_mut();
                        headers.set("Cache-Control", "s-maxage=300")?;
                        headers.set("Access-Control-Allow-Origin", "*")?;
                        headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
                        headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
                        headers.set("Access-Control-Max-Age", "86400")?;
                        headers.set("X-Query-Cost", &cost.to_string())?;
                    }
                    
                    let mut cache_resp = resp.cloned()?;
                    cache_resp.headers_mut().set("X-Cache", "HIT")?;
                    CACHE.put(cache_key, cache_resp).await?;
                    
                    resp.headers_mut().set("X-Cache", "MISS")?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .get_async("/v2/commits/latest", |req, ctx| async move {
            let url = req.url()?;
            let config = match get_config(&ctx.env) {
                Ok(c) => c,
                Err(e) => return e.to_response(),
            };

            let params = match parse_and_validate_query_params(&url) {
                Ok(p) => p,
                Err(e) => return e.to_response(),
            };

            let is_whitelisted = config.whitelist.contains(&params.username.to_lowercase());
            let cache_key = format!("{}:wl={}", url, is_whitelisted);

            if let Some(mut resp) = CACHE.get(cache_key.clone(), true).await? {
                console_log!("Cache HIT: {}", cache_key);
                resp.headers_mut().set("X-Cache", "HIT")?;
                return Ok(resp);
            }
            console_log!("Cache MISS: {}", cache_key);

            let auth = match resolve_auth(&req, &config, &params.username).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            match github::get_commits_list(&auth.username, &auth.token, params.limit, params.history_limit).await {
                Ok((commits, cost)) => {
                    let mut resp = Response::from_json(&commits)?;
                    {
                        let headers = resp.headers_mut();
                        headers.set("Cache-Control", "s-maxage=300")?;
                        headers.set("Access-Control-Allow-Origin", "*")?;
                        headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
                        headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
                        headers.set("Access-Control-Max-Age", "86400")?;
                        headers.set("X-Query-Cost", &cost.to_string())?;
                    }

                    let mut cache_resp = resp.cloned()?;
                    cache_resp.headers_mut().set("X-Cache", "HIT")?;
                    CACHE.put(cache_key, cache_resp).await?;
                    
                    resp.headers_mut().set("X-Cache", "MISS")?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .get_async("/streak", |req, ctx| async move {
            let url = req.url()?;
            let config = match get_config(&ctx.env) {
                Ok(c) => c,
                Err(e) => return e.to_response(),
            };

            let params = match parse_and_validate_query_params(&url) {
                Ok(p) => p,
                Err(e) => return e.to_response(),
            };

            let is_whitelisted = config.whitelist.contains(&params.username.to_lowercase());
            let cache_key = format!("{}:wl={}", url, is_whitelisted);

            if let Some(mut resp) = CACHE.get(cache_key.clone(), true).await? {
                console_log!("Cache HIT: {}", cache_key);
                resp.headers_mut().set("X-Cache", "HIT")?;
                return Ok(resp);
            }
            console_log!("Cache MISS: {}", cache_key);

            let auth = match resolve_auth(&req, &config, &params.username).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            match github::get_streak_info(&auth.username, &auth.token).await {
                Ok((streak, cost)) => {
                    let mut resp = Response::from_json(&streak)?;
                    {
                        let headers = resp.headers_mut();
                        headers.set("Cache-Control", "s-maxage=300")?;
                        headers.set("Access-Control-Allow-Origin", "*")?;
                        headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
                        headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
                        headers.set("Access-Control-Max-Age", "86400")?;
                        headers.set("X-Query-Cost", &cost.to_string())?;
                    }

                    let mut cache_resp = resp.cloned()?;
                    cache_resp.headers_mut().set("X-Cache", "HIT")?;
                    CACHE.put(cache_key, cache_resp).await?;
                    
                    resp.headers_mut().set("X-Cache", "MISS")?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .run(req, env)
        .await;

    res
}

async fn resolve_auth(req: &Request, config: &AppConfig, username: &str) -> std::result::Result<ResolvedAuth, AppError> {
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

    #[test]
    fn test_parse_and_validate_query_params_valid() {
        let url = Url::parse("https://api.example.com/v2/commits/latest?username=test-user&limit=20&history_limit=10&language_limit=5").unwrap();
        let params = parse_and_validate_query_params(&url).unwrap();
        assert_eq!(params.username, "test-user");
        assert_eq!(params.limit, 20);
        assert_eq!(params.history_limit, 10);
        assert_eq!(params.language_limit, 5);
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
}

