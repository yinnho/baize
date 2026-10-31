//! 账户选择器：`channels/<kind>/accounts/<id>/account.toml` → secretd scope。

use serde::Deserialize;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::secret_fetch::fetch_secret_json;

#[derive(Debug, Clone, Deserialize)]
pub struct AccountMeta {
    pub id: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub account_type: Option<String>,
    pub secret: SecretRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecretRef {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub scope: String,
    #[serde(default)]
    pub fields: Vec<String>,
}

fn default_provider() -> String {
    "secretd".to_string()
}

/// 内存中的账户凭据（JSON 字段）；Drop 时尽量清零。
#[derive(Debug)]
pub struct AccountCreds {
    pub meta: AccountMeta,
    pub scope: String,
    /// secretd 返回的原始 JSON 字符串（含 appid/secret）。
    pub raw_json: Zeroizing<String>,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl AccountCreds {
    pub fn field_str(&self, name: &str) -> Option<&str> {
        self.fields.get(name).and_then(|v| v.as_str())
    }
}

/// 按 appid 或 nickname 解析账户；`account` 为空则取第一个（频道默认号）。
pub fn resolve_account(
    channel_kind: &str,
    account: Option<&str>,
) -> anyhow::Result<AccountCreds> {
    let root = crate::channels_root().join(channel_kind).join("accounts");
    if !root.is_dir() {
        anyhow::bail!("no accounts dir at {}", root.display());
    }
    let metas = list_accounts(&root)?;
    if metas.is_empty() {
        anyhow::bail!("no account.toml under {}", root.display());
    }
    let meta = match account {
        None | Some("") => metas
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty accounts"))?,
        Some(sel) => metas
            .into_iter()
            .find(|m| m.id == sel || m.nickname.as_deref() == Some(sel))
            .ok_or_else(|| anyhow::anyhow!("account not found: {sel}"))?,
    };
    if meta.secret.provider != "secretd" {
        anyhow::bail!(
            "unsupported secret provider {:?} (need secretd)",
            meta.secret.provider
        );
    }
    let raw = fetch_secret_json(&meta.secret.scope)?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("secretd value for {} is not JSON: {e}", meta.secret.scope))?;
    let obj = value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("secretd JSON must be an object"))?;
    // 若声明了 fields，校验齐全（不把字段名以外的东西打进错误细节）。
    for f in &meta.secret.fields {
        if !obj.contains_key(f) {
            anyhow::bail!("secretd JSON missing field `{f}` for scope {}", meta.secret.scope);
        }
    }
    Ok(AccountCreds {
        scope: meta.secret.scope.clone(),
        meta,
        raw_json: Zeroizing::new(raw),
        fields: obj,
    })
}

fn list_accounts(root: &Path) -> anyhow::Result<Vec<AccountMeta>> {
    let mut out = Vec::new();
    for ent in std::fs::read_dir(root)? {
        let ent = ent?;
        if !ent.path().is_dir() {
            continue;
        }
        let p = ent.path().join("account.toml");
        if !p.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&p)?;
        let meta: AccountMeta = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("parse {}: {e}", p.display()))?;
        out.push(meta);
    }
    // 稳定：按 id 排序；第一个当默认。
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// 频道 accounts 根（测试用）。
pub fn accounts_dir(channel_kind: &str) -> PathBuf {
    crate::channels_root().join(channel_kind).join("accounts")
}
