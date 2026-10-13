use worker::*;

mod cache;
mod error;
mod github;
mod models;
mod pipeline;
mod shaping;

use crate::cache::{cache_key, CacheStore, WorkersCache};
use crate::error::AppError;
use crate::models::QueryParams;
use crate::pipeline::{get_config, run_pipeline, CachePolicy, EndpointConfig};

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
        for (key, value) in err.extra_headers() {
            headers.set(key, &value)?;
        }
        Ok(resp)
    }
}

// --- Scheduled cache warming ---

/// Warm the default v2 query for every whitelisted profile. Keys are derived
/// by the cache module, so warmed entries are readable by real requests.
#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let config = match get_config(&env) {
        Ok(c) => c,
        Err(_) => return,
    };

    let cache = WorkersCache;
    let futs: Vec<_> = config.whitelist.iter().map(|username| {
        let token = config.github_token.clone();
        let excluded = config.rules_for(username).excluded_repos;
        let params = QueryParams {
            username: username.clone(),
            limit: 10,
            history_limit: 10,
        };
        let key = cache_key("/v2/commits/latest", username, &params);
        async move {
            if let Ok(Some(_)) = cache.get(&key).await {
                return;
            }

            let fetcher = github::RealHttpFetch;
            if let Ok((commits, cost)) = github::get_commits_list(
                &fetcher, username, &token, params.limit, params.history_limit, &excluded,
                config.request_timeout_secs,
            ).await {
                if let Ok(mut resp) = ResponseBuilder::new()
                    .with_header("Cache-Control", "s-maxage=300")
                    .with_header("X-Query-Cost", &cost.to_string())
                    .json(&commits)
                {
                    // Stamp as a HIT so the first real request reports a cache hit.
                    let _ = resp.headers_mut().set("X-Cache", "HIT");
                    let _ = cache.put(&key, resp).await;
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
        // v1 commits — username-only adapter with fixed limits, routes through get_commits_list
        .get_async("/commits/latest", |req, ctx| async move {
            let endpoint = EndpointConfig {
                name: "/commits/latest",
                cache: CachePolicy { ttl_secs: 300 },
                fixed_params: Some((5, 5)),
            };
            run_pipeline(&req, &ctx.env, &WorkersCache, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                github::get_commits_list(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                    rctx.params.limit,
                    rctx.params.history_limit,
                    &rctx.rules.excluded_repos,
                    rctx.timeout_secs,
                ).await
            }).await
        })
        // v2 commits — full param support
        .get_async("/v2/commits/latest", |req, ctx| async move {
            let endpoint = EndpointConfig {
                name: "/v2/commits/latest",
                cache: CachePolicy { ttl_secs: 300 },
                fixed_params: None,
            };
            run_pipeline(&req, &ctx.env, &WorkersCache, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                github::get_commits_list(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                    rctx.params.limit,
                    rctx.params.history_limit,
                    &rctx.rules.excluded_repos,
                    rctx.timeout_secs,
                ).await
            }).await
        })
        // Streak
        .get_async("/streak", |req, ctx| async move {
            let endpoint = EndpointConfig {
                name: "/streak",
                cache: CachePolicy { ttl_secs: 300 },
                fixed_params: None,
            };
            run_pipeline(&req, &ctx.env, &WorkersCache, &endpoint, |rctx| async move {
                let fetcher = github::RealHttpFetch;
                let (data, cost) = github::get_streak_info(
                    &fetcher,
                    &rctx.auth.username,
                    &rctx.auth.token,
                    rctx.timeout_secs,
                ).await?;
                Ok((data, cost))
            }).await
        })
        .run(req, env)
        .await;

    res
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
