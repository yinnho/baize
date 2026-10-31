//! access_token 进程外缓存（0600 文件）。只存 token 与到期时刻；路径不含密文。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    access_token: String,
    /// unix 秒：过期时刻（已扣 margin）。
    expires_at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("AGINX_API_TOKEN_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/aginx/api-leg-token"))
}

fn entry_path(scope: &str) -> PathBuf {
    // scope 可能含 ':' → 文件名安全替换
    let safe: String = scope
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    cache_dir().join(format!("{safe}.json"))
}

pub fn get_cached(scope: &str) -> Option<String> {
    let path = entry_path(scope);
    let text = std::fs::read_to_string(&path).ok()?;
    let ent: CacheEntry = serde_json::from_str(&text).ok()?;
    if now_secs() >= ent.expires_at {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    if ent.access_token.is_empty() {
        return None;
    }
    Some(ent.access_token)
}

/// `expires_in` 为微信返回秒数；`margin_s` 提前失效（对齐 cache_s≈7000）。
pub fn put_cached(scope: &str, access_token: &str, expires_in: u64, margin_s: u64) {
    let ttl = expires_in.saturating_sub(margin_s.min(expires_in.saturating_sub(1)));
    let ent = CacheEntry {
        access_token: access_token.to_string(),
        expires_at: now_secs().saturating_add(ttl),
    };
    let dir = cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = entry_path(scope);
    let tmp = path.with_extension("json.tmp");
    if let Ok(body) = serde_json::to_string(&ent) {
        if std::fs::write(&tmp, body).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            }
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

pub fn invalidate(scope: &str) {
    let _ = std::fs::remove_file(entry_path(scope));
}

#[allow(dead_code)]
pub fn cache_path_for(scope: &str) -> PathBuf {
    entry_path(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_cache() {
        let _g = testkit::env_lock();
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("AGINX_API_TOKEN_CACHE", d.path());
        put_cached("weixin-oa:test", "tok-abc", 7200, 200);
        assert_eq!(get_cached("weixin-oa:test").as_deref(), Some("tok-abc"));
        invalidate("weixin-oa:test");
        assert!(get_cached("weixin-oa:test").is_none());
        std::env::remove_var("AGINX_API_TOKEN_CACHE");
    }
}
