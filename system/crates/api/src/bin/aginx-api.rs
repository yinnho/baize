//! aginx-api — D17 api 腿 CLI 直调面。
//!
//! stdout = JSON 信封；密文/完整 access_token 永不打印（token 工具只露前缀）。

use std::collections::HashMap;
use std::process::ExitCode;

use aginx_api::runtime::{run_tool, RunOptions};
use aginx_api::{scan_legs, channels_root};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "aginx-api", about = "D17 api 腿运行时（声明件 + secretd）")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 列出已加载腿与工具名（默认隐藏 admin=true；加 --include-admin 才见 freepublish 等）
    List {
        #[arg(long)]
        channel: Option<String>,
        /// 列出 admin=true 工具（人审门；默认不展示）
        #[arg(long)]
        include_admin: bool,
    },
    /// 执行声明工具
    Run {
        tool: String,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        account: Option<String>,
        /// k=v，可重复；JSON 值可用 raw: 前缀
        #[arg(long = "param", value_name = "K=V")]
        params: Vec<String>,
        /// 允许 admin=true 工具（人审后门）
        #[arg(long)]
        admin: bool,
        /// 对 token 工具打印完整 access_token（默认打码；危险，仅调试）
        #[arg(long)]
        show_token: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::List {
            channel,
            include_admin,
        } => match scan_legs(channel.as_deref()) {
            Ok(legs) => {
                let data: Vec<Value> = legs
                    .iter()
                    .map(|l| {
                        let policy = aginx_api::load_channel_policy(&l.channel).unwrap_or_default();
                        let tools: Vec<Value> = l
                            .tools
                            .iter()
                            .filter(|t| include_admin || !t.admin)
                            .map(|t| {
                                json!({
                                    "name": t.name,
                                    "method": t.method,
                                    "admin": t.admin,
                                })
                            })
                            .collect();
                        json!({
                            "channel": l.channel,
                            "path": l.path.display().to_string(),
                            "policy": {
                                "draft_only": policy.draft_only,
                                "publish_admin_gate": policy.publish_admin_gate,
                            },
                            "tools": tools,
                        })
                    })
                    .collect();
                print_ok(json!({
                    "channels_root": channels_root().display().to_string(),
                    "include_admin": include_admin,
                    "legs": data
                }));
                ExitCode::SUCCESS
            }
            Err(e) => fail_usage(&e.to_string()),
        },
        Cmd::Run {
            tool,
            channel,
            account,
            params,
            admin,
            show_token,
        } => {
            let mut map = HashMap::new();
            for p in params {
                let Some((k, v)) = p.split_once('=') else {
                    return fail_usage(&format!("bad --param {p:?} (want K=V)"));
                };
                let val = if let Some(raw) = v.strip_prefix("raw:") {
                    serde_json::from_str(raw).unwrap_or(Value::String(raw.to_string()))
                } else {
                    Value::String(v.to_string())
                };
                map.insert(k.to_string(), val);
            }
            let opts = RunOptions {
                channel,
                account,
                params: map,
                admin,
                redact_token: !show_token,
                resolve_depth: 0,
            };
            match run_tool(&tool, &opts) {
                Ok(r) => {
                    let env = r.to_json();
                    println!("{}", serde_json::to_string(&env).unwrap_or_else(|_| "{}".into()));
                    if r.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(1)
                    }
                }
                Err(e) => fail_state(&e.to_string()),
            }
        }
    }
}

fn print_ok(data: Value) {
    println!("{}", serde_json::to_string(&json!({"ok": true, "data": data})).unwrap());
}

fn fail_usage(msg: &str) -> ExitCode {
    eprintln!(
        "{}",
        serde_json::to_string(&json!({
            "ok": false,
            "error": {"type": "usage", "code": "usage", "message": msg}
        }))
        .unwrap()
    );
    ExitCode::from(2)
}

fn fail_state(msg: &str) -> ExitCode {
    // 剥离可能误入的 secret 形态：超长 hex/似 token 不原样回显
    let safe = sanitize_err(msg);
    eprintln!(
        "{}",
        serde_json::to_string(&json!({
            "ok": false,
            "error": {"type": "state", "code": "run_failed", "message": safe}
        }))
        .unwrap()
    );
    ExitCode::from(1)
}

fn sanitize_err(msg: &str) -> String {
    // 粗线：连续 >40 的 url-safe 字符块打码
    let mut out = String::new();
    let mut buf = String::new();
    for c in msg.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            buf.push(c);
        } else {
            flush_buf(&mut out, &mut buf);
            out.push(c);
        }
    }
    flush_buf(&mut out, &mut buf);
    out
}

fn flush_buf(out: &mut String, buf: &mut String) {
    if buf.len() > 40 {
        out.push_str(&buf[..8]);
        out.push('…');
    } else {
        out.push_str(buf);
    }
    buf.clear();
}
