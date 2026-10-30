//! Shared blocking HTTP client cache.
//!
//! Creating a `reqwest::blocking::Client` is expensive due to TLS/connection-pool setup.
//! Don't create one per request; build one per timeout and reuse it.
//! `Client::clone` is cheap because the inside is shared via `Arc`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

static CLIENTS: OnceLock<Mutex<HashMap<u128, reqwest::blocking::Client>>> = OnceLock::new();

/// Returns the shared client for a timeout. Each timeout value is created only once.
pub fn shared_client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    let key = timeout.as_millis().max(1);
    let slot = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = slot.lock() {
        if let Some(hit) = guard.get(&key) {
            return Ok(hit.clone());
        }
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("HTTP 준비 실패: {e}"))?;
    if let Ok(mut guard) = slot.lock() {
        guard.insert(key, client.clone());
    }
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_client_per_timeout() {
        let timeout = Duration::from_millis(4321);
        let _a = shared_client(timeout).unwrap();
        let _b = shared_client(timeout).unwrap();
        let guard = CLIENTS.get().unwrap().lock().unwrap();
        assert!(guard.contains_key(&4321));
    }
}
