//! aginx-agent-usage CLI — 用量契约的机上面：emit（采集器调）+ report
//! （主人/夜巡看账）。rc：0 成功；1 验证/io 失败；2 usage。

use serde_json::json;
use std::process::exit;

use aginx_agent_usage::{aggregate, dir, emit_into, load, Record};

fn usage() -> &'static str {
    "usage: aginx-agent-usage [--json] <command>
  emit --engine E [--session S] [--model M] [--ts T] [--in N] [--out N]
       [--turns N] [--ms N]          追加一条用量记录（采集器调）
  report [--day YYYY-MM-DD] [--engine E]   聚合看账（缺省=全部天）

schema（唯一记录形状，zeroed-not-null）：
  {ts, engine, session, model, input_tokens, output_tokens, turns, duration_ms}
记录家 /var/lib/aginx/usage/<UTC日>.jsonl（机器本地劳力账；flock 串行）。
采集器形状：读引擎自己的会话档（只读）→ 摘增量 → emit。一个引擎一个
采集器，共享本 schema，不发明第二种记录格式。
env 覆盖（测试）：AGINX_USAGE_DIR。"
}

fn num_flag(args: &[String], name: &str) -> Result<Option<u64>, String> {
    let mut it = args.iter().zip(args.iter().skip(1));
    while let Some((a, v)) = it.next() {
        if a == name {
            return v
                .parse()
                .map(Some)
                .map_err(|_| format!("{name} 要数字，得到「{v}」"));
        }
    }
    Ok(None)
}

fn str_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().zip(args.iter().skip(1)).find(|(a, _)| a.as_str() == name).map(|(_, v)| v.as_str())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    // 命令 = 首个非 --json 的词；其余全是该命令的旗标/值（值里可以有
    // 不带 -- 的词，所以不能拿「非 -- 开头」当位置参数收）。
    let positional: Vec<&str> = args.iter().filter(|a| **a != "--json" && !a.starts_with("--")).map(String::as_str).collect();
    let cmd = positional.first().copied();
    let flags: Vec<String> = args
        .iter()
        .filter(|a| *a != "--json" && a.as_str() != cmd.unwrap_or(""))
        .cloned()
        .collect();

    match cmd {
        None => {
            println!("{}", usage());
            exit(2);
        }
        Some("--help") | Some("-h") => {
            println!("{}", usage());
            exit(0);
        }
        Some("emit") => {
            let args = &flags;
            let rec = Record {
                ts: num_flag(args, "--ts").unwrap_or_else(|e| die_usage(&e)).unwrap_or(0),
                engine: str_flag(args, "--engine").unwrap_or_else(|| die_usage("--engine 必给（采集器要自称）")).to_string(),
                session: str_flag(args, "--session").unwrap_or("").to_string(),
                model: str_flag(args, "--model").unwrap_or("").to_string(),
                input_tokens: num_flag(args, "--in").unwrap_or_else(|e| die_usage(&e)).unwrap_or(0),
                output_tokens: num_flag(args, "--out").unwrap_or_else(|e| die_usage(&e)).unwrap_or(0),
                turns: num_flag(args, "--turns").unwrap_or_else(|e| die_usage(&e)).unwrap_or(0),
                duration_ms: num_flag(args, "--ms").unwrap_or_else(|e| die_usage(&e)).unwrap_or(0),
            };
            match emit_into(&dir(), rec) {
                Ok((n, day)) => {
                    if json {
                        agio::exit_ok(json!({ "written": n, "day": day }));
                    }
                    println!("aginx-agent-usage: +{n}B → {day}");
                    exit(0);
                }
                Err(e) => {
                    if json {
                        agio::exit_fail(agio::ErrorType::State, "usage_emit", &e);
                    }
                    eprintln!("aginx-agent-usage: {e}");
                    exit(1);
                }
            }
        }
        Some("report") => {
            let day = str_flag(&flags, "--day");
            let engine = str_flag(&flags, "--engine");
            let loaded = match load(&dir(), day) {
                Ok(l) => l,
                Err(e) => {
                    if json {
                        agio::exit_fail(agio::ErrorType::Io, "usage_report", &e);
                    }
                    eprintln!("aginx-agent-usage: {e}");
                    exit(1);
                }
            };
            let agg = aggregate(&loaded.records, engine);
            if json {
                agio::exit_ok_meta(
                    json!({
                        "engines": agg,
                        "malformed": loaded.malformed,
                        "files": loaded.files,
                    }),
                    json!({ "day": day, "engine": engine, "records": loaded.records.len() }),
                );
            }
            if loaded.files == 0 {
                println!("aginx-agent-usage: 无账（{}）", day.unwrap_or("全部天"));
                exit(0);
            }
            println!("{:<12} {:>8} {:>12} {:>13} {:>7} {:>10}", "engine", "records", "input_tok", "output_tok", "turns", "dur_s");
            for e in &agg {
                println!(
                    "{:<12} {:>8} {:>12} {:>13} {:>7} {:>10.1}",
                    e.engine, e.records, e.input_tokens, e.output_tokens, e.turns, e.duration_ms as f64 / 1000.0
                );
            }
            if loaded.malformed > 0 {
                println!("（跳过坏行 {} 条）", loaded.malformed);
            }
            exit(0);
        }
        Some(_) => {
            println!("{}", usage());
            exit(2);
        }
    }
}

fn die_usage(msg: &str) -> ! {
    eprintln!("aginx-agent-usage: {msg}");
    eprintln!("{}", usage());
    exit(2);
}
