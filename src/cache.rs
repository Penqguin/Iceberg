use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use worker::*;

const CACHE_DURATION_HOURS: i64 = 1;

pub async fn get_cached<T: DeserializeOwned>(
    bucket: &Bucket,
    username: &str,
    endpoint: &str,
) -> Result<Option<(T, bool)>> {
    let key = format!("{}:{}", username.to_lowercase(), endpoint);
    let object = bucket.get(&key).execute().await?;

    match object {
        Some(obj) => {
            let metadata = obj.custom_metadata()?;
            let last_fetched: DateTime<Utc> = metadata
                .get("last_fetched_at")
                .and_then(|s| s.parse::<i64>().ok())
                .and_then(|ts| DateTime::from_timestamp(ts, 0))
                .unwrap_or_default();

            let elapsed = Utc::now() - last_fetched;
            let needs_refresh = elapsed.num_hours() >= CACHE_DURATION_HOURS;

            let body = obj.body()
                .ok_or_else(|| Error::from("R2 object has no body"))?;
            let bytes = body.bytes().await?;
            let data: T = serde_json::from_slice(&bytes)
                .map_err(|e| Error::from(e.to_string()))?;

            Ok(Some((data, needs_refresh)))
        }
        None => Ok(None),
    }
}

pub async fn set_cached<T: Serialize>(
    bucket: &Bucket,
    username: &str,
    endpoint: &str,
    data: &T,
) -> Result<()> {
    let key = format!("{}:{}", username.to_lowercase(), endpoint);
    let bytes = serde_json::to_vec(data)
        .map_err(|e| Error::from(e.to_string()))?;

    let now = Utc::now().timestamp();
    let mut metadata = HashMap::new();
    metadata.insert("last_fetched_at".to_string(), now.to_string());

    bucket.put(&key, bytes)
        .custom_metadata(metadata)
        .execute()
        .await?;

    Ok(())
}
