//! aginx-agent-usage — 引擎用量归一契约（omacom usage-collector 借鉴，
//! 2026-10-31）。
//!
//! 引擎是商品劳力：claude/codex/grok 各自的会话档形状各异，主人要的账
//! 只有一种——**一个引擎一天烧了多少**。本 crate 定死三件：
//!
//! 1. **ONE record schema**（`Record`）：数字一律 zeroed-not-null（缺数
//!    =0，永不写 null——消费端不用二态解析）。
//! 2. **emit**：单点追加口。追加到 `/var/lib/aginx/usage/<UTC日>.jsonl`，
//!    `flock(LOCK_EX)` 串行（多采集器并发安全），一行一记录。**只追加
//!    不改写**——谁家的记录谁负责，别人的行不碰。
//! 3. **report**：按引擎/日聚合（malformed 行跳过并计数，不让一条坏行
//!    炸整份报告）。
//!
//! 各引擎的**采集器**不在本 crate：形状是「读引擎自己的会话档（只读）
//! → 摘增量 → 调 `aginx-agent-usage emit`」。一个引擎一个采集器，glob
//! 可发现，共享这一个 schema——不发明第二种记录格式。
//!
//! 日切用 UTC（无 tz 依赖；账的粒度是「一天」，不承诺时区语义）。
//! 记录家是**机器本地**的劳力账（不随认祖走——新身体的账从零记）。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_DIR: &str = "/var/lib/aginx/usage";

pub fn dir() -> PathBuf {
    std::env::var_os("AGINX_USAGE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DIR))
}

/// 唯一记录形状。新增字段只能加尾、给默认——已落盘的旧行按缺 0 读。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// epoch 秒（采集时刻；缺省=emit 当时）。
    pub ts: u64,
    /// 引擎名（采集器自称，如 claude / codex / grok；非空、无控制字符）。
    pub engine: String,
    /// 引擎自己的会话标识（可为空串；只做对账不承诺唯一）。
    pub session: String,
    /// 模型名（可为空串；zeroed-not-null）。
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub turns: u64,
    pub duration_ms: u64,
}

impl Record {
    /// 记录行的 UTC 日键（YYYY-MM-DD）。
    pub fn day(&self) -> String {
        day_from_epoch(self.ts)
    }
}

/// epoch 秒 → UTC YYYY-MM-DD（civil-from-days 算法，无 chrono 依赖）。
pub fn day_from_epoch(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // Howard Hinnant 的 civil_from_days：1970-01-01=day 0
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn valid_engine(e: &str) -> Result<(), String> {
    if e.is_empty() {
        return Err("engine 不能为空（采集器要自称）".into());
    }
    if e.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(format!("engine「{e}」含控制字符"));
    }
    Ok(())
}

/// 追加一条记录（flock 串行）。ts=0 → 取当时。返回（写入字节数，实际
/// 归账的 UTC 日）——调用方别再拿原 rec 算日（ts=0 的壳子算不出）。
pub fn emit_into(dir: &Path, mut rec: Record) -> Result<(usize, String), String> {
    valid_engine(&rec.engine)?;
    if rec.ts == 0 {
        rec.ts = now_epoch();
    }
    fs::create_dir_all(dir).map_err(|e| format!("建 {}: {e}", dir.display()))?;
    let line = serde_json::to_string(&rec).map_err(|e| e.to_string())?;
    let mut line = line.into_bytes();
    line.push(b'\n');

    let path = dir.join(format!("{}.jsonl", rec.day()));
    let day = rec.day();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("开 {}: {e}", path.display()))?;
    // LOCK_EX：多采集器并发追加的串行门（同机同文件）
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
    let n = f.write(&line).map_err(|e| format!("写 {}: {e}", path.display()))?;
    let _ = f.flush();
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_UN) };
    Ok((n, day))
}

/// 一天的文件名键（带 .jsonl）。
pub fn day_file(dir: &Path, day: &str) -> PathBuf {
    dir.join(format!("{day}.jsonl"))
}

/// 读一天（或全部天）解析成记录。malformed 行跳过并计数。
#[derive(Debug, Default)]
pub struct Loaded {
    pub records: Vec<Record>,
    pub malformed: usize,
    pub files: usize,
}

pub fn load(dir: &Path, day: Option<&str>) -> Result<Loaded, String> {
    let mut loaded = Loaded::default();
    let mut files: Vec<PathBuf> = Vec::new();
    match day {
        Some(d) => {
            let p = day_file(dir, d);
            if p.is_file() {
                files.push(p);
            }
        }
        None => {
            let Ok(rd) = fs::read_dir(dir) else {
                return Ok(loaded); // 目录不在 = 空账（新机器没有账不是错）
            };
            let mut names: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false))
                .collect();
            names.sort();
            files = names;
        }
    }
    for f in files {
        let text = fs::read_to_string(&f).map_err(|e| format!("读 {}: {e}", f.display()))?;
        loaded.files += 1;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Record>(line) {
                Ok(r) => loaded.records.push(r),
                Err(_) => loaded.malformed += 1,
            }
        }
    }
    Ok(loaded)
}

/// 按引擎聚合（可选引擎过滤）。
#[derive(Debug, Default, Serialize)]
pub struct EngineTotals {
    pub engine: String,
    pub records: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub turns: u64,
    pub duration_ms: u64,
}

pub fn aggregate(records: &[Record], engine_filter: Option<&str>) -> Vec<EngineTotals> {
    use std::collections::BTreeMap;
    let mut by: BTreeMap<String, EngineTotals> = BTreeMap::new();
    for r in records {
        if let Some(want) = engine_filter {
            if r.engine != want {
                continue;
            }
        }
        let e = by.entry(r.engine.clone()).or_insert_with(|| EngineTotals { engine: r.engine.clone(), ..Default::default() });
        e.records += 1;
        e.input_tokens += r.input_tokens;
        e.output_tokens += r.output_tokens;
        e.turns += r.turns;
        e.duration_ms += r.duration_ms;
    }
    by.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("usage-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn rec(engine: &str, ts: u64, i: u64, o: u64) -> Record {
        Record { ts, engine: engine.into(), session: "s1".into(), model: "m".into(), input_tokens: i, output_tokens: o, turns: 1, duration_ms: 1000 }
    }

    #[test]
    fn zeroed_not_null_serializes() {
        let r = Record { ts: 1, engine: "claude".into(), session: String::new(), model: String::new(), input_tokens: 0, output_tokens: 0, turns: 0, duration_ms: 0 };
        let j = serde_json::to_string(&r).unwrap();
        assert!(!j.contains("null"), "缺数=0 不许 null：{j}");
        for k in ["input_tokens", "output_tokens", "turns", "duration_ms", "model", "session"] {
            assert!(j.contains(k), "字段 {k} 必在：{j}");
        }
    }

    #[test]
    fn emit_then_report_roundtrip() {
        let d = tmp("rt");
        emit_into(&d, rec("claude", 1_700_000_000, 100, 50)).unwrap();
        emit_into(&d, rec("claude", 1_700_000_050, 10, 5)).unwrap();
        emit_into(&d, rec("codex", 1_700_000_100, 7, 3)).unwrap();
        let loaded = load(&d, None).unwrap();
        assert_eq!(loaded.records.len(), 3);
        let agg = aggregate(&loaded.records, None);
        assert_eq!(agg.len(), 2);
        let claude = agg.iter().find(|e| e.engine == "claude").unwrap();
        assert_eq!(claude.input_tokens, 110);
        assert_eq!(claude.records, 2);
        let only = aggregate(&loaded.records, Some("codex"));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].input_tokens, 7);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn day_bucketing_and_zero_ts() {
        let d = tmp("day");
        // ts=0 → 自动当时；day() 应等于落盘文件名
        emit_into(&d, rec("grok", 0, 1, 1)).unwrap();
        let files: Vec<_> = fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with(".jsonl"));
        let day = files[0].trim_end_matches(".jsonl").to_string();
        let loaded = load(&d, Some(&day)).unwrap();
        assert_eq!(loaded.records.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn malformed_lines_skip_not_explode() {
        let d = tmp("bad");
        fs::create_dir_all(&d).unwrap();
        fs::write(day_file(&d, "2026-10-31"), "{\"ts\":1,\"engine\":\"claude\",\"session\":\"\",\"model\":\"\",\"input_tokens\":1,\"output_tokens\":1,\"turns\":1,\"duration_ms\":1}\nnot-json\n{\"broken\":true}\n").unwrap();
        let loaded = load(&d, Some("2026-10-31")).unwrap();
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.malformed, 2);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn engine_validation() {
        let d = tmp("val");
        assert!(emit_into(&d, rec("", 1, 0, 0)).is_err());
        assert!(emit_into(&d, Record { engine: "bad\x01".into(), ..rec("x", 1, 0, 0) }).is_err());
        assert!(load(&d, None).unwrap().records.is_empty(), "没有记录文件=空账不是错");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(day_from_epoch(0), "1970-01-01");
        assert_eq!(day_from_epoch(1_700_000_000), "2023-11-14");
        assert_eq!(day_from_epoch(86_400 * 365), "1971-01-01");
        assert_eq!(day_from_epoch(86_400 * 366), "1971-01-02", "1970 非闰年");
        // 1972 闰年：day 730=1972-01-01，+31=Feb1，+29 → 1972-02-29 在册
        assert_eq!(day_from_epoch(86_400 * 789), "1972-02-29");
        assert_eq!(day_from_epoch(86_400 * 790), "1972-03-01");
        assert_eq!(day_from_epoch(1_767_225_600), "2026-01-01");
        assert_eq!(day_from_epoch(1_779_840_000), "2026-05-27", "=2026-01-01+146天");
    }
}
