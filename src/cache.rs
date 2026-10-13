use crate::models::QueryParams;
use once_cell::sync::Lazy;
use worker::*;

// --- Request cache seam ---

/// One place owns cache-key derivation. The request pipeline and the cron
/// warmer both derive keys through this function, so they can never diverge
/// (the old warmer built its own key from a hardcoded domain URL and stored
/// entries no request could ever read).
pub fn cache_key(endpoint: &str, username: &str, params: &QueryParams) -> String {
    format!(
        "iceberg:{}:user={}:limit={}:history={}:langs={}",
        endpoint, username, params.limit, params.history_limit, params.language_limit
    )
}

/// Storage at the request-cache seam. The key is derived by the caller via
/// [`cache_key`]; what the adapter does with it is implementation detail.
pub trait CacheStore {
    async fn get(&self, key: &str) -> Result<Option<Response>>;
    async fn put(&self, key: &str, response: Response) -> Result<()>;
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
    fn cache_key_encodes_all_scopes() {
        let key = cache_key("/v2/commits/latest", "test", &params(5, 10, 3));
        assert_eq!(key, "iceberg:/v2/commits/latest:user=test:limit=5:history=10:langs=3");
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
