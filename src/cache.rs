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
        "iceberg:{}:user={}:limit={}:history={}",
        endpoint, username, params.limit, params.history_limit
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
