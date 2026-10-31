//! 声明工具执行：账户选择 → secretd → resolve 链 → HTTP；
//! token 链 + 40001 重取；admin ↔ publish_admin_gate 联检。

use serde_json::{json, Map, Value};
use std::collections::HashMap;

use crate::account::{resolve_account, AccountCreds};
use crate::leg::{ApiLeg, ApiToolDef, ErrorCheck};
use crate::token_cache;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub channel: Option<String>,
    pub account: Option<String>,
    /// 调用方参数（覆盖 default）。
    pub params: HashMap<String, Value>,
    /// 允许跑 admin=true 工具。
    pub admin: bool,
    /// token 工具验收：stdout 只露前缀，不返回完整 token。
    pub redact_token: bool,
    /// resolve 嵌套深度（内部；CLI 保持 0）。
    pub resolve_depth: u8,
}

#[derive(Debug)]
pub struct RunResult {
    pub tool: String,
    pub channel: String,
    pub account_id: String,
    /// 抽取后的字段（可能已打码）。
    pub extracted: Map<String, Value>,
    /// 原始响应是否经 error_check。
    pub ok: bool,
    pub errmsg: Option<String>,
    pub errcode: Option<i64>,
    pub from_cache: bool,
}

pub fn run_tool(tool_name: &str, opts: &RunOptions) -> anyhow::Result<RunResult> {
    let legs = crate::leg::scan_legs(opts.channel.as_deref())?;
    if legs.is_empty() {
        anyhow::bail!(
            "no api-leg.toml found under {}{}",
            crate::channels_root().display(),
            opts.channel
                .as_ref()
                .map(|c| format!(" (channel={c})"))
                .unwrap_or_default()
        );
    }
    let (leg, tool) = find_tool(&legs, tool_name)?;
    let policy = crate::policy::load_channel_policy(&leg.channel)?;
    crate::policy::check_admin_gate(tool_name, tool.admin, opts.admin, &policy)
        .map_err(anyhow::Error::msg)?;
    let kind = tool
        .auth
        .as_ref()
        .and_then(|a| a.account_kind.clone())
        .unwrap_or_else(|| leg.channel.clone());
    let creds = resolve_account(&kind, opts.account.as_deref())?;
    run_tool_with_creds(leg, tool, &creds, opts)
}

/// 已选账户后的执行入口（供 resolve 嵌套调前步工具）。
pub fn run_tool_with_creds(
    leg: &ApiLeg,
    tool: &ApiToolDef,
    creds: &AccountCreds,
    opts: &RunOptions,
) -> anyhow::Result<RunResult> {
    let mut params = opts.params.clone();
    crate::resolve::apply_resolves(leg, tool, creds, opts, &mut params)?;
    let mut nested = opts.clone();
    nested.params = params;
    execute_with_token_retry(leg, tool, creds, &nested)
}


fn find_tool<'a>(
    legs: &'a [ApiLeg],
    name: &str,
) -> anyhow::Result<(&'a ApiLeg, &'a ApiToolDef)> {
    for leg in legs {
        if let Some(t) = leg.find_tool(name) {
            return Ok((leg, t));
        }
    }
    anyhow::bail!("tool not found in loaded legs: {name}")
}

fn execute_with_token_retry(
    leg: &ApiLeg,
    tool: &ApiToolDef,
    creds: &AccountCreds,
    opts: &RunOptions,
) -> anyhow::Result<RunResult> {
    let chain = tool.auth.as_ref().and_then(|a| a.token.as_ref());
    let mut from_cache = false;
    let access_token = if let Some(chain) = chain {
        match token_cache::get_cached(&creds.scope) {
            Some(t) => {
                from_cache = true;
                Some(t)
            }
            None => {
                let t = fetch_fresh_token(leg, chain, creds, opts)?;
                Some(t)
            }
        }
    } else {
        None
    };

    let mut result = http_call(tool, creds, opts, access_token.as_deref())?;
    result.from_cache = from_cache;
    result.channel = leg.channel.clone();
    result.account_id = creds.meta.id.clone();

    if let Some(chain) = chain {
        if result.errcode == Some(chain.retry_on) {
            token_cache::invalidate(&creds.scope);
            let fresh = fetch_fresh_token(leg, chain, creds, opts)?;
            result = http_call(tool, creds, opts, Some(&fresh))?;
            result.from_cache = false;
            result.channel = leg.channel.clone();
            result.account_id = creds.meta.id.clone();
        }
    }

    // token 工具：写缓存 + 可选打码
    if tool.name.contains("token") || tool.extract.contains_key("access_token") {
        if let (Some(tok), Some(exp)) = (
            result.extracted.get("access_token").and_then(|v| v.as_str()),
            result
                .extracted
                .get("expires_in")
                .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64))),
        ) {
            let margin = tool
                .auth
                .as_ref()
                .and_then(|a| a.token.as_ref())
                .map(|t| 7200u64.saturating_sub(t.cache_s))
                .unwrap_or(200);
            // 若本工具自己就是 token 工具，用 extract 写缓存
            token_cache::put_cached(&creds.scope, tok, exp, margin.max(1));
        }
        if opts.redact_token {
            if let Some(Value::String(tok)) = result.extracted.get_mut("access_token") {
                *tok = redact_prefix(tok);
            }
        }
    }

    Ok(result)
}

fn fetch_fresh_token(
    leg: &ApiLeg,
    chain: &crate::leg::TokenChain,
    creds: &AccountCreds,
    opts: &RunOptions,
) -> anyhow::Result<String> {
    let token_tool = leg
        .find_tool(&chain.tool)
        .ok_or_else(|| anyhow::anyhow!("token chain tool missing: {}", chain.tool))?;
    let mut nested = opts.clone();
    nested.redact_token = false;
    // token 工具本身不带 token 链
    let mut r = http_call(token_tool, creds, &nested, None)?;
    r.channel = leg.channel.clone();
    r.account_id = creds.meta.id.clone();
    if !r.ok {
        anyhow::bail!(
            "token fetch failed: errcode={:?} errmsg={:?}",
            r.errcode,
            r.errmsg
        );
    }
    let tok = r
        .extracted
        .get(&chain.param)
        .or_else(|| r.extracted.get("access_token"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("token tool response missing {}", chain.param))?
        .to_string();
    let exp = r
        .extracted
        .get("expires_in")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
        .unwrap_or(7200);
    let margin = 7200u64.saturating_sub(chain.cache_s).max(1);
    token_cache::put_cached(&creds.scope, &tok, exp, margin);
    Ok(tok)
}

fn http_call(
    tool: &ApiToolDef,
    creds: &AccountCreds,
    opts: &RunOptions,
    access_token: Option<&str>,
) -> anyhow::Result<RunResult> {
    let mut query: Vec<(String, String)> = Vec::new();
    let mut body_fields: Map<String, Value> = Map::new();

    // defaults + caller params
    for (name, def) in &tool.params {
        if name == "account" {
            continue; // 选择器已处理
        }
        let val = opts
            .params
            .get(name)
            .cloned()
            .or_else(|| def.default.clone());
        match val {
            Some(v) => {
                if tool.method.eq_ignore_ascii_case("POST") {
                    // weixin draft: map content_html → content
                    let key = if name == "content_html" {
                        "content".to_string()
                    } else {
                        name.clone()
                    };
                    body_fields.insert(key, v);
                } else {
                    query.push((name.clone(), value_to_string(&v)));
                }
            }
            None if def.required => {
                anyhow::bail!("missing required param `{name}`");
            }
            None => {}
        }
    }

    // auth fields → query（token 工具：appid/secret）
    if let Some(auth) = &tool.auth {
        for (field, qname) in &auth.fields {
            let Some(v) = creds.field_str(field) else {
                anyhow::bail!("account secret missing field `{field}`");
            };
            query.push((qname.clone(), v.to_string()));
        }
    }

    if let Some(tok) = access_token {
        query.push(("access_token".into(), tok.to_string()));
    }

    let url = append_query(&tool.url, &query);

    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
        .build()
        .new_agent();

    let resp = if tool.method.eq_ignore_ascii_case("POST") {
        // 微信 draft/add：articles 单层包裹
        let payload = if tool.name.contains("draft") {
            wrap_draft_articles(body_fields)
        } else if tool.name.contains("freepublish") {
            Value::Object(body_fields)
        } else if body_fields.is_empty() {
            json!({})
        } else {
            Value::Object(body_fields)
        };
        agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(&payload)?)
    } else {
        agent.get(&url).call()
    };

    let resp = resp.map_err(|e| anyhow::anyhow!("HTTP {}: {e}", tool.name))?;
    let status = resp.status();
    let body_text = resp
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("read body: {e}"))?;

    let parsed: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
        json!({"_raw": body_text, "_http_status": status.as_u16()})
    });

    let (ok, errcode, errmsg) = check_error(&parsed, tool.error_check.as_ref());
    let mut extracted = extract_fields(&parsed, tool);

    // 确保 expires_in 为数字
    if let Some(v) = extracted.get("expires_in").cloned() {
        if let Some(s) = v.as_str() {
            if let Ok(n) = s.parse::<i64>() {
                extracted.insert("expires_in".into(), json!(n));
            }
        }
    }

    Ok(RunResult {
        tool: tool.name.clone(),
        channel: String::new(),
        account_id: String::new(),
        extracted,
        ok,
        errmsg,
        errcode,
        from_cache: false,
    })
}

fn wrap_draft_articles(mut fields: Map<String, Value>) -> Value {
    // need_open_comment default
    if !fields.contains_key("need_open_comment") {
        fields.insert("need_open_comment".into(), json!(0));
    }
    if !fields.contains_key("only_fans_can_comment") {
        fields.insert("only_fans_can_comment".into(), json!(0));
    }
    json!({ "articles": [Value::Object(fields)] })
}

fn check_error(parsed: &Value, check: Option<&ErrorCheck>) -> (bool, Option<i64>, Option<String>) {
    let errcode = parsed.get("errcode").and_then(|v| v.as_i64());
    let errmsg = parsed
        .get("errmsg")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let Some(check) = check else {
        return (errcode.unwrap_or(0) == 0 || errcode.is_none(), errcode, errmsg);
    };
    if check.expect_absent {
        // 微信：成功无 errcode
        if parsed.get(&check.field).is_none() {
            return (true, None, None);
        }
        // 有 errcode 且为 0 也算成功
        if errcode == Some(0) {
            return (true, errcode, errmsg);
        }
        return (false, errcode, errmsg);
    }
    if let Some(expect) = &check.expect {
        let actual = parsed.get(&check.field);
        let ok = actual == Some(expect);
        return (ok, errcode, errmsg);
    }
    (errcode.unwrap_or(0) == 0, errcode, errmsg)
}

fn extract_fields(parsed: &Value, tool: &ApiToolDef) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, ex) in &tool.extract {
        if let Some(v) = navigate(parsed, &ex.path) {
            out.insert(name.clone(), v.clone());
        }
    }
    // 若无 extract 配置，把常用字段带出（不带 secret）
    if out.is_empty() {
        for k in ["media_id", "publish_id", "access_token", "expires_in", "url"] {
            if let Some(v) = parsed.get(k) {
                out.insert(k.to_string(), v.clone());
            }
        }
    }
    out
}

fn navigate<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        if seg.is_empty() {
            continue;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

fn append_query(base: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }
    let q: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect();
    if base.contains('?') {
        format!("{base}&{}", q.join("&"))
    } else {
        format!("{base}?{}", q.join("&"))
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub fn redact_prefix(tok: &str) -> String {
    let prefix: String = tok.chars().take(8).collect();
    if tok.len() <= 8 {
        format!("{prefix}…")
    } else {
        format!("{prefix}…({} chars)", tok.len())
    }
}

impl RunResult {
    pub fn to_json(&self) -> Value {
        let mut data = self.extracted.clone();
        data.insert("tool".into(), json!(self.tool));
        data.insert("channel".into(), json!(self.channel));
        data.insert("account".into(), json!(self.account_id));
        data.insert("from_cache".into(), json!(self.from_cache));
        if let Some(c) = self.errcode {
            data.insert("errcode".into(), json!(c));
        }
        if let Some(m) = &self.errmsg {
            data.insert("errmsg".into(), json!(m));
        }
        if self.ok {
            agio_ok_data(Value::Object(data))
        } else {
            json!({
                "ok": false,
                "error": {
                    "type": "state",
                    "code": "weixin_api_error",
                    "message": self.errmsg.clone().unwrap_or_else(|| format!("errcode={:?}", self.errcode)),
                },
                "data": Value::Object(data),
            })
        }
    }
}

fn agio_ok_data(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}
