use crate::error::AppError;
use crate::github::types::*;
use futures::future::{select, Either};
use serde_json::json;
use std::collections::HashMap;
use worker::*;
#[cfg(test)]
use tokio::time as tokio_time;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

/// Log a warning — `console_warn!` on wasm32, `eprintln!` on native.
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "wasm32")]
        console_warn!($($arg)*);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($arg)*);
    }};
}

/// Sleep — `tokio::time::sleep` in tests, `worker::Delay` in production (wasm32).
#[cfg(test)]
async fn sleep(dur: std::time::Duration) {
    tokio_time::sleep(dur).await;
}
#[cfg(not(test))]
async fn sleep(dur: std::time::Duration) {
    Delay::from(dur).await;
}

// --- HttpFetch seam ---

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub trait HttpFetch {
    async fn post_json(&self, url: &str, token: &str, body: &serde_json::Value, timeout_secs: u64) -> std::result::Result<HttpResponse, AppError>;
}

pub struct RealHttpFetch;

impl HttpFetch for RealHttpFetch {
    async fn post_json(&self, url: &str, token: &str, body: &serde_json::Value, timeout_secs: u64) -> std::result::Result<HttpResponse, AppError> {
        let headers = Headers::new();
        headers.set("User-Agent", "iceberg-rust-api/1.0")?;
        headers.set("Authorization", &format!("Bearer {}", token))?;
        headers.set("Content-Type", "application/json")?;

        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(serde_json::to_vec(body)?.into()));

        let request = Request::new_with_init(url, &init)?;
        let fetch = Fetch::Request(request);
        let fetch_fut = Box::pin(fetch.send());
        let timeout_fut = Box::pin(Delay::from(std::time::Duration::from_secs(timeout_secs)));

        let mut response = match select(fetch_fut, timeout_fut).await {
            Either::Left((resp, _)) => resp?,
            Either::Right(_) => {
                return Err(AppError::Timeout(format!("GitHub API request timed out after {} seconds", timeout_secs)));
            }
        };

        let status = response.status_code();
        let resp_headers = response.headers().clone();
        let mut hdrs = HashMap::new();
        for key in &["X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset", "Retry-After"] {
            if let Ok(Some(val)) = resp_headers.get(key) {
                hdrs.insert(key.to_string(), val);
            }
        }

        if let (Ok(Some(limit)), Ok(Some(rem)), Ok(Some(reset))) = (
            resp_headers.get("X-RateLimit-Limit"),
            resp_headers.get("X-RateLimit-Remaining"),
            resp_headers.get("X-RateLimit-Reset"),
        ) {
            console_log!("GitHub API Rate Limit: {}/{} (Resets: {})", rem, limit, reset);
        }

        let body_bytes = response.bytes().await?.to_vec();
        Ok(HttpResponse { status, headers: hdrs, body: body_bytes })
    }
}

// --- GraphQL execution layer ---

pub(crate) async fn execute_graphql<F, R>(
    fetcher: &F,
    token: &str,
    query: &str,
    variables: serde_json::Value,
    timeout_secs: Option<u64>,
) -> std::result::Result<(R, u64), AppError>
where
    F: HttpFetch,
    R: serde::de::DeserializeOwned,
{
    let timeout = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let payload = json!({ "query": query, "variables": variables });

    let resp = execute_with_retries::<F, GraphQLResponse<R>>(fetcher, token, &payload, timeout).await?;
    let cost = resp.extensions.as_ref().map(|e| e.cost.actual_cost).unwrap_or(0);

    if let Some(errors) = resp.errors {
        if !errors.is_empty() {
            return Err(AppError::GitHubError(
                errors[0]["message"].as_str().unwrap_or("Unknown GraphQL error").to_string(),
            ));
        }
    }

    let data = resp.data.ok_or_else(|| AppError::Internal("No data in GraphQL response".to_string()))?;
    Ok((data, cost))
}

async fn execute_with_retries<F, R>(
    fetcher: &F,
    token: &str,
    payload: &serde_json::Value,
    timeout_secs: u64,
) -> std::result::Result<R, AppError>
where
    F: HttpFetch,
    R: serde::de::DeserializeOwned,
{
    let max_retries = 5;
    let mut retry_count = 0;

    loop {
        let resp = fetcher.post_json(GITHUB_GRAPHQL_URL, token, payload, timeout_secs).await?;

        let is_rate_limited = resp.status == 429
            || (resp.status == 403 && resp.headers.get("X-RateLimit-Remaining").map(|s| s.as_str()) == Some("0"));

        if is_rate_limited {
            if retry_count >= max_retries {
                let retry_after = resp.headers.get("Retry-After")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                return Err(AppError::RateLimited(retry_after));
            }
            let wait = 2u64.pow(retry_count);
            log_warn!("Rate limited. Retrying in {} seconds...", wait);
            sleep(std::time::Duration::from_secs(wait)).await;
            retry_count += 1;
            continue;
        }

        if resp.status != 200 {
            let body = String::from_utf8_lossy(&resp.body).to_string();
            return match resp.status {
                401 | 403 => Err(AppError::Unauthorized(format!("GitHub API unauthorized: {}", body))),
                400 => Err(AppError::BadRequest(format!("GitHub API bad request: {}", body))),
                _ => Err(AppError::GitHubError(format!("GitHub API status {}: {}", resp.status, body))),
            };
        }

        let gql_resp: R = serde_json::from_slice(&resp.body)?;
        return Ok(gql_resp);
    }
}

// --- Fake HttpFetch for tests ---

#[cfg(test)]
pub struct FakeHttpFetch {
    responses: std::cell::RefCell<std::collections::VecDeque<std::result::Result<HttpResponse, AppError>>>,
}

#[cfg(test)]
impl FakeHttpFetch {
    pub fn new() -> Self {
        Self { responses: std::cell::RefCell::new(std::collections::VecDeque::new()) }
    }

    pub fn push_ok(&self, status: u16, body: &str) {
        self.responses.borrow_mut().push_back(Ok(HttpResponse {
            status,
            headers: HashMap::from([
                ("X-RateLimit-Remaining".into(), "4900".into()),
                ("X-RateLimit-Limit".into(), "5000".into()),
                ("X-RateLimit-Reset".into(), "9999999999".into()),
            ]),
            body: body.as_bytes().to_vec(),
        }));
    }

    pub fn push_rate_limited(&self) {
        self.responses.borrow_mut().push_back(Ok(HttpResponse {
            status: 403,
            headers: HashMap::from([
                ("X-RateLimit-Remaining".into(), "0".into()),
                ("X-RateLimit-Limit".into(), "5000".into()),
                ("X-RateLimit-Reset".into(), "9999999999".into()),
            ]),
            body: r#"{"data":null}"#.as_bytes().to_vec(),
        }));
    }

    #[allow(dead_code)]
    pub fn push_err(&self, err: AppError) {
        self.responses.borrow_mut().push_back(Err(err));
    }
}

#[cfg(test)]
impl HttpFetch for FakeHttpFetch {
    async fn post_json(&self, _url: &str, _token: &str, _body: &serde_json::Value, _timeout_secs: u64) -> std::result::Result<HttpResponse, AppError> {
        self.responses.borrow_mut().pop_front().unwrap_or_else(|| {
            Err(AppError::Internal("No more fake responses".to_string()))
        })
    }
}
