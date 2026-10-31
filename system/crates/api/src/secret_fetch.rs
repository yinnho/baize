//! 经 unix socket 向 aginx-secretd 取 scope 值。永不打日志。

use aginx_secret::client::{default_socket, request};
use serde_json::json;

/// `op:get` → scope 的字符串值（微信 OA 为 JSON 文本）。
pub fn fetch_secret_json(scope: &str) -> anyhow::Result<String> {
    let sock = default_socket();
    let resp = request(&sock, &json!({"op": "get", "scope": scope})).map_err(|e| {
        anyhow::anyhow!("secretd unreachable ({e}); is aginx-secretd running?")
    })?;
    let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let code = resp
            .pointer("/error/code")
            .and_then(|v| v.as_str())
            .unwrap_or("error");
        let msg = resp
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // 不回显 scope 以外的细节到调用方可接受；scope 名本身非密。
        anyhow::bail!("secretd get {scope}: {code} {msg}");
    }
    resp.pointer("/data/value")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("secretd get {scope}: missing data.value"))
}
