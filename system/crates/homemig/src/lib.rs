//! aginx-home-migrate — /home 迁移层（omacom 每用户迁移借鉴，2026-10-31）。
//!
//! 人格树没有发版号，但格式会长：目录改名、frontmatter 换代、账本
//! 换形状。以前只靠「前向兼容 + 知识件自描述」硬扛；本 crate 给它一条
//! 可验收的迁移正路：
//!
//! - **迁移稿住人格树**：`${AGINX_HOME}/migrations/NNN-slug.sh`（改
//!   「人」的件，随人格备份/认祖走，duphub 可分发）；同名 `.md` 是稿的
//!   说明文档，不参与执行。
//! - **标记住机身**：`/var/lib/aginx/home-migrate/<NNN-slug>` 内容
//!   `epoch=…` + `sha256=…`（advisory）——**这台身体**施过哪些法。认祖
//!   到新身体 = 标记空 = 全部重跑，所以——
//! - **稿必须幂等**：这是硬纪律，不是建议。第一支真迁移起就用
//!   `[ -e 新 ] || mv 旧 新` 的姿势写。
//! - **严格顺序**：按 NNN 升序施待跑；失败即停，失败者**不落标记**
//!   （下次自然重试——同 aginx-done 的 check||step&&mark 纪律）。
//! - **发布后不改稿**：标记里的 sha 对不上现稿 = `Edited`，run 拒跑
//!   （要改就取新编号——改已施过的法等于偷改历史）。
//! - **谁跑**：系统自己（夜巡挂账→系统/主人发起 `run`）。provision/
//!   boot 永不代跑——HOME-PHASE Finalize 禁 silent /home 写，迁移是
//!   人格树自己的事，不是机身装机的事。
//! - 稿的运行环境：`sh <稿>`，cwd=`$AGINX_HOME`，env 带 `BAIZE_HOME`，
//!   PATH 前置 `/var/bin`（可调面 CLI）。改机身的活不写在这（那是
//!   pkg 供应链的事）。
//!
//! 核心函数全参数化（显式传路径），env 解析只在薄壳层——测试零 env
//! 竞态。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

pub const DEFAULT_HOME: &str = "/home";
pub const DEFAULT_MARKER_DIR: &str = "/var/lib/aginx/home-migrate";

/// 一支迁移（NNN-slug.sh）。num 001–999，slug [a-z0-9-]+。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub num: u32,
    pub slug: String,
    pub path: PathBuf,
}

impl Migration {
    pub fn name(&self) -> String {
        format!("{:03}-{}", self.num, self.slug)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Pending,
    Applied { at: u64, sha: String },
    /// 标记在、稿变了——run 拒跑，要改取新编号。
    Edited { at: u64, sha: String, now_sha: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Applied(Migration),
    Planned(Migration),
    /// 失败即停：这支失败、后面不动、失败者无标记。
    Failed { migration: Migration, code: i32, stderr_tail: String },
    /// 队列里有 Edited 稿，整队拒跑。
    RefusedEdited(Migration),
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("AGINX_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_HOME))
}

pub fn marker_root() -> PathBuf {
    std::env::var_os("AGINX_HOMEMIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MARKER_DIR))
}

fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}：{e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// 扫迁移目录：只认 `NNN-slug.sh`（迁移）与 `NNN-slug.md`（文档，跳过）；
/// 目录不存在 = 空队（没有迁移的人格树完全正常）；其它任何条目 = Err
/// （严格目录——塞错东西的人要被听见，不是被跳过）。编号重复 = Err。
pub fn discover(dir: &Path) -> Result<Vec<Migration>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    let mut migs = Vec::new();
    let mut offenders = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if !path.is_file() {
            offenders.push(format!("{}（不是文件）", name));
            continue;
        }
        if let Some(stem) = name.strip_suffix(".md") {
            let _ = stem; // 文档：同名 .sh 的说明，不参与执行
            continue;
        }
        let Some(stem) = name.strip_suffix(".sh") else {
            offenders.push(format!("{}（既非 .sh 也非 .md）", name));
            continue;
        };
        let Some((n, slug)) = stem.split_once('-') else {
            offenders.push(format!("{}（缺 NNN-slug 形）", name));
            continue;
        };
        let Ok(num) = n.parse::<u32>() else {
            offenders.push(format!("{}（编号不是数字）", name));
            continue;
        };
        if !(1..=999).contains(&num) || n.len() != 3 || slug.is_empty() || !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            offenders.push(format!("{}（编号须三位 001–999，slug 须 [a-z0-9-]）", name));
            continue;
        }
        migs.push(Migration { num, slug: slug.to_string(), path });
    }
    if !offenders.is_empty() {
        return Err(format!("{} 里有不认识的条目：{}", dir.display(), offenders.join("、")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for m in &migs {
        if !seen.insert(m.num) {
            return Err(format!("{} 里编号 {} 重复（{}）——顺序是契约，不许撞号", dir.display(), m.num, m.name()));
        }
    }
    migs.sort_by_key(|m| m.num);
    Ok(migs)
}

fn read_marker(marker_dir: &Path, m: &Migration) -> Option<(u64, String)> {
    let text = fs::read_to_string(marker_dir.join(m.name())).ok()?;
    let mut at = None;
    let mut sha = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("epoch=") {
            at = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("sha256=") {
            sha = Some(v.trim().to_string());
        }
    }
    Some((at?, sha?))
}

/// 每支迁移的现状（Applied / Pending / Edited）。
pub fn states(migs: &[Migration], marker_dir: &Path) -> Vec<(Migration, State)> {
    migs.iter()
        .map(|m| {
            let state = match read_marker(marker_dir, m) {
                Some((at, sha)) => match sha256_file(&m.path) {
                    Ok(now_sha) if now_sha == sha => State::Applied { at, sha },
                    Ok(now_sha) => State::Edited { at, sha, now_sha },
                    Err(_) => State::Pending, // 读不了稿按待跑处理（run 时会再炸一次给人看）
                },
                None => State::Pending,
            };
            (m.clone(), state)
        })
        .collect()
}

/// 待跑队（Pending 按号升序）。Edited 不在队里——它挡 run，不是待跑。
pub fn pending(migs: &[Migration], marker_dir: &Path) -> Vec<Migration> {
    states(migs, marker_dir)
        .into_iter()
        .filter(|(_, s)| matches!(s, State::Pending))
        .map(|(m, _)| m)
        .collect()
}

/// 施法。dry=true 只列计划不执行。home = 稿的 cwd 与 BAIZE_HOME。
pub fn run(
    migs: &[Migration],
    marker_dir: &Path,
    home: &Path,
    dry: bool,
) -> Result<Vec<RunOutcome>, String> {
    let mut out = Vec::new();
    for (m, state) in states(migs, marker_dir) {
        if let State::Edited { .. } = state {
            out.push(RunOutcome::RefusedEdited(m));
            break;
        }
    }
    if out.is_empty() {
        for m in pending(migs, marker_dir) {
            if dry {
                out.push(RunOutcome::Planned(m));
                continue;
            }
            fs::create_dir_all(marker_dir)
                .map_err(|e| format!("建标记目录 {}：{e}", marker_dir.display()))?;
            let path_env = format!(
                "/var/bin:{}",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into())
            );
            let res = Command::new("sh")
                .arg(&m.path)
                .current_dir(home)
                .env("BAIZE_HOME", home)
                .env("PATH", &path_env)
                .output();
            match res {
                Ok(o) if o.status.success() => {
                    let sha = sha256_file(&m.path)?;
                    let at = now_epoch();
                    fs::write(marker_dir.join(m.name()), format!("epoch={at}\nsha256={sha}\n"))
                        .map_err(|e| format!("写标记 {}：{e}", m.name()))?;
                    out.push(RunOutcome::Applied(m));
                }
                Ok(o) => {
                    let tail = String::from_utf8_lossy(&o.stderr).to_string();
                    let tail = tail.chars().rev().take(800).collect::<Vec<_>>();
                    let tail: String = tail.into_iter().rev().collect();
                    out.push(RunOutcome::Failed { migration: m, code: o.status.code().unwrap_or(-1), stderr_tail: tail });
                    break;
                }
                Err(e) => {
                    return Err(format!("拉起 {} 失败：{e}", m.name()));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("homemig-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let markers = root.join("markers");
        fs::create_dir_all(home.join("migrations")).unwrap();
        fs::create_dir_all(&markers).unwrap();
        (root, home.join("migrations"))
    }

    fn mig(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    fn markers_of(root: &Path) -> PathBuf {
        root.join("markers")
    }

    #[test]
    fn naming_and_strict_dir() {
        let (root, dir) = setup("naming");
        mig(&dir, "001-rename-traces.sh", ":");
        mig(&dir, "001-rename-traces.md", "# doc");
        let migs = discover(&dir).unwrap();
        assert_eq!(migs.len(), 1, ".md 是文档不计数");
        assert_eq!(migs[0].name(), "001-rename-traces");

        mig(&dir, "002 bad.sh", ":");
        assert!(discover(&dir).is_err(), "坏名要炸不是跳");
        fs::remove_file(dir.join("002 bad.sh")).unwrap();

        mig(&dir, "002-x.sh", ":");
        mig(&dir, "002-y.sh", ":");
        assert!(discover(&dir).is_err(), "撞号要炸");

        let (root2, dir2) = setup("naming2");
        mig(&dir2, "0099-zero-pad.sh", ":");
        assert!(discover(&dir2).is_err(), "编号必须三位");
        // 目录不存在 = 空队
        assert!(discover(&root2.join("nope")).unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&root2);
    }

    #[test]
    fn run_applies_in_order_marks_and_is_idempotent() {
        let (root, dir) = setup("order");
        let home = root.join("home");
        // 幂等姿势的稿：目标在就不动
        mig(&dir, "001-touch.sh", "[ -e ran-001 ] || echo hi > ran-001\n");
        mig(&dir, "002-touch.sh", "[ -e ran-002 ] || echo hi > ran-002\n");
        let migs = discover(&dir).unwrap();
        let out = run(&migs, &markers_of(&root), &home, false).unwrap();
        assert!(matches!(&out[0], RunOutcome::Applied(m) if m.num == 1));
        assert!(matches!(&out[1], RunOutcome::Applied(m) if m.num == 2));
        assert!(home.join("ran-001").is_file());
        assert!(markers_of(&root).join("001-touch").is_file());

        // 全 applied 后再跑：空手而归（幂等重入）
        let out2 = run(&migs, &markers_of(&root), &home, false).unwrap();
        assert!(out2.is_empty());

        // 认祖到新身体：标记空 → 全部重跑 → 幂等稿不重复施法
        let new_body = root.join("home2");
        fs::create_dir_all(&new_body).unwrap();
        let out3 = run(&migs, &root.join("markers2"), &new_body, false).unwrap();
        assert_eq!(out3.len(), 2);
        assert!(new_body.join("ran-002").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failure_stops_and_leaves_no_marker() {
        let (root, dir) = setup("fail");
        let home = root.join("home");
        mig(&dir, "001-ok.sh", "echo ok > ok.txt\n");
        mig(&dir, "002-boom.sh", "echo boom-stderr >&2\nexit 7\n");
        mig(&dir, "003-never.sh", "echo no > never.txt\n");
        let migs = discover(&dir).unwrap();
        let out = run(&migs, &markers_of(&root), &home, false).unwrap();
        assert_eq!(out.len(), 2, "失败即停：001 applied + 002 failed");
        match &out[1] {
            RunOutcome::Failed { migration, code, stderr_tail } => {
                assert_eq!(migration.num, 2);
                assert_eq!(*code, 7);
                assert!(stderr_tail.contains("boom-stderr"));
            }
            other => panic!("要 Failed，得到 {other:?}"),
        }
        assert!(home.join("ok.txt").is_file());
        assert!(!home.join("never.txt").exists(), "后面的不许跑");
        assert!(!markers_of(&root).join("002-boom").exists(), "失败者不落标记");
        // 修好稿再跑：只补 002/003
        fs::write(dir.join("002-boom.sh"), "echo fixed > fixed.txt\n").unwrap();
        let out2 = run(&migs, &markers_of(&root), &home, false).unwrap();
        assert_eq!(out2.len(), 2);
        assert!(home.join("never.txt").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn edited_after_mark_refuses_run() {
        let (root, dir) = setup("edited");
        let home = root.join("home");
        mig(&dir, "001-a.sh", "echo v1 > a.txt\n");
        let migs = discover(&dir).unwrap();
        run(&migs, &markers_of(&root), &home, false).unwrap();
        // 发布后改稿
        fs::write(dir.join("001-a.sh"), "echo v2 > a.txt\n").unwrap();
        let st = states(&migs, &markers_of(&root));
        assert!(matches!(st[0].1, State::Edited { .. }));
        mig(&dir, "002-b.sh", "echo b > b.txt\n");
        let migs2 = discover(&dir).unwrap();
        let out = run(&migs2, &markers_of(&root), &home, false).unwrap();
        assert!(matches!(&out[0], RunOutcome::RefusedEdited(m) if m.num == 1), "整队拒跑");
        assert!(!home.join("b.txt").exists(), "拒跑时后面也不动");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dry_run_plans_without_touching() {
        let (root, dir) = setup("dry");
        let home = root.join("home");
        mig(&dir, "001-x.sh", "echo x > x.txt\n");
        let migs = discover(&dir).unwrap();
        let out = run(&migs, &markers_of(&root), &home, true).unwrap();
        assert!(matches!(&out[0], RunOutcome::Planned(m) if m.num == 1));
        assert!(!home.join("x.txt").exists(), "dry 不施法");
        assert!(!markers_of(&root).join("001-x").exists(), "dry 不落标记");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn script_env_has_baize_home_and_varbin_path() {
        let (root, dir) = setup("env");
        let home = root.join("home");
        mig(&dir, "001-env.sh", "printf '%s\\n' \"$BAIZE_HOME\" > env-home.txt\nprintf '%s' \"$PATH\" > env-path.txt\n");
        let migs = discover(&dir).unwrap();
        run(&migs, &markers_of(&root), &home, false).unwrap();
        let got = fs::read_to_string(home.join("env-home.txt")).unwrap();
        assert_eq!(got.trim(), home.to_string_lossy());
        let path = fs::read_to_string(home.join("env-path.txt")).unwrap();
        assert!(path.starts_with("/var/bin:"), "面 CLI 可直接调：{path}");
        let _ = fs::remove_dir_all(&root);
    }
}
