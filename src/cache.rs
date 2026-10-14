use crate::models::QueryParams;
use once_cell::sync::Lazy;
use worker::*;

// --- Request cache seam ---

/// One place owns cache-key derivation. The request pipeline and the cron
/// warmer both derive keys through this function, so they can never diverge
/// (the old warmer built its own key from a hardcoded domain URL and stored
/// entries no request could ever read).
///
/// The Workers Cache API requires keys to be fully-qualified, valid URLs, so
/// the logical key (endpoint/user/limits) is embedded in a synthetic URL path
/// + query instead of a namespaced string. Component values need no escaping
/// to be URL-safe: `username` is regex-validated `[a-zA-Z0-9-]`, limits are
/// `usize` (digits only).
pub fn cache_key(endpoint: &str, username: &str, params: &QueryParams) -> String {
    format!(
        "https://cache.iceberg/{endpoint}/{username}?limit={}&history={}&langs={}",
        params.limit, params.history_limit, params.language_limit
    )
}

/// Storage at the request-cache seam. The key is derived by the caller via
/// [`cache_key`]; what the adapter does with it is implementation detail.
/// `Resp` is the response value the store moves — `Response` in production
/// (the default), or a test materialization in unit tests, so the pipeline
/// core can run without a worker runtime.
pub trait CacheStore<Resp = Response> {
    async fn get(&self, key: &str) -> Result<Option<Resp>>;
    async fn put(&self, key: &str, response: Resp) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QueryParams;

    fn params(limit: usize, history_limit: usize, language_limit: usize) -> QueryParams {
        QueryParams { username: "test".into(), limit, history_limit, language_limit }
    }

    #[test]
    fn cache_key_includes_language_limit() {
        assert_ne!(
            cache_key("/v2/commits/latest", "test", &params(10, 10, 3)),
            cache_key("/v2/commits/latest", "test", &params(10, 10, 7)),
            "differently-scoped language requests must not collide"
        );
    }

    #[test]
    fn cache_key_is_a_valid_url() {
        // The Workers Cache API rejects non-fully-qualified keys
        // ("Cache API keys must be fully-qualified, valid URLs") — invalid
        // keys made every cache op fail and caching never happened.
        let key = cache_key("/v2/commits/latest", "test", &params(5, 10, 3));
        assert!(key.starts_with("https://"), "key must be a fully-qualified URL: {key}");
        assert!(worker::Url::parse(&key).is_ok(), "key must parse as a URL: {key}");
    }

    #[test]
    fn cache_key_encodes_all_scopes() {
        let key = cache_key("/v2/commits/latest", "test", &params(5, 10, 3));
        assert_eq!(
            key,
            "https://cache.iceberg//v2/commits/latest/test?limit=5&history=10&langs=3"
        );
    }
}

/// Adapter over the Workers Cache API.
#[derive(Clone, Copy)]
pub struct WorkersCache;

static CACHE: Lazy<Cache> = Lazy::new(|| Cache::default());

impl CacheStore for WorkersCache {
    async fn get(&self, key: &str) -> Result<Option<Response>> {
        CACHE.get(key, true).await
    }

    async fn put(&self, key: &str, response: Response) -> Result<()> {
        CACHE.put(key, response).await
    }
}
