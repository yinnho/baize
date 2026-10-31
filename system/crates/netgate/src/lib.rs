//! aginx-netgate — 监听盘点门（NETWORK-SURFACE v2 P0）。
//!
//! 不变量（写成常驻断言，不是一次性扫描）：**清单外多一个 TCP 监听口
//! 即红**。L0 的缩面是构造性的（镜像 svc.d 只两单元），本门把它变成
//! 每晚可验收的账：谁在听、听在哪个面、清单准不准。
//!
//! 数据源是 /proc/net/tcp 与 /proc/net/tcp6 的原始文本（纯函数解析，
//! 机器无关可测）；允许清单三源并入：
//!
//! 1. `/etc/aginx/listeners.allow` — 平台根正本（缺省路径，可覆盖）；
//! 2. `listeners.allow.d/*.allow` — 正本同目录 drop-in，按名序；
//! 3. `/var/lib/aginx/pkgfiles/*/netgate.allow` — **包自表**：开监听的
//!    包在自己的 files 树里带一枚 netgate.allow 声明自己的口，装上即
//!    并入，零装配零改表。
//!
//! 条目语法：`port@addr`，`#` 后是注释。addr 三种通配 `*` / `0.0.0.0`
//! / `::` = 该口不限面放行；具体 IP = 只许绑那个面（绑 any 或别的面
//! 都是红——清单即策略，面比清单宽要红，比清单窄也要红）。
//!
//! fail-closed：有监听而三源一份都没有 = 全员 offender；坏行直接报错
//! 拒判（不静默跳过——被写坏的清单本身就是红）。

use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

pub const DEFAULT_PROC_DIR: &str = "/proc/net";
pub const DEFAULT_ALLOW: &str = "/etc/aginx/listeners.allow";
pub const DEFAULT_PKGFILES: &str = "/var/lib/aginx/pkgfiles";

/// 盘点出的一个 TCP 监听（/proc LISTEN 行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    /// 绑定的地址（any 绑定时为 0.0.0.0 / :: 本身）。
    pub addr: IpAddr,
    /// 全接口绑定（0.0.0.0 / ::）——比具体绑定宽的面。
    pub any: bool,
    pub v6: bool,
    /// /proc 行的 inode（人在机上 `ls -l /proc/<pid>/fd` 对账用）。
    pub inode: String,
}

/// 允许清单一条：`port@addr`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub port: u16,
    pub addr: RuleAddr,
    /// 来源（文件:行），红时对账用。
    pub src: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleAddr {
    /// `*` / `0.0.0.0` / `::`：该口放行，不限面。
    Any,
    /// 只许绑这个面。
    Exact(IpAddr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offender {
    pub listener: Listener,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verdict {
    pub offenders: Vec<Offender>,
    /// 清单写了、机上却没人听的口（`--require` 时才算红——服务歇着
    /// 不是安全回归，但收紧后要能证明口真的起来了）。
    pub missing: Vec<Rule>,
}

impl Verdict {
    pub fn red(&self, require: bool) -> bool {
        !self.offenders.is_empty() || (require && !self.missing.is_empty())
    }
}

/// 三源解析后的清单来源集合（测试直填路径，不碰 env）。
#[derive(Debug, Clone)]
pub struct RuleSources {
    pub base: PathBuf,
    pub dropin: PathBuf,
    pub pkgfiles: PathBuf,
}

impl RuleSources {
    pub fn resolve() -> RuleSources {
        let base = std::env::var_os("AGINX_NETGATE_ALLOW")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ALLOW));
        // drop-in 恒从正本推导（正本可覆盖，drop-in 跟着走）。
        let dropin = base.with_extension("allow.d");
        let pkgfiles = std::env::var_os("AGINX_NETGATE_PKGFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PKGFILES));
        RuleSources { base, dropin, pkgfiles }
    }
}

// ── /proc 解析 ────────────────────────────────────────────────────────

/// 解析一份 /proc/net/tcp{,6} 文本，只留 LISTEN（st=0A）行。
/// 坏行（列数不足/坏 hex）= Err——盘点腿自己坏了必须喊，不能装绿。
pub fn parse_proc(text: &str, v6: bool) -> Result<Vec<Listener>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // 表头 / 空行
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            return Err(format!("{} 行：列数不足", i + 1));
        }
        if f[3] != "0A" {
            continue;
        }
        let (addr, port, any) = parse_local(f[1], v6)
            .map_err(|e| format!("{} 行：{e}", i + 1))?;
        out.push(Listener { port, addr, any, v6, inode: f[9].to_string() });
    }
    Ok(out)
}

/// `local_address` 形如 `0100007F:1F90`：端口 4 位 hex 大端；地址按
/// 族别解析（v4 全串一组、v6 八组，**每组内部小端**——/proc 的老坑）。
fn parse_local(s: &str, v6: bool) -> Result<(IpAddr, u16, bool), String> {
    let (haddr, hport) = s
        .split_once(':')
        .ok_or_else(|| format!("坏 local_address「{s}」"))?;
    if hport.len() != 4 || !hport.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("坏端口 hex「{hport}」"));
    }
    let port = u16::from_str_radix(hport, 16).map_err(|e| format!("坏端口：{e}"))?;
    let ip = if v6 { parse_v6(haddr)? } else { parse_v4(haddr)? };
    let any = ip.is_unspecified();
    Ok((ip, port, any))
}

/// hex（每字节两字符）按小端还原进 out。/proc 的每个 32-bit 组内部
/// 小端：v4 全串即一组（`0100007F` → 127.0.0.1）；v6 四组、组间网络序
/// （`…01000000` → ::1）。
fn hex_le_group(h: &str, out: &mut [u8]) -> Result<(), String> {
    if h.len() != out.len() * 2 {
        return Err(format!("坏地址 hex 长度：{}（要 {} 位）", h.len(), out.len() * 2));
    }
    let mut raw = vec![0u8; out.len()];
    for (i, slot) in raw.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16)
            .map_err(|e| format!("坏地址 hex「{h}」：{e}"))?;
    }
    for i in 0..out.len() {
        out[i] = raw[out.len() - 1 - i];
    }
    Ok(())
}

fn parse_v4(h: &str) -> Result<IpAddr, String> {
    let mut b = [0u8; 4];
    hex_le_group(h, &mut b)?;
    Ok(IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3])))
}

fn parse_v6(h: &str) -> Result<IpAddr, String> {
    if h.len() != 32 {
        return Err(format!("坏 v6 hex 长度：{}（要 32 位）", h.len()));
    }
    let mut b = [0u8; 16];
    for g in 0..4 {
        hex_le_group(&h[g * 8..g * 8 + 8], &mut b[g * 4..g * 4 + 4])?;
    }
    Ok(IpAddr::V6(Ipv6Addr::from(b)))
}

// ── 清单解析与装配 ────────────────────────────────────────────────────

/// 一行清单：`22@*` / `8089@127.0.0.1`；`#` 起注释；空/纯注释行 → None。
pub fn parse_rule_line(line: &str, src: &str) -> Result<Option<Rule>, String> {
    let body = line.split('#').next().unwrap_or("").trim();
    if body.is_empty() {
        return Ok(None);
    }
    let (p, a) = body
        .split_once('@')
        .ok_or_else(|| format!("{src}：「{body}」缺 @（形如 22@* 或 8089@127.0.0.1）"))?;
    let port: u16 = p
        .trim()
        .parse()
        .map_err(|_| format!("{src}：坏端口「{}」", p.trim()))?;
    let a = a.trim();
    let addr = match a {
        "*" | "0.0.0.0" | "::" => RuleAddr::Any,
        s => RuleAddr::Exact(
            s.parse().map_err(|_| format!("{src}：坏地址「{s}」"))?,
        ),
    };
    Ok(Some(Rule { port, addr, src: src.to_string() }))
}

/// 装配三源。正本/drop-in 目录/包自表目录**缺省都合法**（可以只有
/// 包自表）；存在的源里出现坏行 = Err（fail-closed：坏清单拒判）。
/// 重复条目（同口同面）去重保首。
pub fn load_rules(src: &RuleSources) -> Result<Vec<Rule>, String> {
    let mut rules: Vec<Rule> = Vec::new();
    let mut read_file = |p: &Path, rules: &mut Vec<Rule>| -> Result<(), String> {
        let text = fs::read_to_string(p).map_err(|e| format!("{}：{e}", p.display()))?;
        for (i, line) in text.lines().enumerate() {
            let at = format!("{}:{}", p.display(), i + 1);
            if let Some(r) = parse_rule_line(line, &at)? {
                if !rules.iter().any(|x| x.port == r.port && x.addr == r.addr) {
                    rules.push(r);
                }
            }
        }
        Ok(())
    };

    if src.base.is_file() {
        read_file(&src.base, &mut rules)?;
    }
    if let Ok(entries) = fs::read_dir(&src.dropin) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "allow").unwrap_or(false))
            .collect();
        files.sort();
        for f in files {
            read_file(&f, &mut rules)?;
        }
    }
    if let Ok(entries) = fs::read_dir(&src.pkgfiles) {
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            let decl = d.join("netgate.allow");
            if decl.is_file() {
                read_file(&decl, &mut rules)?;
            }
        }
    }
    Ok(rules)
}

// ── 判定 ─────────────────────────────────────────────────────────────

/// 规则是否放行该监听：Any=口放行不限面；Exact=必须绑那个面
/// （any 绑定算更宽的面，不放行）。
pub fn rule_allows(r: &Rule, l: &Listener) -> bool {
    if r.port != l.port {
        return false;
    }
    match r.addr {
        RuleAddr::Any => true,
        RuleAddr::Exact(ip) => !l.any && l.addr == ip,
    }
}

/// 盘点 vs 清单 → 判定。offender 每条带一句人能直接动手的原因。
pub fn judge(listeners: &[Listener], rules: &[Rule]) -> Verdict {
    let mut offenders = Vec::new();
    for l in listeners {
        if rules.iter().any(|r| rule_allows(r, l)) {
            continue;
        }
        let reason = match rules.iter().find(|r| r.port == l.port) {
            Some(r) => match r.addr {
                RuleAddr::Exact(ip) => {
                    let face = if l.any { "any（全接口）".to_string() } else { l.addr.to_string() };
                    format!("端口 {} 绑 {}，清单只许 {}", l.port, face, ip)
                }
                RuleAddr::Any => format!("端口 {} 绑 {}，清单按 Any 应放行（不一致=盘点腿 bug，报修）", l.port, l.addr),
            },
            None => format!("端口 {} 不在清单", l.port),
        };
        offenders.push(Offender { listener: l.clone(), reason });
    }
    let missing = rules
        .iter()
        .filter(|r| !listeners.iter().any(|l| l.port == r.port))
        .cloned()
        .collect();
    Verdict { offenders, missing }
}

/// 读两份 /proc 文本 → 监听全集（两份全缺 = Err：盘点不能装绿）。
pub fn inventory(proc_dir: &Path) -> Result<Vec<Listener>, String> {
    let mut out = Vec::new();
    let mut saw_any = false;
    for (name, v6) in [("tcp", false), ("tcp6", true)] {
        if let Ok(text) = fs::read_to_string(proc_dir.join(name)) {
            saw_any = true;
            out.extend(parse_proc(&text, v6)?);
        }
    }
    if !saw_any {
        return Err(format!(
            "读不到 {}/tcp 或 tcp6（host 无 /proc？盘点不能装绿）",
            proc_dir.display()
        ));
    }
    out.sort_by_key(|l| (l.port, l.v6));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    const TCP_FIX: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
     0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11101 1 0000000000000000 0 0 0 0 0\n\
     1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 22202 1 0000000000000000 0 0 0 0 0\n\
     2: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 33303 1 0000000000000000 0 0 0 0 0\n\
     3: 6C0AA801:E6D8 0100007F:E3B0 01 00000000:00000000 02:000A9F1F 00000000  1000        0 44404 2 28 4 30 10 -1\n";

    const TCP6_FIX: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
     4: 00000000000000000000000001000000:21F4 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 55505 1 0000000000000000 0 0 0 0 0\n\
     5: 0000000000000000FFFF00000100007F:0A0A 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 66606 1 0000000000000000 0 0 0 0 0\n";

    fn v4(a: [u8; 4]) -> IpAddr { IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3])) }

    #[test]
    fn parses_v4_le_and_filters_non_listen() {
        let ls = parse_proc(TCP_FIX, false).unwrap();
        assert_eq!(ls.len(), 3, "ESTAB 行必须滤掉");
        assert_eq!(ls[0].port, 8080);
        assert_eq!(ls[0].addr, v4([127, 0, 0, 1]));
        assert!(!ls[0].any);
        assert_eq!(ls[0].inode, "11101");
        assert_eq!(ls[1].port, 22);
        assert_eq!(ls[1].addr, v4([0, 0, 0, 0]));
        assert!(ls[1].any);
        assert_eq!(ls[2].port, 8080); // 同口两绑定各自成行
    }

    #[test]
    fn parses_v6_group_le() {
        let ls = parse_proc(TCP6_FIX, true).unwrap();
        assert_eq!(ls[0].port, 0x21F4);
        assert_eq!(ls[0].addr, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert!(!ls[0].any);
        // ::ffff:127.0.0.1 的 v4 映射组：末组 LE 还原成 7F 00 00 01
        assert_eq!(
            ls[1].addr,
            IpAddr::V6("::ffff:127.0.0.1".parse::<Ipv6Addr>().unwrap())
        );
    }

    #[test]
    fn malformed_proc_line_is_err_not_silent() {
        let bad = "hd\n  0: ZZ:1F90 0:0 0A 0 0 0 0 0 1 0\n";
        assert!(parse_proc(bad, false).is_err());
    }

    #[test]
    fn rule_line_shapes() {
        let r = parse_rule_line("22@*", "t").unwrap().unwrap();
        assert_eq!(r.addr, RuleAddr::Any);
        assert_eq!(parse_rule_line("# only comment", "t").unwrap(), None);
        assert_eq!(parse_rule_line("   ", "t").unwrap(), None);
        assert!(parse_rule_line("22", "t").is_err(), "缺 @ 必须报");
        assert!(parse_rule_line("abc@*", "t").is_err());
        assert!(parse_rule_line("22@nope", "t").is_err());
        // 通配三种写法等价
        for w in ["*", "0.0.0.0", "::"] {
            let r = parse_rule_line(&format!("8089@{w}"), "t").unwrap().unwrap();
            assert_eq!(r.addr, RuleAddr::Any);
        }
        let r = parse_rule_line("8089@127.0.0.1  # 收口目标", "t").unwrap().unwrap();
        assert_eq!(r.addr, RuleAddr::Exact("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn judge_matrix() {
        let lo = Listener { port: 8080, addr: v4([127, 0, 0, 1]), any: false, v6: false, inode: "1".into() };
        let any8080 = Listener { port: 8080, addr: v4([0, 0, 0, 0]), any: true, v6: false, inode: "2".into() };
        let ssh = Listener { port: 22, addr: v4([0, 0, 0, 0]), any: true, v6: false, inode: "3".into() };

        // 空清单 + 有监听 = 全员 offender（fail-closed）
        let v = judge(&[ssh.clone()], &[]);
        assert_eq!(v.offenders.len(), 1);
        assert!(v.red(false));

        // 22@* 放行 any 绑定与具体绑定（口放行不限面）
        let star = Rule { port: 22, addr: RuleAddr::Any, src: "t".into() };
        assert!(rule_allows(&star, &ssh));
        let ssh_lo = Listener { port: 22, addr: v4([127, 0, 0, 1]), any: false, v6: false, inode: "3".into() };
        assert!(rule_allows(&star, &ssh_lo));

        // 8089@127.0.0.1：loopback 绑定放行、any 绑定红（面比清单宽）
        let exact = Rule { port: 8080, addr: RuleAddr::Exact(v4([127, 0, 0, 1])), src: "t".into() };
        assert!(rule_allows(&exact, &lo));
        assert!(!rule_allows(&exact, &any8080));
        let v = judge(&[lo.clone(), any8080], &[exact]);
        assert_eq!(v.offenders.len(), 1);
        assert!(v.offenders[0].reason.contains("只许 127.0.0.1"));

        // 清单外的口红、报「不在清单」
        let v = judge(&[lo], &[star]);
        assert_eq!(v.offenders.len(), 1);
        assert!(v.offenders[0].reason.contains("不在清单"));

        // 清单写了没人听：默认不红，--require 红
        let star2 = Rule { port: 22, addr: RuleAddr::Any, src: "t".into() };
        let v = judge(&[], &[star2]);
        assert!(!v.red(false));
        assert!(v.red(true));
        assert_eq!(v.missing.len(), 1);
    }

    #[test]
    fn load_rules_merges_three_sources_and_dedupes() {
        let tmp = std::env::temp_dir().join(format!("netgate-t-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let base = tmp.join("listeners.allow");
        let dropin = tmp.join("listeners.allow.d");
        let pkgs = tmp.join("pkgfiles");
        std::fs::create_dir_all(&dropin).unwrap();
        std::fs::create_dir_all(pkgs.join("aginx-gateway")).unwrap();
        std::fs::create_dir_all(pkgs.join("aginx-channels")).unwrap();
        std::fs::write(&base, "# 正本\n22@*\n8089@*\n").unwrap();
        std::fs::write(dropin.join("10-extra.allow"), "8686@127.0.0.1\n22@*\n").unwrap();
        std::fs::write(pkgs.join("aginx-gateway/netgate.allow"), "# 包自表\n8686@127.0.0.1\n").unwrap();
        // 没带 netgate.allow 的包不参与
        let rules = load_rules(&RuleSources { base, dropin: dropin.clone(), pkgfiles: pkgs }).unwrap();
        assert_eq!(rules.len(), 3, "同口同面去重：22@*/8089@*/8686@127.0.0.1");
        assert!(rules.iter().any(|r| r.port == 8686));

        // 坏行拒判（fail-closed）
        std::fs::write(dropin.join("20-bad.allow"), "not-a-rule\n").unwrap();
        let err = load_rules(&RuleSources {
            base: tmp.join("listeners.allow"),
            dropin,
            pkgfiles: tmp.join("pkgfiles"),
        });
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn missing_everything_is_ok_when_machine_quiet() {
        // 三源全缺 + 无监听文件：load 空、judge 空 → 绿（安静机器过门）
        let tmp = std::env::temp_dir().join(format!("netgate-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let rules = load_rules(&RuleSources {
            base: tmp.join("none.allow"),
            dropin: tmp.join("none.allow.d"),
            pkgfiles: tmp.join("none-pkgfiles"),
        })
        .unwrap();
        assert!(rules.is_empty());
        assert!(!judge(&[], &rules).red(true));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
