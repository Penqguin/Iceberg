use worker::*;

mod error;
mod github;
mod models;

use crate::error::AppError;
use crate::models::ResolvedAuth;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Better panic reports in the console
    console_error_panic_hook::set_once();

    let origin = req.headers().get("Origin")?.unwrap_or_default();

    let mut resp = Router::new()
        .options("/*path", |_, _| {
            Response::empty()
        })
        .get("/", |_, _| {
            Response::from_html(include_str!("../static/docs.html"))
        })
        .get("/healthcheck", |_, _| {
            Response::from_json(&serde_json::json!({ "status": "ok" }))
        })
        .get_async("/commits/latest", |req, ctx| async move {
            let cache = Cache::default();
            let url = req.url()?;
            if let Some(mut resp) = cache.get(url.to_string(), true).await? {
                return Ok(resp.cloned()?);
            }

            let auth = match resolve_auth(&req, &ctx.env).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            match github::get_most_recent_commit(&auth.username, &auth.token).await {
                Ok(commit) => {
                    let mut resp = Response::from_json(&commit)?;
                    resp.headers_mut().set("Cache-Control", "s-maxage=60")?;
                    cache.put(url.to_string(), resp.cloned()?).await?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .get_async("/v2/commits/latest", |req, ctx| async move {
            let cache = Cache::default();
            let url = req.url()?;
            if let Some(mut resp) = cache.get(url.to_string(), true).await? {
                return Ok(resp.cloned()?);
            }

            let auth = match resolve_auth(&req, &ctx.env).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            let limit = url.query_pairs()
                .find(|(k, _)| k == "limit")
                .map(|(_, v)| v.into_owned())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10);
            
            let limit = if limit == 0 { 10 } else { limit };

            match github::get_commits_list(&auth.username, &auth.token, limit).await {
                Ok(commits) => {
                    let mut resp = Response::from_json(&commits)?;
                    resp.headers_mut().set("Cache-Control", "s-maxage=60")?;
                    cache.put(url.to_string(), resp.cloned()?).await?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .get_async("/streak", |req, ctx| async move {
            let cache = Cache::default();
            let url = req.url()?;
            if let Some(mut resp) = cache.get(url.to_string(), true).await? {
                return Ok(resp.cloned()?);
            }

            let auth = match resolve_auth(&req, &ctx.env).await {
                Ok(a) => a,
                Err(e) => return e.to_response(),
            };

            match github::get_streak_info(&auth.username, &auth.token).await {
                Ok(streak) => {
                    let mut resp = Response::from_json(&streak)?;
                    resp.headers_mut().set("Cache-Control", "s-maxage=60")?;
                    cache.put(url.to_string(), resp.cloned()?).await?;
                    Ok(resp)
                }
                Err(e) => e.to_response(),
            }
        })
        .run(req, env)
        .await?;

    let headers = resp.headers_mut();
    let cors_origin = if origin.contains("penqguin.com") {
        origin
    } else {
        "https://penqguin.com".to_string()
    };

    headers.set("Access-Control-Allow-Origin", &cors_origin)?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
    headers.set("Access-Control-Max-Age", "86400")?;

    Ok(resp)
}

async fn resolve_auth(req: &Request, env: &Env) -> std::result::Result<ResolvedAuth, AppError> {
    let url = req.url()?;
    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();

    let username = query.get("username")
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| AppError::BadRequest("username parameter is required".to_string()))?;

    // Whitelist logic
    let whitelist_str = env.var("WHITELIST")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "penqguin".to_string());
    
    let whitelist: Vec<String> = whitelist_str
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let is_whitelisted = whitelist.contains(&username.to_lowercase());

    let token = if is_whitelisted {
        env.var("GITHUB_TOKEN")
            .map(|v| v.to_string())
            .map_err(|_| AppError::Internal("System GITHUB_TOKEN is not configured".to_string()))?
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

    Ok(ResolvedAuth { username, token })
}
