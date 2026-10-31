//! 频道 `channel.toml` 策略：draft_only / publish_admin_gate 联检。

use serde::Deserialize;
use std::path::Path;

use crate::channels_root;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ChannelPolicy {
    /// 默认 true 语义由频道声明；缺省=false（未立法则不开放发布后门）。
    #[serde(default)]
    pub draft_only: bool,
    /// 人审发布门：admin=true 工具须 `--admin` **且** 本开关为 true。
    #[serde(default)]
    pub publish_admin_gate: bool,
}

#[derive(Debug, Deserialize)]
struct ChannelFile {
    #[serde(default)]
    policy: ChannelPolicy,
}

/// 读 `channels/<kind>/channel.toml` 的 `[policy]`；缺文件/缺段 → 默认全 false。
pub fn load_channel_policy(channel_kind: &str) -> anyhow::Result<ChannelPolicy> {
    let path = channels_root().join(channel_kind).join("channel.toml");
    load_channel_policy_at(&path)
}

pub fn load_channel_policy_at(path: &Path) -> anyhow::Result<ChannelPolicy> {
    if !path.is_file() {
        return Ok(ChannelPolicy::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let file: ChannelFile = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
    Ok(file.policy)
}

/// admin 工具联检：无 `--admin` 拒；有 `--admin` 但频道未开 `publish_admin_gate` 亦拒。
pub fn check_admin_gate(
    tool_name: &str,
    tool_admin: bool,
    opts_admin: bool,
    policy: &ChannelPolicy,
) -> Result<(), String> {
    if !tool_admin {
        return Ok(());
    }
    if !opts_admin {
        return Err(format!(
            "tool `{tool_name}` is admin-gated; pass --admin after human review \
             (channel publish_admin_gate={}, draft_only={})",
            policy.publish_admin_gate, policy.draft_only
        ));
    }
    if !policy.publish_admin_gate {
        return Err(format!(
            "tool `{tool_name}`: --admin set but channel has publish_admin_gate=false \
             (or missing); refuse publish path. Set [policy] publish_admin_gate=true \
             in channel.toml to open the human-review gate (draft_only={})",
            policy.draft_only
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_weixin_policy() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("channel.toml");
        fs::write(
            &p,
            r#"
type = "weixin-oa"
[policy]
draft_only = true
publish_admin_gate = true
"#,
        )
        .unwrap();
        let pol = load_channel_policy_at(&p).unwrap();
        assert!(pol.draft_only);
        assert!(pol.publish_admin_gate);
    }

    #[test]
    fn missing_file_defaults_closed() {
        let dir = tempfile::tempdir().unwrap();
        let pol = load_channel_policy_at(&dir.path().join("nope.toml")).unwrap();
        assert!(!pol.publish_admin_gate);
        assert!(!pol.draft_only);
    }

    #[test]
    fn gate_matrix() {
        let open = ChannelPolicy {
            draft_only: true,
            publish_admin_gate: true,
        };
        let closed = ChannelPolicy::default();
        assert!(check_admin_gate("t", false, false, &open).is_ok());
        assert!(check_admin_gate("t", true, false, &open).is_err());
        assert!(check_admin_gate("t", true, true, &open).is_ok());
        assert!(check_admin_gate("t", true, true, &closed).is_err());
        let e = check_admin_gate("weixin_mp_freepublish_submit", true, false, &open).unwrap_err();
        assert!(e.contains("publish_admin_gate=true"));
        let e2 = check_admin_gate("weixin_mp_freepublish_submit", true, true, &closed).unwrap_err();
        assert!(e2.contains("publish_admin_gate=false"));
    }
}
