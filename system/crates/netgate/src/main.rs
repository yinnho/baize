//! aginx-netgate CLI — 监听盘点门的机上面。
//!
//! 退出码（学 aginx-done 的三分）：0=绿；3=红（清单外/面不符——查询的
//! 合法答案，不是故障）；1=盘点不能（/proc 读不到、清单坏行）；2=usage。
//! `--json` 出 D1 信封：判定本身成功仍是 `ok:true`（`data.verdict="red"`），
//! rc 保持 3 —— 夜巡门腿按 rc 分支，JSON 消费者按 data 分支。

use serde_json::json;
use std::path::PathBuf;
use std::process::exit;

use aginx_netgate::{inventory, judge, load_rules, Listener, RuleSources};

const RED: i32 = 3;

fn usage() -> &'static str {
    "usage: aginx-netgate [--json] [--require] [--list]
  默认        盘点 /proc/net/tcp{,6} 的 LISTEN，对允许清单判红绿
  --list      只盘点不判（排障：谁在听、听哪个面）
  --require   清单里写了却没人听的口也算红（收紧后证明口起来了）
  --allow F   允许清单正本（缺省 /etc/aginx/listeners.allow；drop-in=同
              名 .d/*.allow；包自表 /var/lib/aginx/pkgfiles/*/netgate.allow）

清单行：port@addr；addr=* / 0.0.0.0 / :: = 口放行不限面；具体 IP = 只许
那个面。rc：0 绿 / 3 红 / 1 盘点不能 / 2 usage。
env 覆盖（测试）：AGINX_NETGATE_ALLOW、AGINX_NETGATE_PKGFILES、
AGINX_NETGATE_PROC_DIR（缺省 /proc/net）。"
}

fn proc_dir() -> PathBuf {
    std::env::var_os("AGINX_NETGATE_PROC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(aginx_netgate::DEFAULT_PROC_DIR))
}

fn show_listener(l: &Listener) -> String {
    let face = if l.any { format!("{} (any)", l.addr) } else { l.addr.to_string() };
    let fam = if l.v6 { "tcp6" } else { "tcp" };
    format!("  LISTEN {:<5} {:<15} {:<4} inode {}", l.port, face, fam, l.inode)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let list_only = args.iter().any(|a| a == "--list");
    let require = args.iter().any(|a| a == "--require");

    let mut allow: Option<String> = None;
    let mut bad = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--allow" if i + 1 < args.len() => {
                allow = Some(args[i + 1].clone());
                i += 2;
            }
            "--json" | "--list" | "--require" => i += 1,
            "--help" | "-h" => {
                println!("{}", usage());
                exit(0);
            }
            _ => {
                bad = true;
                i += 1;
            }
        }
    }
    if bad || args.is_empty() {
        println!("{}", usage());
        exit(if bad { 2 } else { 2 });
    }

    let listeners = match inventory(&proc_dir()) {
        Ok(l) => l,
        Err(e) => {
            if json {
                agio::exit_fail(agio::ErrorType::Io, "netgate_proc", &e);
            }
            eprintln!("aginx-netgate: {e}");
            exit(1);
        }
    };

    if list_only {
        if json {
            let ls: Vec<_> = listeners.iter().map(|l| json!({
                "port": l.port, "addr": l.addr.to_string(), "any": l.any, "v6": l.v6, "inode": l.inode,
            })).collect();
            agio::exit_ok(json!({ "listeners": ls, "count": ls.len() }));
        }
        println!("aginx-netgate: {} 个监听", listeners.len());
        for l in &listeners {
            println!("{}", show_listener(l));
        }
        exit(0);
    }

    let mut sources = RuleSources::resolve();
    if let Some(f) = allow {
        sources.base = PathBuf::from(f);
        sources.dropin = sources.base.with_extension("allow.d");
    }    let rules = match load_rules(&sources) {
        Ok(r) => r,
        Err(e) => {
            let hint = "清单坏行拒判（fail-closed）：修清单或删坏行，别绕门";
            if json {
                agio::exit_fail_hint(agio::ErrorType::State, "netgate_allowlist", &e, hint);
            }
            eprintln!("aginx-netgate: {e}\n  hint: {hint}");
            exit(1);
        }
    };

    let verdict = judge(&listeners, &rules);
    let red = verdict.red(require);

    if json {
        let env = agio::ok_meta(
            json!({
                "verdict": if red { "red" } else { "green" },
                "listeners": listeners.iter().map(|l| json!({
                    "port": l.port, "addr": l.addr.to_string(), "any": l.any, "v6": l.v6, "inode": l.inode,
                })).collect::<Vec<_>>(),
                "rules": rules.iter().map(|r| json!({
                    "port": r.port,
                    "addr": match &r.addr {
                        aginx_netgate::RuleAddr::Any => "*".to_string(),
                        aginx_netgate::RuleAddr::Exact(ip) => ip.to_string(),
                    },
                    "src": r.src,
                })).collect::<Vec<_>>(),
                "offenders": verdict.offenders.iter().map(|o| json!({
                    "port": o.listener.port,
                    "addr": o.listener.addr.to_string(),
                    "any": o.listener.any,
                    "reason": o.reason,
                })).collect::<Vec<_>>(),
                "missing_expected": verdict.missing.iter().map(|r| json!({
                    "port": r.port,
                    "src": r.src,
                })).collect::<Vec<_>>(),
            }),
            json!({ "require": require, "listeners": listeners.len(), "rules": rules.len() }),
        );
        agio::print(&env);
        exit(if red { RED } else { 0 });
    }

    println!("aginx-netgate: 盘点 {} 监听 / 清单 {} 条", listeners.len(), rules.len());
    for o in &verdict.offenders {
        println!("RED  {}", o.reason);
    }
    for m in &verdict.missing {
        println!("WARN 清单写了但没人听：{}（{}）——require 模式算红", m.port, m.src);
    }
    if !red {
        println!("aginx-netgate: green（清单即面）");
    }
    exit(if red { RED } else { 0 });
}
