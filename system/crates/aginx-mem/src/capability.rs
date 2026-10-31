//! 能力固化轨（SELF-EVOLUTION P0–P6）：提案契约 + 总闸 + T3/T1/T2→L0/L1/L2/L3。
//!
//! - 总闸：`capability-persist.toml` 的 `enabled`（默认 false；缺文件=关）。
//!   **不**复用 `self_growth_enabled`。`t2_repeat_threshold`（默认 2）管夜巡重复门槛。
//! - 提案件：`<home>/run/capability-proposals/<name>.md`（frontmatter 契约）。
//! - 落盘：L0 knowledge / L1 flow_create / L2 `materialize_l2`（只 mkdir，不注册）/
//!   L3 `capability_register_l3` → 复用 `aginx_server::gateway_registry::register`（满 D17）。
//! - 闸关：propose/accept 均拒绝落盘（只回文说明），满足「零落盘」。
//! - P2：`capability_t1_review` 只读撞车/收工助检（**不需开闸**）；T1 提案须多步。
//! - P3：`capability_t2_scan` 读 hints；闸开才 T2 提案。闸关只报告。
//! - P4：L2 = `workflows/<名>/{AGENTS.md,flows/,knowledge/}`；禁止写 workflows.md /
//!   agent TOML；回执写明不可假定 `agent://`。
//! - P5：L2→L3 须 `admin_approved`/`owner_approved` + 总闸；调用 `gateway_registry::
//!   register`（名册+toml）；`unregister` 可逆摘名册/toml、保留目录（回落 L2）。
//! - P6：`self_growth_enabled` ≠ 本闸；面级可选 `capability-persist.toml` / EVOLUTION.md
//!   `capability_persist_enabled`（须 home 闸先开；self_growth 不暗开）。

use crate::knowledge::{flow_create, knowledge_add_core};
use carrier_types::error::{CarrierError, CarrierResult};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const GATE_FILE: &str = "capability-persist.toml";
const PROPOSALS_DIR: &str = "run/capability-proposals";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tier {
    L0,
    L1,
    L2,
    L3,
}

impl Tier {
    pub fn parse(s: &str) -> CarrierResult<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            other => Err(CarrierError::InvalidInput(format!(
                "tier must be L0|L1|L2|L3, got '{other}'"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    T1,
    T2,
    T3,
}

impl Trigger {
    pub fn parse(s: &str) -> CarrierResult<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "T1" => Ok(Self::T1),
            "T2" => Ok(Self::T2),
            "T3" => Ok(Self::T3),
            other => Err(CarrierError::InvalidInput(format!(
                "trigger must be T1|T2|T3, got '{other}'"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::T1 => "T1",
            Self::T2 => "T2",
            Self::T3 => "T3",
        }
    }
}

/// 从 workspace 向上最多 8 层找 `capability-persist.toml`；找不到则 home=workspace。
pub fn resolve_home_root(workspace: &Path) -> PathBuf {
    let mut cur = workspace.to_path_buf();
    for _ in 0..8 {
        if cur.join(GATE_FILE).is_file() {
            return cur;
        }
        // 启发式：带 knowledge/ + AGENTS.md 的树根也算 home
        if cur.join("AGENTS.md").is_file() && cur.join("knowledge").is_dir() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    workspace.to_path_buf()
}

/// 简易 TOML 行读：`key = value`（# 注释、空行忽略）。
fn gate_toml_value(home_root: &Path, key: &str) -> Option<String> {
    let path = home_root.join(GATE_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            return Some(v.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// 读总闸。缺文件 / 解析失败 → false。`CAPABILITY_PERSIST_ENABLED=1|true` 可强制开（测用）。
pub fn gate_enabled(home_root: &Path) -> bool {
    if let Ok(v) = std::env::var("CAPABILITY_PERSIST_ENABLED") {
        let v = v.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "1" | "true" | "yes" | "on") {
            return true;
        }
        if matches!(v.as_str(), "0" | "false" | "no" | "off") {
            return false;
        }
    }
    match gate_toml_value(home_root, "enabled").as_deref() {
        Some("true") | Some("1") => true,
        _ => false,
    }
}

/// T2 跨日重复门槛。缺省 2；可用 `t2_repeat_threshold` 或环境变量覆盖。

/// 若 `workspace` 落在 `home/workflows/<face>/`，返回该 face 名。
pub fn face_name_under_home(home_root: &Path, workspace: &Path) -> Option<String> {
    let wf = home_root.join("workflows");
    let ws = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    let wf = wf.canonicalize().unwrap_or(wf);
    let rel = ws.strip_prefix(&wf).ok()?;
    let face = rel.components().next()?.as_os_str().to_str()?.to_string();
    if face.is_empty() || face.starts_with('.') {
        None
    } else {
        Some(face)
    }
}

/// 面级闸：无配置 → 继承 true（仅受 home 闸约束）；显式 false → 关；显式 true → 开。
/// 来源：`workflows/<face>/capability-persist.toml` 的 enabled，否则 EVOLUTION.md 的
/// `capability_persist_enabled:`。**从不**读 `self_growth_enabled`。
pub fn face_persist_allowed(home_root: &Path, face: &str) -> bool {
    let face_dir = home_root.join("workflows").join(face);
    let face_toml = face_dir.join(GATE_FILE);
    if face_toml.is_file() {
        return match gate_toml_value(&face_dir, "enabled").as_deref() {
            Some("true") | Some("1") => true,
            Some("false") | Some("0") => false,
            _ => true, // 文件在但无 enabled 行 → 不额外关
        };
    }
    let evo = face_dir.join("EVOLUTION.md");
    if evo.is_file() {
        if let Ok(text) = std::fs::read_to_string(&evo) {
            for line in text.lines() {
                let line = line.split('#').next().unwrap_or("").trim();
                if let Some(rest) = line.strip_prefix("capability_persist_enabled:") {
                    return rest.trim() == "true";
                }
            }
            // EVOLUTION 在但无该键 → 继承（不因 self_growth 关闭/打开）
        }
    }
    true
}

/// 有效固化闸 = home 总闸 ∧ 面级允许。`self_growth_enabled` 不参与。
pub fn effective_persist_enabled(home_root: &Path, workspace: Option<&Path>) -> bool {
    if !gate_enabled(home_root) {
        return false;
    }
    if let Some(ws) = workspace {
        if let Some(face) = face_name_under_home(home_root, ws) {
            return face_persist_allowed(home_root, &face);
        }
    }
    true
}

pub fn t2_repeat_threshold(home_root: &Path) -> usize {
    if let Ok(v) = std::env::var("CAPABILITY_T2_REPEAT_THRESHOLD") {
        if let Ok(n) = v.trim().parse::<usize>() {
            return n.max(1);
        }
    }
    gate_toml_value(home_root, "t2_repeat_threshold")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2)
        .max(1)
}

fn gated_off_msg(home_root: &Path) -> String {
    format!(
        "capability_persist_enabled=false（闸在 {}/{}；默认关）。\
         只记账不落盘。打开：写 enabled=true 或测用 CAPABILITY_PERSIST_ENABLED=1。\
         勿与 self_growth_enabled 混用。",
        home_root.display(),
        GATE_FILE
    )
}

fn parse_string_list(input: &Value, key: &str) -> Vec<String> {
    match &input[key] {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Value::String(s) if !s.trim().is_empty() => s
            .split(|c| c == ',' || c == '\n')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}


fn parse_fm_list(fm: &str, key: &str) -> Vec<String> {
    let mut ts = Vec::new();
    let mut in_list = false;
    let prefix = format!("{key}:");
    for line in fm.lines() {
        let t = line.trim();
        if t.starts_with(&prefix) {
            in_list = true;
            // inline empty: "steps: []"
            if t.ends_with("[]") {
                break;
            }
            continue;
        }
        if in_list {
            if let Some(rest) = t.strip_prefix("- ") {
                ts.push(rest.trim().trim_matches('"').to_string());
            } else if !t.is_empty() && !t.starts_with('-') {
                break;
            }
        }
    }
    ts
}

fn yaml_escape(s: &str) -> String {
    if s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || matches!(c, ':' | '#' | '"' | '\'' | '[' | ']'))
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn sanitize_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if c.is_whitespace() || c == '/' {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "proposal".into()
    } else {
        out
    }
}

/// §五 禁止清单（P1 机械可检子集）。
fn forbid_checks(tier: &Tier, body_or_content: &str, collision: Option<&str>) -> CarrierResult<()> {
    if matches!(tier, Tier::L3) {
        return Err(CarrierError::InvalidInput(
            "拒绝：L3 满 D17 注册属 P5（人审/admin 门）；本阶段最多 L2 共享流程库（不写名册）。"
                .into(),
        ));
    }
    let _ = (tier, collision);
    let lower = body_or_content.to_lowercase();
    for pat in [
        "app_secret",
        "api_key",
        "apikey",
        "secret_key",
        "access_token",
        "private_key",
        "password=",
    ] {
        if lower.contains(pat) {
            return Err(CarrierError::InvalidInput(format!(
                "拒绝：正文含疑似凭据形态 '{pat}'——秘密不进提案/知识/flow（走 secretd/kv）。"
            )));
        }
    }
    Ok(())
}

fn render_proposal_md(
    name: &str,
    tier: &Tier,
    trigger: &Trigger,
    steps: &[String],
    tools: &[String],
    evidence: &str,
    collision: &str,
    status: &str,
    body: &str,
) -> String {
    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str("type: capability-proposal\n");
    fm.push_str(&format!("name: {}\n", yaml_escape(name)));
    fm.push_str(&format!("description: capability proposal {name}\n"));
    fm.push_str(&format!("tier: {}\n", tier.as_str()));
    fm.push_str(&format!("trigger: {}\n", trigger.as_str()));
    fm.push_str(&format!("status: {}\n", status));
    fm.push_str(&format!("evidence: {}\n", yaml_escape(evidence)));
    if collision.is_empty() {
        fm.push_str("collision: null\n");
    } else {
        fm.push_str(&format!("collision: {}\n", yaml_escape(collision)));
    }
    fm.push_str("steps:\n");
    if steps.is_empty() {
        fm.push_str("  []\n");
    } else {
        for s in steps {
            fm.push_str(&format!("  - {}\n", yaml_escape(s)));
        }
    }
    fm.push_str("tools:\n");
    if tools.is_empty() {
        fm.push_str("  []\n");
    } else {
        for t in tools {
            fm.push_str(&format!("  - {}\n", yaml_escape(t)));
        }
    }
    fm.push_str("---\n\n");
    fm.push_str(body.trim());
    fm.push('\n');
    fm
}

fn parse_fm_value<'a>(fm: &'a str, key: &str) -> Option<&'a str> {
    for line in fm.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            let v = rest.trim();
            if v == "null" || v.is_empty() {
                return None;
            }
            if let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return Some(inner);
            }
            return Some(v);
        }
    }
    None
}

fn split_fm(content: &str) -> CarrierResult<(&str, &str)> {
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
        .ok_or_else(|| {
            CarrierError::InvalidInput("proposal missing frontmatter ---".into())
        })?;
    let end = rest.find("\n---").ok_or_else(|| {
        CarrierError::InvalidInput("proposal frontmatter not closed".into())
    })?;
    let fm = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n');
    Ok((fm, body))
}

/// 名册条目：`- `name`: display —— desc`
fn parse_roster_entries(roster: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in roster.lines() {
        let line = line.trim();
        if !line.starts_with("- `") {
            continue;
        }
        let rest = &line[3..];
        let Some(end) = rest.find('`') else { continue };
        let name = rest[..end].trim().to_string();
        if name.is_empty() {
            continue;
        }
        let after = rest[end + 1..].trim().trim_start_matches(':').trim();
        out.push((name, after.to_string()));
    }
    out
}

fn tokenize_query(q: &str) -> Vec<String> {
    let mut out: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.chars().count() >= 2)
        .collect();
    let cjk: Vec<char> = q
        .chars()
        .filter(|c| {
            let u = *c as u32;
            (0x4E00..=0x9FFF).contains(&u)
        })
        .collect();
    for w in cjk.windows(2) {
        out.push(format!("{}{}", w[0], w[1]));
    }
    out.sort();
    out.dedup();
    out
}

fn hay_matches(hay: &str, tokens: &[String], query: &str) -> bool {
    let hay_l = hay.to_ascii_lowercase();
    let q_l = query.to_ascii_lowercase();
    if tokens.iter().any(|t| hay_l.contains(t)) {
        return true;
    }
    let cjk: Vec<char> = hay
        .chars()
        .filter(|c| {
            let u = *c as u32;
            (0x4E00..=0x9FFF).contains(&u)
        })
        .collect();
    cjk.windows(2).any(|w| q_l.contains(&format!("{}{}", w[0], w[1])))
}

/// 撞车检测：workflows.md 行 + workflows/<名>/AGENTS.md 前文。
pub fn find_collisions(home: &Path, query: &str) -> Vec<(String, String)> {
    let tokens = tokenize_query(query);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(String, String)> = Vec::new();

    let roster_path = home.join("workflows.md");
    if let Ok(roster) = std::fs::read_to_string(&roster_path) {
        for (name, desc) in parse_roster_entries(&roster) {
            let hay = format!("{name} {desc}");
            if hay_matches(&hay, &tokens, query)
                || q_contains_name(query, &name)
            {
                hits.push((name, format!("roster: {desc}")));
            }
        }
    }

    let wf_root = home.join("workflows");
    if let Ok(rd) = std::fs::read_dir(&wf_root) {
        for ent in rd.flatten() {
            if !ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = ent.file_name().to_string_lossy().to_string();
            let agents = ent.path().join("AGENTS.md");
            let Ok(text) = std::fs::read_to_string(&agents) else {
                continue;
            };
            let head: String = text.lines().take(40).collect::<Vec<_>>().join("\n");
            let hay = format!("{name} {head}");
            if hay_matches(&hay, &tokens, query) || q_contains_name(query, &name) {
                if !hits.iter().any(|(n, _)| n == &name) {
                    hits.push((name, "workflows/*/AGENTS.md duty match".into()));
                }
            }
        }
    }
    hits
}

fn q_contains_name(query: &str, name: &str) -> bool {
    query.to_ascii_lowercase().contains(&name.to_ascii_lowercase())
}

/// P2 收工助检（只读，**不需开闸**）。
///
/// 入参：`query`（任务摘要/关键词）、可选 `steps`/`tools`、可选 `success`（默认 true）。
/// 出：recommendation = skip | propose_l0 | propose_l1 | flow_update + collisions。
pub async fn capability_t1_review(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_t1_review requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    let query = input["query"]
        .as_str()
        .or_else(|| input["task"].as_str())
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return Err(CarrierError::InvalidInput(
            "capability_t1_review needs query/task (本轮做了什么的摘要)".into(),
        ));
    }
    let steps = parse_string_list(input, "steps");
    let tools = parse_string_list(input, "tools");
    let success = input["success"].as_bool().unwrap_or(true);
    let gate = gate_enabled(&home);
    let collisions = find_collisions(&home, query);

    let multi = steps.len() >= 2 || tools.len() >= 2;
    let (recommendation, reason) = if !success {
        ("skip", "本轮未成功——不提案")
    } else if !multi {
        ("skip", "步骤/工具不足 2——不像可复述多步工作流（T1 门槛）")
    } else if !collisions.is_empty() {
        ("flow_update", "名册或已有能力面撞车——应 flow_update / 补知识，不新开面")
    } else if tools.len() >= 2 || steps.len() >= 3 {
        ("propose_l1", "无名册对口且多步可复述——可 capability_propose trigger=T3|T1 tier=L1（需已有 face 才能 accept）")
    } else {
        ("propose_l0", "无名册对口——可 capability_propose trigger=T1 tier=L0")
    };

    let coll_txt = if collisions.is_empty() {
        "[]".into()
    } else {
        collisions
            .iter()
            .map(|(n, w)| format!("{n} ({w})"))
            .collect::<Vec<_>>()
            .join("; ")
    };

    Ok(format!(
        "t1_review: recommendation={recommendation}\n\
         reason: {reason}\n\
         collisions: {coll_txt}\n\
         multi_step: {multi} (steps={} tools={})\n\
         gate_enabled: {gate}\n\
         home: {}\n\
         next: 若 recommendation 以 propose_ 开头且 gate 开 → capability_propose(trigger=T1)；\
         若 flow_update → 对 collision 面改 flow/知识；闸关则只把本审查记入回话，零落盘。",
        steps.len(),
        tools.len(),
        home.display()
    ))
}

/// T3/任意触发：组提案件落盘（需闸开）。
pub async fn capability_propose(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_propose requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    if !effective_persist_enabled(&home, Some(ws)) {
        return Err(CarrierError::InvalidInput(gated_off_msg(&home)));
    }

    let name = input["name"].as_str().ok_or(CarrierError::InvalidInput(
        "Missing 'name' parameter".into(),
    ))?;
    let tier = Tier::parse(input["tier"].as_str().unwrap_or("L0"))?;
    let trigger = Trigger::parse(input["trigger"].as_str().unwrap_or("T3"))?;
    let evidence = input["evidence"].as_str().unwrap_or("");
    let collision = input["collision"].as_str().unwrap_or("");
    let steps = parse_string_list(input, "steps");
    let tools = parse_string_list(input, "tools");
    let body = input["body"].as_str().unwrap_or("");
    let status = input["status"].as_str().unwrap_or("proposed");

    if !matches!(status, "proposed" | "accepted" | "rejected" | "deferred") {
        return Err(CarrierError::InvalidInput(format!(
            "status must be proposed|accepted|rejected|deferred, got '{status}'"
        )));
    }
    if matches!(tier, Tier::L3) {
        return Err(CarrierError::InvalidInput(
            "拒绝提案 L3：满 D17 注册属 P5（人审/admin）；本阶段最多提案 L2。".into(),
        ));
    }
    forbid_checks(&tier, body, Some(collision))?;
    if tier == Tier::L2 && body.trim().is_empty() && steps.is_empty() {
        return Err(CarrierError::InvalidInput(
            "L2 提案需要 body 或 steps（共享流程库要有可落盘步骤）".into(),
        ));
    }
    if evidence.trim().is_empty() && trigger == Trigger::T3 {
        // T3 可弱证据，但至少要有 body 或 steps
        if body.trim().is_empty() && steps.is_empty() {
            return Err(CarrierError::InvalidInput(
                "T3 提案需要 evidence、body 或 steps 至少一项非空".into(),
            ));
        }
    }
    if trigger == Trigger::T1 {
        if steps.len() < 2 && tools.len() < 2 {
            return Err(CarrierError::InvalidInput(
                "T1 提案需要 steps≥2 或 tools≥2（收工可复述多步）；先 capability_t1_review。".into(),
            ));
        }
        if body.trim().is_empty() && evidence.trim().is_empty() {
            return Err(CarrierError::InvalidInput(
                "T1 提案需要 body 或 evidence（本轮做成了什么）".into(),
            ));
        }
    }
    if trigger == Trigger::T2 {
        if evidence.trim().is_empty() {
            return Err(CarrierError::InvalidInput(
                "T2 提案需要 evidence（跨日重复或主人「下次照这样」线索；先 capability_t2_scan）".into(),
            ));
        }
        if tools.is_empty() && steps.is_empty() && !evidence.contains("cue:") {
            return Err(CarrierError::InvalidInput(
                "T2 提案需要 tools/steps 指纹，或 evidence 含 cue:（主人明示固化）".into(),
            ));
        }
    }

    // P2：未显式给 collision 时，按 name+body+evidence 扫名册/面
    let mut collision = collision.to_string();
    let mut collision_note = String::new();
    if collision.is_empty() {
        let q = format!("{name} {body} {evidence}");
        let hits = find_collisions(&home, &q);
        if !hits.is_empty() {
            collision = hits[0].0.clone();
            collision_note = format!(
                "；auto-collision={}（建议 flow_update 而非新面）",
                hits.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(",")
            );
        }
    }

    let dir = home.join(PROPOSALS_DIR);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| CarrierError::Internal(format!("mkdir proposals: {e}")))?;
    let slug = sanitize_slug(name);
    let path = dir.join(format!("{slug}.md"));
    let md = render_proposal_md(
        name,
        &tier,
        &trigger,
        &steps,
        &tools,
        evidence,
        &collision,
        status,
        body,
    );
    // frontmatter 三关：name/description 非空已保证
    tokio::fs::write(&path, &md)
        .await
        .map_err(|e| CarrierError::Internal(format!("write proposal: {e}")))?;

    Ok(format!(
        "capability proposal written: {} (tier={} trigger={} status={}{})",
        path.display(),
        tier.as_str(),
        trigger.as_str(),
        status,
        collision_note
    ))
}


/// P4：只落 `workflows/<名>/` 目录骨架，**不**写名册 / gateway TOML / 不 register。
/// 回执必须声明不可假定 `agent://`。
pub async fn materialize_l2(
    home: &Path,
    name: &str,
    body: &str,
    steps: &[String],
    tools: &[String],
    description: &str,
) -> CarrierResult<String> {
    forbid_checks(&Tier::L2, body, None)?;
    let slug = sanitize_slug(name);
    if slug.is_empty() {
        return Err(CarrierError::InvalidInput("L2 name 无效".into()));
    }
    let wf_root = home.join("workflows");
    let face = wf_root.join(&slug);
    if face.exists() {
        return Err(CarrierError::InvalidInput(format!(
            "L2 目录已存在：{}——勿覆盖；改流程用 face 上 flow_update，或另起名。",
            face.display()
        )));
    }

    // 名册文件应在（空册也可）；绝不追加本面行
    let roster = home.join("workflows.md");
    let roster_before = if roster.is_file() {
        std::fs::read_to_string(&roster).unwrap_or_default()
    } else {
        let stub = "# workflows 助理能力注册表\n\n<!-- L2 不写名册行；空册亦可 -->\n";
        tokio::fs::write(&roster, stub)
            .await
            .map_err(|e| CarrierError::Internal(format!("ensure workflows.md: {e}")))?;
        stub.to_string()
    };

    tokio::fs::create_dir_all(face.join("flows"))
        .await
        .map_err(|e| CarrierError::Internal(format!("mkdir flows: {e}")))?;
    tokio::fs::create_dir_all(face.join("knowledge"))
        .await
        .map_err(|e| CarrierError::Internal(format!("mkdir knowledge: {e}")))?;

    let desc = if description.trim().is_empty() {
        format!("L2 共享流程库：{name}")
    } else {
        description.trim().to_string()
    };
    let agents = format!(
        "# {name}\n\n\
         > **L2 共享流程库**（SELF-EVOLUTION P4）。**不可假定** `agent://{slug}`——未满 D17 三件。\n\
         > 入站可能落到 system 兜底；人工/规程可派。禁止本面自写 `workflows.md` / gateway TOML。\n\n\
         ## 职责\n\n{desc}\n\n\
         ## 来源\n\ncapability persist L2 · materialize_l2（只 mkdir，不 register）\n"
    );
    tokio::fs::write(face.join("AGENTS.md"), agents)
        .await
        .map_err(|e| CarrierError::Internal(format!("write AGENTS.md: {e}")))?;

    // 主 flow：提案正文 + 步骤
    let mut flow_body = String::new();
    if !steps.is_empty() {
        flow_body.push_str("## 步骤\n\n");
        for (i, s) in steps.iter().enumerate() {
            flow_body.push_str(&format!("{}. {s}\n", i + 1));
        }
        flow_body.push('\n');
    }
    if !body.trim().is_empty() {
        flow_body.push_str(body.trim());
        flow_body.push('\n');
    }
    if flow_body.trim().is_empty() {
        flow_body = format!("# {name}\n\n（L2 占位流程——待补步骤）\n");
    }
    let mut fm = format!("---\nname: {slug}\ndescription: {desc}\n");
    if !tools.is_empty() {
        fm.push_str("tools:\n");
        for t in tools {
            fm.push_str(&format!("  - {t}\n"));
        }
    }
    fm.push_str("---\n\n");
    let flow_path = face.join("flows").join(format!("{slug}.md"));
    tokio::fs::write(&flow_path, format!("{fm}{flow_body}"))
        .await
        .map_err(|e| CarrierError::Internal(format!("write flow: {e}")))?;

    // 名册完整性：内容不得因本函数新增本面行
    let roster_after = tokio::fs::read_to_string(&roster)
        .await
        .unwrap_or_default();
    if roster_after != roster_before {
        // 回滚目录，避免半写
        let _ = tokio::fs::remove_dir_all(&face).await;
        return Err(CarrierError::Internal(
            "L2 纪律违反：workflows.md 在 materialize 期间被改动——已回滚目录".into(),
        ));
    }
    // 双检：名册不得出现本 slug 行
    if roster_after.contains(&format!("`{slug}`")) {
        let _ = tokio::fs::remove_dir_all(&face).await;
        return Err(CarrierError::Internal(format!(
            "L2 纪律违反：名册已含 `{slug}`——L2 禁止写名册"
        )));
    }

    Ok(format!(
        "L2 materialized: {}/\n\
         - wrote AGENTS.md, flows/{slug}.md, knowledge/\n\
         - workflows.md NOT modified; no gateway TOML; no register\n\
         - 不可假定 agent://{slug}（未满 D17；入站可能 system 兜底）",
        face.display()
    ))
}

/// 工具入口：闸开才落 L2 目录（与 accept tier=L2 同腿）。
pub async fn capability_materialize_l2(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_materialize_l2 requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    if !effective_persist_enabled(&home, Some(ws)) {
        return Err(CarrierError::InvalidInput(gated_off_msg(&home)));
    }
    let name = input["name"].as_str().ok_or(CarrierError::InvalidInput(
        "Missing 'name' parameter".into(),
    ))?;
    let body = input["body"].as_str().unwrap_or("");
    let steps = parse_string_list(input, "steps");
    let tools = parse_string_list(input, "tools");
    let description = input["description"].as_str().unwrap_or("");
    if body.trim().is_empty() && steps.is_empty() {
        return Err(CarrierError::InvalidInput(
            "L2 materialize 需要 body 或 steps".into(),
        ));
    }
    let out = materialize_l2(&home, name, body, &steps, &tools, description).await?;
    if let Some(pname) = input["proposal"].as_str() {
        mark_proposal_status(&home, pname.trim_end_matches(".md"), "accepted").await;
    } else {
        mark_proposal_status(&home, name, "accepted").await;
    }
    Ok(out)
}


/// L3 人审/admin 门：必须显式 `admin_approved=true` 或 `owner_approved=true`。
/// （另：`types::is_admin_gated("capability_register_l3")` 供 runtime 桥二次门。）
fn require_l3_admin_approval(input: &Value) -> CarrierResult<()> {
    let approved = matches!(input["admin_approved"].as_bool(), Some(true))
        || matches!(input["owner_approved"].as_bool(), Some(true))
        || matches!(
            input["admin_approved"].as_str().map(|s| s.to_ascii_lowercase()).as_deref(),
            Some("1") | Some("true") | Some("yes")
        )
        || matches!(
            input["owner_approved"].as_str().map(|s| s.to_ascii_lowercase()).as_deref(),
            Some("1") | Some("true") | Some("yes")
        );
    if !approved {
        return Err(CarrierError::InvalidInput(
            "L3 需要显式 admin_approved=true 或 owner_approved=true（人审/admin 门；自动回路不得擅自满 D17）"
                .into(),
        ));
    }
    Ok(())
}

fn kernel_config_for_home(home: &Path) -> carrier_types::config::KernelConfig {
    carrier_types::config::KernelConfig {
        home_dir: home.to_path_buf(),
        workflows_dir: Some(home.join("workflows")),
        ..carrier_types::config::KernelConfig::default()
    }
}

/// 解析网关数据根（与 gateway_registry 同律）：AGINX_DATA_DIR → home/gateway-data → 测用回落 home/.aginx-data
fn resolve_agents_data_root(home: &Path) -> PathBuf {
    if let Ok(v) = std::env::var("AGINX_DATA_DIR") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    let ptr = home.join("gateway-data");
    if let Ok(t) = std::fs::read_link(&ptr) {
        let t = if t.is_absolute() {
            t
        } else {
            home.join(t)
        };
        if t.is_dir() {
            return t;
        }
    }
    if ptr.is_dir() {
        return ptr;
    }
    home.join(".aginx-data")
}

fn verify_d17_receipts(home: &Path, name: &str) -> CarrierResult<String> {
    let roster = std::fs::read_to_string(home.join("workflows.md")).unwrap_or_default();
    let roster_ok = roster.lines().any(|l| l.starts_with(&format!("- `{name}`:")));
    let data = resolve_agents_data_root(home);
    let toml = data.join("agents").join(name).join("aginx.toml");
    let toml_ok = toml.is_file();
    let list_ok = home.join("workflows").join(name).is_dir();
    if !roster_ok || !toml_ok || !list_ok {
        return Err(CarrierError::Internal(format!(
            "D17 三收据不齐：roster={roster_ok} toml={}@{} list={list_ok}",
            toml_ok,
            toml.display()
        )));
    }
    Ok(format!(
        "D17 receipts: roster=ok toml={} list=workflows/{name}",
        toml.display()
    ))
}

/// P5：L2→L3 满 D17。复用 `gateway_registry::register`；须总闸 + admin/owner 批准。
pub async fn capability_register_l3(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_register_l3 requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    if !effective_persist_enabled(&home, Some(ws)) {
        return Err(CarrierError::InvalidInput(gated_off_msg(&home)));
    }
    require_l3_admin_approval(input)?;

    let name = input["name"].as_str().ok_or(CarrierError::InvalidInput(
        "Missing 'name' parameter".into(),
    ))?;
    let slug = sanitize_slug(name);
    let face = home.join("workflows").join(&slug);
    if !face.join("AGENTS.md").is_file() {
        return Err(CarrierError::InvalidInput(format!(
            "L3 需要已有 L2 目录（含 AGENTS.md）：{}——先 capability_materialize_l2 / accept tier=L2",
            face.display()
        )));
    }
    // 秘密扫描：读 AGENTS + 主 flow 正文
    let mut blob = std::fs::read_to_string(face.join("AGENTS.md")).unwrap_or_default();
    if let Ok(rd) = std::fs::read_dir(face.join("flows")) {
        for e in rd.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("md") {
                if let Ok(t) = std::fs::read_to_string(e.path()) {
                    blob.push_str(&t);
                }
            }
        }
    }
    // L3 允许通过 forbid 的 tier 门——只做凭据扫描
    let lower = blob.to_lowercase();
    for pat in [
        "app_secret",
        "api_key",
        "apikey",
        "secret_key",
        "access_token",
        "private_key",
        "password=",
    ] {
        if lower.contains(pat) {
            return Err(CarrierError::InvalidInput(format!(
                "拒绝：能力面正文含疑似凭据 '{pat}'——秘密不进注册面"
            )));
        }
    }

    let display = input["display"]
        .as_str()
        .or_else(|| input["description"].as_str())
        .unwrap_or(name);
    let desc = input["description"].as_str().unwrap_or("L3 能力面（D17）");

    // 确保数据根可写：若无 AGINX_DATA_DIR/指针，建 home/.aginx-data 并指 gateway-data
    let data_root = resolve_agents_data_root(&home);
    tokio::fs::create_dir_all(&data_root)
        .await
        .map_err(|e| CarrierError::Internal(format!("mkdir data root: {e}")))?;
    let ptr = home.join("gateway-data");
    if !ptr.exists() {
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&data_root, &ptr);
        }
    }
    // 对 register 可见：若仍无 env，临时设 AGINX_DATA_DIR（测/裸跑收敛）
    let _env_guard = TempEnvVar::set("AGINX_DATA_DIR", data_root.to_str().unwrap_or(""));

    let cfg = kernel_config_for_home(&home);
    aginx_server::gateway_registry::register(&cfg, &slug, display, desc).map_err(|e| {
        CarrierError::Internal(format!("gateway_registry::register failed: {e}"))
    })?;

    let receipts = verify_d17_receipts(&home, &slug)?;
    mark_proposal_status(&home, name, "accepted").await;
    Ok(format!(
        "L3 registered via gateway_registry::register: workflows/{slug}\n\
         - {receipts}\n\
         - agent://{slug} 可直寻址（须 gateway 重载/重启后名册生效）\n\
         - admin_approved"
    ))
}

/// P5：摘 D17 名册+toml（保留 workflows/<名>/ → 回落 L2）。须总闸 + admin/owner。
pub async fn capability_unregister_l3(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_unregister_l3 requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    if !effective_persist_enabled(&home, Some(ws)) {
        return Err(CarrierError::InvalidInput(gated_off_msg(&home)));
    }
    require_l3_admin_approval(input)?;
    let name = input["name"].as_str().ok_or(CarrierError::InvalidInput(
        "Missing 'name' parameter".into(),
    ))?;
    let slug = sanitize_slug(name);
    let data_root = resolve_agents_data_root(&home);
    let _env_guard = TempEnvVar::set("AGINX_DATA_DIR", data_root.to_str().unwrap_or(""));
    let cfg = kernel_config_for_home(&home);
    aginx_server::gateway_registry::unregister(&cfg, &slug).map_err(|e| {
        CarrierError::Internal(format!("gateway_registry::unregister failed: {e}"))
    })?;
    let face_still = home.join("workflows").join(&slug).is_dir();
    let roster = std::fs::read_to_string(home.join("workflows.md")).unwrap_or_default();
    let roster_gone = !roster.lines().any(|l| l.starts_with(&format!("- `{slug}`:")));
    let toml_gone = !data_root.join("agents").join(&slug).join("aginx.toml").is_file();
    Ok(format!(
        "L3 unregistered: roster_cleared={roster_gone} toml_cleared={toml_gone} face_dir_kept={face_still}（回落 L2）"
    ))
}

/// RAII 临时环境变量（测/裸跑指数据根）；Drop 恢复原值。
struct TempEnvVar {
    key: &'static str,
    old: Option<std::ffi::OsString>,
}
impl TempEnvVar {
    fn set(key: &'static str, val: &str) -> Self {
        let old = std::env::var_os(key);
        std::env::set_var(key, val);
        Self { key, old }
    }
}
impl Drop for TempEnvVar {
    fn drop(&mut self) {
        match &self.old {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// 采纳提案 → L0 knowledge / L1 flow / L2 mkdir-only。读提案文件或直接吃参数。
pub async fn capability_accept(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_accept requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    if !effective_persist_enabled(&home, Some(ws)) {
        return Err(CarrierError::InvalidInput(gated_off_msg(&home)));
    }

    // 优先读已有提案
    let (name, tier, tools, steps, body, collision) = if let Some(p) = input["proposal"].as_str() {
        let path = if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            home.join(PROPOSALS_DIR).join(p)
        };
        let path = if path.exists() {
            path
        } else if !p.ends_with(".md") {
            home.join(PROPOSALS_DIR).join(format!("{p}.md"))
        } else {
            path
        };
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| CarrierError::InvalidInput(format!("read proposal {}: {e}", path.display())))?;
        let (fm, body) = split_fm(&text)?;
        let name = parse_fm_value(fm, "name")
            .ok_or_else(|| CarrierError::InvalidInput("proposal missing name".into()))?
            .to_string();
        let tier = Tier::parse(parse_fm_value(fm, "tier").unwrap_or("L0"))?;
        let tools = parse_fm_list(fm, "tools");
        let steps = parse_fm_list(fm, "steps");
        let collision = parse_fm_value(fm, "collision").unwrap_or("").to_string();
        (name, tier, tools, steps, body.to_string(), collision)
    } else {
        let name = input["name"]
            .as_str()
            .ok_or(CarrierError::InvalidInput(
                "Missing 'name' or 'proposal'".into(),
            ))?
            .to_string();
        let tier = Tier::parse(input["tier"].as_str().unwrap_or("L0"))?;
        let tools = parse_string_list(input, "tools");
        let steps = parse_string_list(input, "steps");
        let body = input["body"].as_str().unwrap_or("").to_string();
        let collision = input["collision"].as_str().unwrap_or("").to_string();
        (name, tier, tools, steps, body, collision)
    };

    if tier != Tier::L3 {
        forbid_checks(&tier, &body, Some(&collision))?;
    }

    match tier {
        Tier::L0 => {
            let title = input["title"].as_str().unwrap_or(&name);
            let content = if body.trim().is_empty() {
                input["content"].as_str().unwrap_or("")
            } else {
                &body
            };
            if content.trim().is_empty() {
                return Err(CarrierError::InvalidInput(
                    "L0 accept needs body/content".into(),
                ));
            }
            let filename = knowledge_add_core(&home, title, content, "capability_accept").await?;
            // 标记提案 accepted（若能定位）
            mark_proposal_status(&home, &name, "accepted").await;
            Ok(format!(
                "L0 persisted: {}/knowledge/{}.md",
                home.display(),
                filename
            ))
        }
        Tier::L1 => {
            // 必须落到已有能力面 workspace
            let face = input["face"].as_str();
            let target = if let Some(f) = face {
                let p = home.join("workflows").join(f);
                if !p.is_dir() {
                    return Err(CarrierError::InvalidInput(format!(
                        "L1 需要已有能力面目录：{} 不存在。撞车面请 flow_update，勿空造 L2。",
                        p.display()
                    )));
                }
                p
            } else if let Some(tw) = input["target_workspace"].as_str() {
                PathBuf::from(tw)
            } else {
                // 若当前 workspace 已是某面（含 flows/ 或在 workflows/ 下）
                if ws.join("flows").exists() || ws.file_name().is_some() {
                    ws.to_path_buf()
                } else {
                    return Err(CarrierError::InvalidInput(
                        "L1 accept 需要 face=<已有workflows名> 或 target_workspace=路径".into(),
                    ));
                }
            };
            if !collision.is_empty() {
                // 有撞车名时优先 update 提示：若调用方显式 update=true 则 flow_update
                if input["update"].as_bool() == Some(true) {
                    let mut m = serde_json::Map::new();
                    m.insert("name".into(), json!(name));
                    m.insert("body".into(), json!(body));
                    if !tools.is_empty() {
                        m.insert("tools".into(), json!(tools));
                    }
                    if let Some(d) = input["description"].as_str() {
                        m.insert("description".into(), json!(d));
                    }
                    let out = crate::knowledge::flow_update(&Value::Object(m), Some(&target)).await?;
                    mark_proposal_status(&home, &name, "accepted").await;
                    return Ok(format!("L1 updated on {}: {out}", target.display()));
                }
            }
            let mut m = serde_json::Map::new();
            m.insert("name".into(), json!(name));
            m.insert("body".into(), json!(body));
            if !tools.is_empty() {
                m.insert("tools".into(), json!(tools));
            }
            if let Some(d) = input["description"].as_str() {
                m.insert("description".into(), json!(d));
            } else {
                m.insert(
                    "description".into(),
                    json!(format!("capability-persist L1: {name}")),
                );
            }
            let out = flow_create(&Value::Object(m), Some(&target)).await?;
            mark_proposal_status(&home, &name, "accepted").await;
            Ok(format!("L1 persisted under {}: {out}", target.display()))
        }
        Tier::L2 => {
            let description = input["description"].as_str().unwrap_or("");
            let out = materialize_l2(&home, &name, &body, &steps, &tools, description).await?;
            mark_proposal_status(&home, &name, "accepted").await;
            Ok(out)
        }
        Tier::L3 => {
            // 升档必须走 register 腿 + admin 门（禁止 accept 绕过）
            let mut m = input.as_object().cloned().unwrap_or_default();
            m.insert("name".into(), json!(name));
            if !m.contains_key("description") && !body.is_empty() {
                m.insert("description".into(), json!(body.chars().take(80).collect::<String>()));
            }
            capability_register_l3(&Value::Object(m), Some(ws)).await
        }
    }
}

async fn mark_proposal_status(home: &Path, name: &str, status: &str) {
    let path = home
        .join(PROPOSALS_DIR)
        .join(format!("{}.md", sanitize_slug(name)));
    if let Ok(text) = tokio::fs::read_to_string(&path).await {
        let updated = text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("status:") {
                    format!("status: {status}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let _ = tokio::fs::write(&path, updated).await;
    }
}

/// P3 T2：扫 `traces/*-capability-hints.md`，跨日 bag 重复 ≥ N 或主人 cue → 候选。
/// - 闸关 / `dry_run=true`：只报告，**零落盘**。
/// - 闸开且有证据：写 `capability_propose(trigger=T2, status=proposed)`；**不** mkdir 能力面。
/// - 无证据不提案。
pub async fn capability_t2_scan(
    input: &Value,
    workspace_root: Option<&Path>,
) -> CarrierResult<String> {
    let ws = workspace_root.ok_or(CarrierError::Internal(
        "capability_t2_scan requires a workspace root".into(),
    ))?;
    let home = resolve_home_root(ws);
    let threshold = input["threshold"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or_else(|| t2_repeat_threshold(&home));
    let dry_run = input["dry_run"].as_bool().unwrap_or(false);
    let lookback_days = input["lookback_days"].as_u64().unwrap_or(14) as usize;

    let traces = home.join("traces");
    let mut hint_files: Vec<PathBuf> = Vec::new();
    if traces.is_dir() {
        let mut ents: Vec<_> = std::fs::read_dir(&traces)
            .map_err(|e| CarrierError::Internal(format!("read traces: {e}")))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with("-capability-hints.md"))
                    .unwrap_or(false)
            })
            .collect();
        ents.sort();
        if lookback_days > 0 && ents.len() > lookback_days {
            ents = ents.split_off(ents.len() - lookback_days);
        }
        hint_files = ents;
    }

    #[derive(Default)]
    struct BagAgg {
        days: std::collections::BTreeSet<String>,
        tools: Vec<String>,
        fingerprint: String,
        sessions: Vec<String>,
    }

    let mut bags: std::collections::BTreeMap<String, BagAgg> = std::collections::BTreeMap::new();
    let mut orphan_cues: Vec<String> = Vec::new();

    for path in &hint_files {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("- cue:") {
                let cue = rest.trim().to_string();
                if cue.is_empty() || cue == "(none)" {
                    continue;
                }
                orphan_cues.push(cue);
            } else if let Some(rest) = line.strip_prefix("- fingerprint:") {
                let mut tool_parts: Vec<&str> = Vec::new();
                let mut bag = String::new();
                let mut day = String::new();
                let mut session = String::new();
                for part in rest.split('|') {
                    let part = part.trim();
                    if let Some(v) = part.strip_prefix("bag:") {
                        bag = v.trim().to_string();
                    } else if let Some(v) = part.strip_prefix("day:") {
                        day = v.trim().to_string();
                    } else if let Some(v) = part.strip_prefix("session:") {
                        session = v.trim().to_string();
                    } else if part.starts_with("tools_n:") {
                        continue;
                    } else if !part.is_empty() {
                        tool_parts.push(part);
                    }
                }
                let fp = tool_parts.join("|");
                if bag.is_empty() {
                    let mut toks: Vec<String> =
                        tool_parts.iter().map(|s| (*s).to_string()).collect();
                    toks.sort();
                    toks.dedup();
                    bag = toks.join("+");
                }
                if bag.is_empty() {
                    continue;
                }
                let entry = bags.entry(bag.clone()).or_default();
                if !day.is_empty() {
                    entry.days.insert(day);
                }
                if entry.fingerprint.is_empty() {
                    entry.fingerprint = fp;
                }
                if entry.tools.is_empty() {
                    entry.tools = tool_parts.iter().map(|s| (*s).to_string()).collect();
                }
                if !session.is_empty() {
                    entry.sessions.push(session);
                }
            }
        }
    }

    let gate_on = effective_persist_enabled(&home, Some(ws));
    let mut report: Vec<String> = Vec::new();
    report.push(format!(
        "t2_scan: hints={} threshold={} gate_enabled={} dry_run={} home={}",
        hint_files.len(),
        threshold,
        gate_on,
        dry_run,
        home.display()
    ));

    let mut proposed = 0usize;
    let mut skipped = 0usize;

    // Collect work items then propose sequentially
    struct Cand {
        name: String,
        evidence: String,
        tools: Vec<String>,
        steps: Vec<String>,
        body: String,
    }
    let mut cands: Vec<Cand> = Vec::new();

    if !orphan_cues.is_empty() {
        let evidence = format!(
            "cue: owner persist phrasing; samples: {}",
            orphan_cues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" || ")
        );
        let body = format!(
            "T2 主人明示固化线索。\n\n{}\n",
            orphan_cues
                .iter()
                .take(8)
                .map(|c| format!("- {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        cands.push(Cand {
            name: "t2-owner-cue".into(),
            evidence,
            tools: vec![],
            steps: vec!["owner-cue".into()],
            body,
        });
    }

    for (bag, agg) in &bags {
        let day_n = agg.days.len();
        if day_n < threshold {
            continue;
        }
        let slug_tools: String = agg
            .tools
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("-");
        let name = if slug_tools.is_empty() {
            format!("t2-{}", sanitize_slug(bag))
        } else {
            format!("t2-{}", sanitize_slug(&slug_tools))
        };
        let days_list: Vec<String> = agg.days.iter().cloned().collect();
        let evidence = format!(
            "bag={} days={} (≥{}); fingerprint={}; sessions={}",
            bag,
            days_list.join(","),
            threshold,
            agg.fingerprint,
            agg.sessions
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        );
        let body = format!(
            "T2 夜巡能力候选：同类临场编排跨日重复。\n\n- bag: `{bag}`\n- days: {}\n- fingerprint: `{}`\n\n不直接 mkdir 能力面；采纳走 capability_accept（≤L1）。\n",
            days_list.join(", "),
            agg.fingerprint
        );
        cands.push(Cand {
            name,
            evidence,
            tools: agg.tools.clone(),
            steps: agg.tools.clone(),
            body,
        });
    }

    let candidates = cands.len();
    for c in cands {
        if !gate_on || dry_run {
            report.push(format!(
                "candidate name={} action=report_only (gate_off_or_dry)",
                c.name
            ));
            skipped += 1;
            continue;
        }
        match capability_propose(
            &json!({
                "name": c.name,
                "tier": "L0",
                "trigger": "T2",
                "status": "proposed",
                "evidence": c.evidence,
                "tools": c.tools,
                "steps": c.steps,
                "body": c.body,
            }),
            Some(ws),
        )
        .await
        {
            Ok(msg) => {
                proposed += 1;
                report.push(format!("proposed {}: {msg}", c.name));
            }
            Err(e) => {
                skipped += 1;
                report.push(format!("propose_fail {}: {e}", c.name));
            }
        }
    }

    if candidates == 0 {
        report.push("candidates: 0（无证据不提案）".into());
    }
    report.push(format!(
        "summary: candidates={candidates} proposed={proposed} report_only_or_fail={skipped}; mkdir_workflows=never"
    ));
    Ok(report.join("\n"))
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    /// AGINX_DATA_DIR 是进程级 env：register/unregister 为给内层
    /// gateway_registry 递根会临时写它（TempEnvVar），两个并行测试各写
    /// 各的必互踩（2026-10-31 实锤 1/3 挂率，check 门随机红）。凡触
    /// materialize/register/unregister/accept-L3 的测试先拿这把锁串行。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_gate(home: &Path, enabled: bool) {
        std::fs::write(
            home.join(GATE_FILE),
            format!("# capability persist gate\nenabled = {}\n", enabled),
        )
        .unwrap();
        std::fs::create_dir_all(home.join("knowledge")).unwrap();
        std::fs::write(home.join("AGENTS.md"), "# test\n").unwrap();
    }

    #[tokio::test]
    async fn gate_off_propose_writes_nothing() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, false);
        // clear env override
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let err = capability_propose(
            &json!({
                "name": "demo",
                "tier": "L0",
                "trigger": "T3",
                "body": "remember this",
                "evidence": "user said 固定下来"
            }),
            Some(home),
        )
        .await
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("capability_persist_enabled=false"), "{msg}");
        assert!(!home.join(PROPOSALS_DIR).exists() || home.join(PROPOSALS_DIR).read_dir().unwrap().count() == 0);
    }

    #[tokio::test]
    async fn t3_propose_then_accept_l0() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");

        let out = capability_propose(
            &json!({
                "name": "fixed-down-demo",
                "tier": "L0",
                "trigger": "T3",
                "steps": ["ask", "save"],
                "tools": ["knowledge_add"],
                "evidence": "主人：固定下来",
                "body": "把这套说法收成知识。"
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("capability proposal written"), "{out}");
        let prop = home.join(PROPOSALS_DIR).join("fixed-down-demo.md");
        assert!(prop.is_file());
        let text = std::fs::read_to_string(&prop).unwrap();
        assert!(text.contains("type: capability-proposal"));
        assert!(text.contains("tier: L0"));
        assert!(text.contains("description:"));

        let out = capability_accept(
            &json!({"proposal": "fixed-down-demo.md"}),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("L0 persisted"), "{out}");
        assert!(home.join("knowledge").join("fixed-down-demo.md").is_file()
            || std::fs::read_dir(home.join("knowledge")).unwrap().any(|e| {
                e.unwrap().path().extension().map(|x| x == "md").unwrap_or(false)
            }));
    }

    #[tokio::test]
    async fn t3_accept_l1_needs_existing_face() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let face = home.join("workflows").join("weather");
        std::fs::create_dir_all(face.join("flows")).unwrap();
        std::fs::write(face.join("AGENTS.md"), "# weather\n").unwrap();

        capability_propose(
            &json!({
                "name": "rain-check",
                "tier": "L1",
                "trigger": "T3",
                "tools": ["web_fetch"],
                "evidence": "做成天气那种",
                "body": "1. fetch\n2. summarize"
            }),
            Some(home),
        )
        .await
        .unwrap();

        let err = capability_accept(
            &json!({"proposal": "rain-check", "face": "missing"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("不存在"), "{err}");

        let out = capability_accept(
            &json!({"proposal": "rain-check", "face": "weather"}),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("L1 persisted"), "{out}");
        assert!(face.join("flows").join("rain-check.md").is_file());
    }

    #[tokio::test]
    async fn reject_l3_and_secrets() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let err = capability_propose(
            &json!({"name": "x", "tier": "L3", "trigger": "T3", "body": "nope", "evidence": "e"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("L3") || format!("{err}").contains("P5"), "{err}");

        let err = capability_propose(
            &json!({
                "name": "x",
                "tier": "L0",
                "trigger": "T3",
                "body": "api_key=sk-secret",
                "evidence": "e"
            }),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("凭据"), "{err}");
    }

    #[test]
    fn missing_gate_file_defaults_off() {
        let tmp = tempdir().unwrap();
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        assert!(!gate_enabled(tmp.path()));
    }

    #[tokio::test]
    async fn t1_review_gate_independent_and_collision() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, false); // 闸关仍可 review
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        std::fs::write(
            home.join("workflows.md"),
            "# workflows\n\n- `weather`: 天气 —— 查上海天气与出行\n",
        )
        .unwrap();
        let face = home.join("workflows").join("weather");
        std::fs::create_dir_all(&face).unwrap();
        std::fs::write(face.join("AGENTS.md"), "# 天气面\n职责：查天气\n").unwrap();

        let out = capability_t1_review(
            &json!({
                "query": "查一下上海天气",
                "steps": ["search", "summarize"],
                "tools": ["web_search", "web_fetch"],
                "success": true
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("recommendation=flow_update"), "{out}");
        assert!(out.contains("weather"), "{out}");
        assert!(out.contains("gate_enabled: false"), "{out}");
    }

    #[tokio::test]
    async fn t1_review_no_collision_propose_l0() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, false);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        std::fs::write(home.join("workflows.md"), "# workflows\n\n").unwrap();

        let out = capability_t1_review(
            &json!({
                "query": "整理一份罕见矿石对照表",
                "steps": ["collect", "normalize"],
                "tools": ["file_write"],
                "success": true
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("recommendation=propose_l0"), "{out}");
    }

    #[tokio::test]
    async fn t1_propose_requires_multistep_and_autofills_collision() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        std::fs::write(
            home.join("workflows.md"),
            "- `weather`: 天气 —— 查天气\n",
        )
        .unwrap();

        let err = capability_propose(
            &json!({
                "name": "one-shot",
                "tier": "L0",
                "trigger": "T1",
                "steps": ["only-one"],
                "body": "x",
                "evidence": "done"
            }),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("steps≥2"), "{err}");

        let out = capability_propose(
            &json!({
                "name": "weather-extra",
                "tier": "L0",
                "trigger": "T1",
                "steps": ["a", "b"],
                "body": "补充查天气步骤",
                "evidence": "本轮做成了查天气"
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("auto-collision=weather"), "{out}");
        let prop = std::fs::read_to_string(home.join(PROPOSALS_DIR).join("weather-extra.md")).unwrap();
        assert!(prop.contains("collision: weather"), "{prop}");
        assert!(prop.contains("trigger: T1"));
    }

    fn write_hint(home: &Path, day: &str, bag: &str, tools: &str) {
        let traces = home.join("traces");
        std::fs::create_dir_all(&traces).unwrap();
        let body = format!(
            "# capability-hints {day}\n\n## cues\n- (none)\n\n## fingerprints\n\
             - fingerprint: {tools} | bag: {bag} | day: {day} | session: s-{day}.jsonl | tools_n: 2\n"
        );
        std::fs::write(traces.join(format!("{day}-capability-hints.md")), body).unwrap();
    }

    #[tokio::test]
    async fn t2_scan_gate_off_report_only() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, false);
        write_hint(home, "2026-10-28", "web_fetch+web_search", "web_search|web_fetch");
        write_hint(home, "2026-10-29", "web_fetch+web_search", "web_search|web_fetch");
        let out = capability_t2_scan(&json!({}), Some(home)).await.unwrap();
        assert!(out.contains("gate_enabled=false"), "{out}");
        assert!(out.contains("report_only"), "{out}");
        assert!(out.contains("candidates=1"), "{out}");
        let prop = home.join(PROPOSALS_DIR);
        let n = if prop.is_dir() {
            std::fs::read_dir(&prop).unwrap().count()
        } else {
            0
        };
        assert_eq!(n, 0, "gate off must not write proposals");
    }

    #[tokio::test]
    async fn t2_scan_two_days_proposes_when_gate_on() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        std::fs::write(
            home.join(GATE_FILE),
            "enabled = true\nt2_repeat_threshold = 2\n",
        )
        .unwrap();
        write_hint(home, "2026-10-28", "web_fetch+web_search", "web_search|web_fetch");
        write_hint(home, "2026-10-29", "web_fetch+web_search", "web_search|web_fetch");
        let out = capability_t2_scan(&json!({}), Some(home)).await.unwrap();
        assert!(out.contains("proposed"), "{out}");
        assert!(out.contains("mkdir_workflows=never"), "{out}");
        let prop = home.join(PROPOSALS_DIR);
        let names: Vec<_> = std::fs::read_dir(&prop)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(!names.is_empty(), "{names:?} out={out}");
        assert!(names.iter().any(|n| n.starts_with("t2-")), "{names:?}");
        let text = std::fs::read_to_string(prop.join(&names[0])).unwrap();
        assert!(text.contains("trigger: T2"), "{text}");
        assert!(text.contains("status: proposed"), "{text}");
        let wf = home.join("workflows");
        let wn = if wf.is_dir() {
            std::fs::read_dir(&wf).unwrap().count()
        } else {
            0
        };
        assert_eq!(wn, 0, "must not mkdir capability faces");
    }

    #[tokio::test]
    async fn t2_scan_below_threshold_no_candidate() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        std::fs::write(
            home.join(GATE_FILE),
            "enabled = true\nt2_repeat_threshold = 3\n",
        )
        .unwrap();
        write_hint(home, "2026-10-28", "a+b", "a|b");
        write_hint(home, "2026-10-29", "a+b", "a|b");
        let out = capability_t2_scan(&json!({}), Some(home)).await.unwrap();
        assert!(
            out.contains("candidates: 0") || out.contains("candidates=0"),
            "{out}"
        );
        assert!(!out.contains("proposed t2-"), "{out}");
    }

    #[tokio::test]
    async fn t2_scan_owner_cue_candidate() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        let traces = home.join("traces");
        std::fs::create_dir_all(&traces).unwrap();
        std::fs::write(
            traces.join("2026-10-30-capability-hints.md"),
            "# capability-hints 2026-10-30\n\n## cues\n- cue: 下次照这样查天气 | session: s.jsonl\n\n## fingerprints\n- (none)\n",
        )
        .unwrap();
        let out = capability_t2_scan(&json!({}), Some(home)).await.unwrap();
        assert!(out.contains("t2-owner-cue"), "{out}");
        let text =
            std::fs::read_to_string(home.join(PROPOSALS_DIR).join("t2-owner-cue.md")).unwrap();
        assert!(text.contains("trigger: T2"), "{text}");
        assert!(text.contains("cue:"), "{text}");
    }

    #[tokio::test]
    async fn t2_threshold_from_toml() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        std::fs::write(
            home.join(GATE_FILE),
            "enabled = false\nt2_repeat_threshold = 5\n",
        )
        .unwrap();
        assert_eq!(t2_repeat_threshold(home), 5);
        assert!(!gate_enabled(home));
    }

    #[tokio::test]
    async fn l2_propose_accept_mkdir_only_no_roster() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        // pre-seed roster — must stay byte-identical
        let roster_body = "# workflows\n\n- `weather`: 天气 —— 查天气\n";
        std::fs::write(home.join("workflows.md"), roster_body).unwrap();

        let out = capability_propose(
            &json!({
                "name": "ore-lookup",
                "tier": "L2",
                "trigger": "T3",
                "steps": ["search", "compare", "note"],
                "tools": ["web_search", "file_write"],
                "body": "罕见矿石对照流程",
                "evidence": "主人点名做成共享库"
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("capability proposal written"), "{out}");

        let out = capability_accept(
            &json!({"proposal": "ore-lookup"}),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("L2 materialized"), "{out}");
        assert!(out.contains("不可假定 agent://ore-lookup"), "{out}");
        assert!(out.contains("workflows.md NOT modified"), "{out}");

        let face = home.join("workflows/ore-lookup");
        assert!(face.join("AGENTS.md").is_file());
        assert!(face.join("flows/ore-lookup.md").is_file());
        assert!(face.join("knowledge").is_dir());
        let agents = std::fs::read_to_string(face.join("AGENTS.md")).unwrap();
        assert!(agents.contains("不可假定"), "{agents}");
        assert!(agents.contains("agent://ore-lookup"), "{agents}");

        let roster_after = std::fs::read_to_string(home.join("workflows.md")).unwrap();
        assert_eq!(roster_after, roster_body, "名册不得被 L2 改写");
        assert!(!roster_after.contains("`ore-lookup`"));
        // no gateway toml
        assert!(!home.join("agents").exists() || std::fs::read_dir(home.join("agents")).map(|d| d.count()).unwrap_or(0)==0);
    }

    #[tokio::test]
    async fn l2_materialize_gate_off_and_no_overwrite() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, false);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let err = capability_materialize_l2(
            &json!({"name": "x", "steps": ["a", "b"], "body": "b"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("capability_persist_enabled=false"), "{err}");

        write_gate(home, true);
        std::fs::create_dir_all(home.join("workflows/x")).unwrap();
        let err = capability_materialize_l2(
            &json!({"name": "x", "steps": ["a", "b"], "body": "b"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("已存在"), "{err}");
    }

    #[tokio::test]
    async fn l2_propose_requires_body_or_steps() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let err = capability_propose(
            &json!({"name": "empty", "tier": "L2", "trigger": "T3", "evidence": "e"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("body 或 steps"), "{err}");
    }

    #[tokio::test]
    async fn l3_register_requires_admin_and_gate() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        // L2 face
        materialize_l2(
            home,
            "ore-lookup",
            "矿石流程",
            &["a".into(), "b".into()],
            &["web_search".into()],
            "查矿",
        )
        .await
        .unwrap();

        let err = capability_register_l3(
            &json!({"name": "ore-lookup"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("admin_approved"), "{err}");

        write_gate(home, false);
        let err = capability_register_l3(
            &json!({"name": "ore-lookup", "admin_approved": true}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("capability_persist_enabled=false"), "{err}");
    }

    #[tokio::test]
    async fn l3_register_unregister_d17_roundtrip() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let data = home.join("gw-data");
        std::fs::create_dir_all(&data).unwrap();
        std::env::set_var("AGINX_DATA_DIR", &data);

        materialize_l2(
            home,
            "ore-lookup",
            "矿石流程",
            &["search".into(), "note".into()],
            &["web_search".into()],
            "查矿",
        )
        .await
        .unwrap();
        // L2 must not have roster line
        let roster0 = std::fs::read_to_string(home.join("workflows.md")).unwrap_or_default();
        assert!(!roster0.contains("`ore-lookup`"), "{roster0}");

        let out = capability_register_l3(
            &json!({
                "name": "ore-lookup",
                "display": "矿石对照",
                "description": "查罕见矿石",
                "admin_approved": true
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("L3 registered"), "{out}");
        assert!(out.contains("D17 receipts"), "{out}");

        let roster = std::fs::read_to_string(home.join("workflows.md")).unwrap();
        assert!(roster.contains("`ore-lookup`"), "{roster}");
        let toml = data.join("agents/ore-lookup/aginx.toml");
        assert!(toml.is_file(), "missing {}", toml.display());
        assert!(home.join("workflows/ore-lookup").is_dir());

        let out = capability_unregister_l3(
            &json!({"name": "ore-lookup", "owner_approved": true}),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("roster_cleared=true"), "{out}");
        assert!(out.contains("toml_cleared=true"), "{out}");
        assert!(out.contains("face_dir_kept=true"), "{out}");
        let roster2 = std::fs::read_to_string(home.join("workflows.md")).unwrap();
        assert!(!roster2.contains("`ore-lookup`"), "{roster2}");
        assert!(home.join("workflows/ore-lookup/AGENTS.md").is_file());
        assert!(!toml.is_file());

        std::env::remove_var("AGINX_DATA_DIR");
    }

    #[tokio::test]
    async fn l3_accept_needs_approval() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let data = home.join("gw-data");
        std::fs::create_dir_all(&data).unwrap();
        std::env::set_var("AGINX_DATA_DIR", &data);
        materialize_l2(
            home,
            "demo-face",
            "body",
            &["s1".into(), "s2".into()],
            &[],
            "demo",
        )
        .await
        .unwrap();
        let err = capability_accept(
            &json!({"name": "demo-face", "tier": "L3", "body": "x"}),
            Some(home),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("admin_approved"), "{err}");

        let out = capability_accept(
            &json!({
                "name": "demo-face",
                "tier": "L3",
                "body": "x",
                "owner_approved": true,
                "description": "demo L3"
            }),
            Some(home),
        )
        .await
        .unwrap();
        assert!(out.contains("L3 registered"), "{out}");
        std::env::remove_var("AGINX_DATA_DIR");
    }

    #[tokio::test]
    async fn p6_self_growth_does_not_open_capability_gate() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        // home gate OFF
        write_gate(home, false);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        // face with self_growth on — must not open persist
        let face = home.join("workflows/demo");
        std::fs::create_dir_all(&face).unwrap();
        std::fs::write(
            face.join("EVOLUTION.md"),
            "---\nself_growth_enabled: true\n---\n",
        )
        .unwrap();
        assert!(!gate_enabled(home));
        assert!(!effective_persist_enabled(home, Some(&face)));
        let err = capability_propose(
            &json!({"name": "x", "tier": "L0", "trigger": "T3", "body": "b", "evidence": "e"}),
            Some(&face),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("capability_persist_enabled=false"), "{err}");
        assert!(format!("{err}").contains("self_growth"), "{err}");
    }

    #[tokio::test]
    async fn p6_face_level_can_close_when_home_open() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, true);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let face = home.join("workflows/demo");
        std::fs::create_dir_all(&face).unwrap();
        std::fs::write(face.join("capability-persist.toml"), "enabled = false\n").unwrap();
        assert!(gate_enabled(home));
        assert!(!effective_persist_enabled(home, Some(&face)));
        assert!(!face_persist_allowed(home, "demo"));
    }

    #[tokio::test]
    async fn p6_face_evolution_opt_in_still_needs_home_gate() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        write_gate(home, false);
        std::env::remove_var("CAPABILITY_PERSIST_ENABLED");
        let face = home.join("workflows/demo");
        std::fs::create_dir_all(&face).unwrap();
        std::fs::write(
            face.join("EVOLUTION.md"),
            "---\ncapability_persist_enabled: true\nself_growth_enabled: true\n---\n",
        )
        .unwrap();
        // face allows, but home off → effective off
        assert!(face_persist_allowed(home, "demo"));
        assert!(!effective_persist_enabled(home, Some(&face)));
    }

    #[test]
    fn p6_clone_lifecycle_defaults_persist_off() {
        let c = carrier_types::config::CloneLifecycleConfig::default();
        assert!(!c.self_growth_enabled);
        assert!(!c.capability_persist_enabled);
        // independence: turning self_growth on in a struct literal must not
        // imply persist (field is separate).
        let c2 = carrier_types::config::CloneLifecycleConfig {
            self_growth_enabled: true,
            ..Default::default()
        };
        assert!(c2.self_growth_enabled);
        assert!(!c2.capability_persist_enabled);
    }
}
