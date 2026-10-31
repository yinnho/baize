//! api-leg.toml 声明件加载。

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::channels_root;

#[derive(Debug, Clone)]
pub struct ApiLeg {
    pub channel: String,
    pub path: PathBuf,
    pub tools: Vec<ApiToolDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub auth: Option<ToolAuth>,
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,
    #[serde(default)]
    pub extract: HashMap<String, ExtractDef>,
    #[serde(default)]
    pub error_check: Option<ErrorCheck>,
    /// admin=true：须显式 --admin，且频道 publish_admin_gate=true 才放行。
    #[serde(default)]
    pub admin: bool,
    /// 参数链式抽值：调前步工具/CLI，或从账户/secretd 字段填入。
    #[serde(default)]
    pub resolve: HashMap<String, ResolveDef>,
}

/// `[tool.resolve.<param>]`：调用前把入参换成抽到的值。
#[derive(Debug, Clone, Deserialize)]
pub struct ResolveDef {
    /// 调另一声明工具取前步输出。
    #[serde(default)]
    pub tool: Option<String>,
    /// 调外部 CLI（如 aginx-mp-material-upload）；值为可执行名或路径。
    #[serde(default)]
    pub cli: Option<String>,
    /// 传给目标工具的参数名；CLI 时作为位置文件参数的语义标签（可选）。
    #[serde(default)]
    pub param: Option<String>,
    /// 从本工具哪一个入参取值作为输入（默认=被 resolve 的键）。
    #[serde(default)]
    pub from_param: Option<String>,
    /// 从目标结果 / 账户 meta / secretd JSON 抽哪个字段。
    pub extract: String,
    /// missing | empty | is_file | always | not_empty（默认 always；若键缺失且 condition=missing 亦触发）。
    #[serde(default)]
    pub condition: Option<String>,
    /// 直接从账户 meta（id / nickname / …）。
    #[serde(default)]
    pub from_account: Option<String>,
    /// 直接从 secretd JSON 字段（非 auth.fields 注入场景）。
    #[serde(default)]
    pub from_secret: Option<String>,
    /// CLI 附加参数（在文件参数之前），如 `["--type", "image"]`。
    #[serde(default)]
    pub cli_args: Vec<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolAuth {
    #[serde(default)]
    pub account_kind: Option<String>,
    /// 账户 JSON 字段 → query 参数名。
    #[serde(default)]
    pub fields: HashMap<String, String>,
    /// token 链：自动调另一工具取 token。
    #[serde(default)]
    pub token: Option<TokenChain>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenChain {
    pub tool: String,
    pub param: String,
    #[serde(default = "default_cache_s")]
    pub cache_s: u64,
    #[serde(default = "default_retry_on")]
    pub retry_on: i64,
}

fn default_cache_s() -> u64 {
    7000
}
fn default_retry_on() -> i64 {
    40001
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamDef {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractDef {
    pub path: String,
    #[serde(default, rename = "type")]
    pub value_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorCheck {
    pub field: String,
    #[serde(default)]
    pub expect_absent: bool,
    #[serde(default)]
    pub expect: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct LegFile {
    #[serde(default)]
    tool: Vec<ApiToolDef>,
}

/// 加载单个 `api-leg.toml`。
pub fn load_leg(path: &Path, channel: &str) -> anyhow::Result<ApiLeg> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let file: LegFile = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
    Ok(ApiLeg {
        channel: channel.to_string(),
        path: path.to_path_buf(),
        tools: file.tool,
    })
}

/// 扫 `channels/*/leg/api-leg.toml`（可限 channel 名）。
pub fn scan_legs(only: Option<&str>) -> anyhow::Result<Vec<ApiLeg>> {
    let root = channels_root();
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&root) {
        Ok(r) => r,
        Err(e) => anyhow::bail!("channels root {}: {e}", root.display()),
    };
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if let Some(only) = only {
            if name != only {
                continue;
            }
        }
        if !ent.path().is_dir() {
            continue;
        }
        let leg_path = ent.path().join("leg").join("api-leg.toml");
        if !leg_path.is_file() {
            continue;
        }
        out.push(load_leg(&leg_path, &name)?);
    }
    out.sort_by(|a, b| a.channel.cmp(&b.channel));
    Ok(out)
}

impl ApiLeg {
    pub fn find_tool(&self, name: &str) -> Option<&ApiToolDef> {
        self.tools.iter().find(|t| t.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[tool]]
name = "weixin_mp_token"
description = "token"
url = "https://api.weixin.qq.com/cgi-bin/token"
method = "GET"

[tool.auth]
account_kind = "weixin-oa"

[tool.auth.fields]
appid = "appid"
secret = "secret"

[tool.params]
grant_type = { type = "string", default = "client_credential" }
account = { type = "string" }

[tool.extract]
access_token = { path = "access_token" }
expires_in = { path = "expires_in", type = "int" }

[tool.error_check]
field = "errcode"
expect_absent = true

[[tool]]
name = "weixin_mp_draft_add"
url = "https://api.weixin.qq.com/cgi-bin/draft/add"
method = "POST"

[tool.auth]
account_kind = "weixin-oa"
token = { tool = "weixin_mp_token", param = "access_token", cache_s = 7000, retry_on = 40001 }

[tool.params]
title = { required = true, type = "string" }
content_html = { required = true, type = "string" }
thumb_media_id = { required = true, type = "string" }

[tool.extract]
draft_media_id = { path = "media_id" }

[tool.error_check]
field = "errcode"
expect_absent = true
"#;

    #[test]
    fn parses_weixin_oa_sample() {
        let file: LegFile = toml::from_str(SAMPLE).unwrap();
        assert_eq!(file.tool.len(), 2);
        assert_eq!(file.tool[0].name, "weixin_mp_token");
        assert!(file.tool[0].error_check.as_ref().unwrap().expect_absent);
        let auth = file.tool[1].auth.as_ref().unwrap();
        assert_eq!(auth.token.as_ref().unwrap().tool, "weixin_mp_token");
        assert_eq!(auth.token.as_ref().unwrap().retry_on, 40001);
        assert!(file.tool[1].params["title"].required);
    }

    #[test]
    fn parses_resolve_and_admin() {
        let raw = r#"
[[tool]]
name = "weixin_mp_draft_add"
url = "https://example/draft/add"
method = "POST"
admin = false

[tool.params]
thumb_media_id = { required = true, type = "string" }
cover_file = { type = "string" }

[tool.resolve.thumb_media_id]
cli = "aginx-mp-material-upload"
from_param = "cover_file"
extract = "media_id"
condition = "is_file"
cli_args = ["--type", "image"]

[[tool]]
name = "weixin_mp_freepublish_submit"
url = "https://example/freepublish/submit"
method = "POST"
admin = true
"#;
        let file: LegFile = toml::from_str(raw).unwrap();
        assert!(file.tool[1].admin);
        let r = &file.tool[0].resolve["thumb_media_id"];
        assert_eq!(r.cli.as_deref(), Some("aginx-mp-material-upload"));
        assert_eq!(r.from_param.as_deref(), Some("cover_file"));
        assert_eq!(r.extract, "media_id");
        assert_eq!(r.cli_args, vec!["--type", "image"]);
    }
}
