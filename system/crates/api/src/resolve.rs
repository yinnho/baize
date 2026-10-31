//! `[tool.resolve]` 链式抽值：账户 / secretd / 前步工具 / CLI。

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::account::AccountCreds;
use crate::leg::{ApiLeg, ApiToolDef, ResolveDef};
use crate::runtime::RunOptions;

/// 按声明改写 `params`（就地）。`depth` 防环。
pub fn apply_resolves(
    leg: &ApiLeg,
    tool: &ApiToolDef,
    creds: &AccountCreds,
    opts: &RunOptions,
    params: &mut HashMap<String, Value>,
) -> anyhow::Result<()> {
    if tool.resolve.is_empty() {
        return Ok(());
    }
    if opts.resolve_depth > 4 {
        anyhow::bail!("resolve chain too deep (>4) on tool {}", tool.name);
    }
    // 稳定顺序：按键名排序，避免 HashMap 次序抖动。
    let mut keys: Vec<&String> = tool.resolve.keys().collect();
    keys.sort();
    for key in keys {
        let def = &tool.resolve[key];
        let target = params.get(key).cloned();
        let input_key = def.from_param.as_deref().unwrap_or(key.as_str());
        let input = params.get(input_key).cloned();
        let cond = effective_condition(def);
        if !should_resolve(&cond, target.as_ref(), input.as_ref()) {
            continue;
        }
        let value = resolve_one(leg, tool, creds, opts, def, input.as_ref())?;
        params.insert(key.clone(), value);
    }
    Ok(())
}

fn effective_condition(def: &ResolveDef) -> String {
    if let Some(c) = &def.condition {
        return c.clone();
    }
    if def.from_account.is_some() || def.from_secret.is_some() {
        "missing".into()
    } else if def.cli.is_some() {
        "is_file".into()
    } else {
        "always".into()
    }
}

fn should_resolve(cond: &str, target: Option<&Value>, input: Option<&Value>) -> bool {
    match cond {
        "missing" => match target {
            None => true,
            Some(Value::Null) => true,
            Some(Value::String(s)) if s.is_empty() => true,
            _ => false,
        },
        "empty" => match target {
            None => true,
            Some(Value::Null) => true,
            Some(Value::String(s)) => s.is_empty(),
            _ => false,
        },
        "is_file" => input
            .and_then(|v| v.as_str())
            .map(|p| Path::new(p).is_file())
            .unwrap_or(false),
        "not_empty" => input
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "always" | "" => true,
        other => {
            // 未知条件：保守不触发，避免误调发布类工具
            let _ = other;
            false
        }
    }
}

fn resolve_one(
    leg: &ApiLeg,
    _tool: &ApiToolDef,
    creds: &AccountCreds,
    opts: &RunOptions,
    def: &ResolveDef,
    input: Option<&Value>,
) -> anyhow::Result<Value> {
    if let Some(field) = &def.from_account {
        return account_field(creds, field).map(Value::String);
    }
    if let Some(field) = &def.from_secret {
        let v = creds
            .fields
            .get(field)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("resolve from_secret: missing field `{field}`"))?;
        return Ok(v);
    }
    if let Some(cli) = &def.cli {
        let path = input
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("resolve cli `{cli}`: input path missing"))?;
        return run_cli(cli, &def.cli_args, path, &leg.channel, creds, &def.extract);
    }
    if let Some(other) = &def.tool {
        let path_or_val = input.cloned().unwrap_or(Value::Null);
        return run_other_tool(leg, other, def, path_or_val, creds, opts);
    }
    anyhow::bail!("resolve entry has no tool/cli/from_account/from_secret")
}

fn account_field(creds: &AccountCreds, field: &str) -> anyhow::Result<String> {
    match field {
        "id" | "account_id" => Ok(creds.meta.id.clone()),
        "nickname" => creds
            .meta
            .nickname
            .clone()
            .ok_or_else(|| anyhow::anyhow!("resolve from_account: nickname absent")),
        "account_type" => creds
            .meta
            .account_type
            .clone()
            .ok_or_else(|| anyhow::anyhow!("resolve from_account: account_type absent")),
        "scope" => Ok(creds.scope.clone()),
        other => anyhow::bail!("resolve from_account: unsupported field `{other}`"),
    }
}


fn resolve_cli_path(cli: &str) -> String {
    let p = Path::new(cli);
    if p.is_absolute() || cli.contains('/') {
        return cli.to_string();
    }
    // 机上 /var/bin 是面；ssh/svc 的 PATH 常不含它
    let cand = format!("/var/bin/{cli}");
    if Path::new(&cand).is_file() {
        return cand;
    }
    cli.to_string()
}

fn run_cli(
    cli: &str,
    extra_args: &[String],
    file: &str,
    channel: &str,
    creds: &AccountCreds,
    extract: &str,
) -> anyhow::Result<Value> {
    let exe = resolve_cli_path(cli);
    let mut cmd = Command::new(&exe);
    cmd.arg("--channel").arg(channel);
    cmd.arg("--account").arg(&creds.meta.id);
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.arg(file);
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("resolve cli `{cli}` spawn: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "resolve cli `{cli}` exit={:?}: {}",
            output.status.code(),
            trim_line(stdout.as_ref()).or_else(|| trim_line(stderr.as_ref()))
                .unwrap_or("no output")
        );
    }
    let parsed: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow::anyhow!("resolve cli `{cli}`: bad json stdout: {e}"))?;
    extract_from_envelope(&parsed, extract)
        .ok_or_else(|| anyhow::anyhow!("resolve cli `{cli}`: missing extract `{extract}`"))
}

fn run_other_tool(
    leg: &ApiLeg,
    other_name: &str,
    def: &ResolveDef,
    input: Value,
    creds: &AccountCreds,
    opts: &RunOptions,
) -> anyhow::Result<Value> {
    let other = leg
        .find_tool(other_name)
        .ok_or_else(|| anyhow::anyhow!("resolve tool `{other_name}` not in channel {}", leg.channel))?;
    if other.admin && !opts.admin {
        anyhow::bail!(
            "resolve tool `{other_name}` is admin-gated; cannot chain without --admin"
        );
    }
    let pass_as = def.param.as_deref().unwrap_or("input");
    let mut nested_params = HashMap::new();
    for (k, v) in &opts.params {
        if k == "account" {
            continue;
        }
        nested_params.insert(k.clone(), v.clone());
    }
    nested_params.insert(pass_as.to_string(), input);
    let mut nested_opts = opts.clone();
    nested_opts.params = nested_params;
    nested_opts.resolve_depth = opts.resolve_depth.saturating_add(1);
    let r = crate::runtime::run_tool_with_creds(leg, other, creds, &nested_opts)?;
    r.extracted
        .get(&def.extract)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "resolve tool `{other_name}`: extract `{}` missing in result",
                def.extract
            )
        })
}

fn extract_from_envelope(parsed: &Value, field: &str) -> Option<Value> {
    if let Some(v) = parsed.pointer(&format!("/data/{field}")).cloned() {
        if !v.is_null() {
            return Some(v);
        }
    }
    parsed.get(field).cloned().filter(|v| !v.is_null())
}

fn trim_line(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.lines().next().unwrap_or(t))
    }
}

/// 单测用：条件判断。
#[cfg(test)]
pub fn should_resolve_for_test(
    cond: &str,
    target: Option<&Value>,
    input: Option<&Value>,
) -> bool {
    should_resolve(cond, target, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::leg::ResolveDef;
    use std::fs;

    #[test]
    fn condition_is_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("cover.jpg");
        fs::write(&f, b"x").unwrap();
        let v = Value::String(f.to_string_lossy().into_owned());
        assert!(should_resolve("is_file", None, Some(&v)));
        assert!(!should_resolve(
            "is_file",
            None,
            Some(&Value::String("/no/such/file".into()))
        ));
    }

    #[test]
    fn condition_missing() {
        assert!(should_resolve("missing", None, None));
        assert!(!should_resolve(
            "missing",
            Some(&Value::String("already".into())),
            None
        ));
    }

    #[test]
    fn extract_envelope() {
        let v = json!({"ok": true, "data": {"media_id": "MID123", "url": "u"}});
        assert_eq!(
            extract_from_envelope(&v, "media_id").unwrap(),
            json!("MID123")
        );
    }

    #[test]
    fn default_condition_for_account() {
        let d = ResolveDef {
            tool: None,
            cli: None,
            param: None,
            from_param: None,
            extract: "id".into(),
            condition: None,
            from_account: Some("id".into()),
            from_secret: None,
            cli_args: vec![],
        };
        assert_eq!(effective_condition(&d), "missing");
    }

    #[test]
    fn apply_from_account_fills_missing() {
        use crate::account::{AccountCreds, AccountMeta, SecretRef};
        use crate::leg::{ApiToolDef, ResolveDef};
        use crate::runtime::RunOptions;
        use zeroize::Zeroizing;

        let mut resolve = std::collections::HashMap::new();
        resolve.insert(
            "account_label".into(),
            ResolveDef {
                tool: None,
                cli: None,
                param: None,
                from_param: None,
                extract: "nickname".into(),
                condition: Some("missing".into()),
                from_account: Some("nickname".into()),
                from_secret: None,
                cli_args: vec![],
            },
        );
        let tool = ApiToolDef {
            name: "t".into(),
            description: String::new(),
            url: "https://example".into(),
            method: "GET".into(),
            auth: None,
            params: Default::default(),
            extract: Default::default(),
            error_check: None,
            admin: false,
            resolve,
        };
        let leg = ApiLeg {
            channel: "weixin-oa".into(),
            path: std::path::PathBuf::from("/tmp/x"),
            tools: vec![],
        };
        let creds = AccountCreds {
            meta: AccountMeta {
                id: "wxAPP".into(),
                nickname: Some("主号".into()),
                account_type: None,
                secret: SecretRef {
                    provider: "secretd".into(),
                    scope: "weixin-oa:wxAPP".into(),
                    fields: vec![],
                },
            },
            scope: "weixin-oa:wxAPP".into(),
            raw_json: Zeroizing::new("{}".into()),
            fields: Default::default(),
        };
        let opts = RunOptions::default();
        let mut params = std::collections::HashMap::new();
        apply_resolves(&leg, &tool, &creds, &opts, &mut params).unwrap();
        assert_eq!(params.get("account_label").unwrap().as_str(), Some("主号"));
        // already set → missing 条件不覆盖
        params.insert("account_label".into(), serde_json::json!("keep"));
        apply_resolves(&leg, &tool, &creds, &opts, &mut params).unwrap();
        assert_eq!(params.get("account_label").unwrap().as_str(), Some("keep"));
    }

    #[test]
    fn apply_from_secret_field() {
        use crate::account::{AccountCreds, AccountMeta, SecretRef};
        use crate::leg::{ApiToolDef, ResolveDef};
        use crate::runtime::RunOptions;
        use zeroize::Zeroizing;

        let mut resolve = std::collections::HashMap::new();
        resolve.insert(
            "appid_out".into(),
            ResolveDef {
                tool: None,
                cli: None,
                param: None,
                from_param: None,
                extract: "appid".into(),
                condition: Some("missing".into()),
                from_account: None,
                from_secret: Some("appid".into()),
                cli_args: vec![],
            },
        );
        let tool = ApiToolDef {
            name: "t".into(),
            description: String::new(),
            url: "https://example".into(),
            method: "GET".into(),
            auth: None,
            params: Default::default(),
            extract: Default::default(),
            error_check: None,
            admin: false,
            resolve,
        };
        let leg = ApiLeg {
            channel: "weixin-oa".into(),
            path: std::path::PathBuf::from("/tmp/x"),
            tools: vec![],
        };
        let mut fields = serde_json::Map::new();
        fields.insert("appid".into(), serde_json::json!("wxSECRETAPP"));
        let creds = AccountCreds {
            meta: AccountMeta {
                id: "wxSECRETAPP".into(),
                nickname: None,
                account_type: None,
                secret: SecretRef {
                    provider: "secretd".into(),
                    scope: "weixin-oa:wxSECRETAPP".into(),
                    fields: vec!["appid".into()],
                },
            },
            scope: "weixin-oa:wxSECRETAPP".into(),
            raw_json: Zeroizing::new(r#"{"appid":"wxSECRETAPP"}"#.into()),
            fields,
        };
        let opts = RunOptions::default();
        let mut params = std::collections::HashMap::new();
        apply_resolves(&leg, &tool, &creds, &opts, &mut params).unwrap();
        assert_eq!(params["appid_out"], serde_json::json!("wxSECRETAPP"));
    }
}
