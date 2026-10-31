//! aginx-home-migrate CLI — /home 迁移层的机上面。
//!
//! 退出码（同 aginx-done 三分）：0=成功/无待跑；3=check 的「有 pending
//! 或 Edited」（夜巡挂账语义，查询的合法答案）；1=run 失败/Edited 拒跑/
//! io 故障；2=usage。`--json` 出 D1 信封。

use serde_json::json;
use std::process::exit;

use aginx_home_migrate::{discover, home_dir, marker_root, run, states, Migration, RunOutcome, State};

const PENDING: i32 = 3;

fn usage() -> &'static str {
    "usage: aginx-home-migrate [--json] <command>
  list            全部迁移 + 现状（applied / pending / EDITED-改稿拒跑）
  check           0=无待跑，3=有 pending 或 EDITED（夜巡门腿挂账用）
  run [--dry-run] 顺序施待跑：成功落标记；失败即停不落标记
  paths           显示目录解析（排障：migrations 在哪、标记在哪）

迁移稿：${AGINX_HOME}/migrations/NNN-slug.sh（001–999，slug [a-z0-9-]，
同名 .md 是文档）。稿必须幂等（认祖到新身体会重跑）；失败非零即停；
发布后不改稿（sha 不符=EDITED 拒跑，要改取新编号）。稿运行环境：
sh，cwd=$AGINX_HOME，env 带 BAIZE_HOME，PATH 前置 /var/bin。
env 覆盖（测试）：AGINX_HOME、AGINX_HOMEMIG_DIR。"
}

fn state_str(s: &State) -> String {
    match s {
        State::Pending => "pending".into(),
        State::Applied { at, .. } => format!("applied at={at}"),
        State::Edited { .. } => "EDITED（改稿拒跑——要改取新编号）".into(),
    }
}

fn mig_json(m: &Migration, s: &State) -> serde_json::Value {
    let (state, at) = match s {
        State::Pending => ("pending", None),
        State::Applied { at, .. } => ("applied", Some(*at)),
        State::Edited { at, .. } => ("edited", Some(*at)),
    };
    json!({ "name": m.name(), "num": m.num, "slug": m.slug, "state": state, "applied_at": at })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let rest: Vec<&str> = args.iter().filter(|a| !a.starts_with("--")).map(String::as_str).collect();

    match rest.as_slice() {
        [] => {
            println!("{}", usage());
            exit(2);
        }
        ["--help"] | ["-h"] => {
            println!("{}", usage());
            exit(0);
        }
        ["paths"] => {
            let home = home_dir();
            let mig = home.join("migrations");
            if json {
                agio::exit_ok(json!({
                    "home": home, "migrations": mig, "markers": marker_root(),
                }));
            }
            println!("home       {}", home.display());
            println!("migrations {}", mig.display());
            println!("markers    {}", marker_root().display());
            exit(0);
        }
        ["list"] | ["check"] => {
            let mdir = home_dir().join("migrations");
            let migs = match discover(&mdir) {
                Ok(m) => m,
                Err(e) => {
                    if json {
                        agio::exit_fail(agio::ErrorType::State, "homemig_dir", &e);
                    }
                    eprintln!("aginx-home-migrate: {e}");
                    exit(1);
                }
            };
            let st = states(&migs, &marker_root());
            let is_check = rest[0] == "check";
            let flagged = st
                .iter()
                .any(|(_, s)| matches!(s, State::Pending | State::Edited { .. }));
            if json {
                let env = agio::ok_meta(
                    json!({ "migrations": st.iter().map(|(m, s)| mig_json(m, s)).collect::<Vec<_>>() }),
                    json!({ "pending_or_edited": flagged, "mode": rest[0] }),
                );
                agio::print(&env);
            } else {
                if st.is_empty() {
                    println!("aginx-home-migrate: 无迁移（空人格树完全正常）");
                }
                for (m, s) in &st {
                    println!("  {:<24} {}", m.name(), state_str(s));
                }
            }
            exit(if is_check && flagged { PENDING } else { 0 });
        }
        ["run"] | ["run", "--dry-run"] => {
            let dry = rest.len() == 2;
            let home = home_dir();
            let mdir = home.join("migrations");
            let migs = match discover(&mdir) {
                Ok(m) => m,
                Err(e) => {
                    if json {
                        agio::exit_fail(agio::ErrorType::State, "homemig_dir", &e);
                    }
                    eprintln!("aginx-home-migrate: {e}");
                    exit(1);
                }
            };
            let out = match run(&migs, &marker_root(), &home, dry) {
                Ok(o) => o,
                Err(e) => {
                    if json {
                        agio::exit_fail(agio::ErrorType::Io, "homemig_run", &e);
                    }
                    eprintln!("aginx-home-migrate: {e}");
                    exit(1);
                }
            };
            let mut failed = false;
            let mut results = Vec::new();
            for o in &out {
                match o {
                    RunOutcome::Applied(m) => {
                        println!("applied  {}", m.name());
                        results.push(json!({ "name": m.name(), "result": "applied" }));
                    }
                    RunOutcome::Planned(m) => {
                        println!("planned  {}", m.name());
                        results.push(json!({ "name": m.name(), "result": "planned" }));
                    }
                    RunOutcome::Failed { migration, code, stderr_tail } => {
                        failed = true;
                        println!("FAILED  {} rc={} — 停队不落标记", migration.name(), code);
                        if !stderr_tail.trim().is_empty() {
                            println!("  stderr 尾：{}", stderr_tail.trim());
                        }
                        results.push(json!({ "name": migration.name(), "result": "failed", "code": code }));
                    }
                    RunOutcome::RefusedEdited(m) => {
                        failed = true;
                        println!("REFUSED {} 已施过的稿被改（EDITED）——要改取新编号，整队拒跑", m.name());
                        results.push(json!({ "name": m.name(), "result": "refused_edited" }));
                    }
                }
            }
            if out.is_empty() {
                println!("aginx-home-migrate: 无待跑");
            }
            if json {
                let env = agio::ok_meta(
                    json!({ "dry": dry, "results": results }),
                    json!({ "count": out.len() }),
                );
                agio::print(&env);
            }
            exit(if failed { 1 } else { 0 });
        }
        _ => {
            println!("{}", usage());
            exit(2);
        }
    }
}
