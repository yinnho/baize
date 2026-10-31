# T2 夜巡能力候选检查清单（规程）

1. harvest 是否写出 `traces/<日>-capability-hints.md`？（cues + fingerprints；密钥须 «REDACTED»）
2. 同类 bag 跨日 ≥ `t2_repeat_threshold`（默认 2）或主人「下次照这样」cue？否 → 无证据不提案。
3. `capability_t2_scan`：闸关 → 只报告；闸开 → `run/capability-proposals/t2-*.md`（trigger=T2, status=proposed）。
4. **禁止** mkdir `workflows/<新面>` / 写名册；`--stub` 不跑扫描。
5. 采纳：主人确认后 `capability_accept`（≤L1）。
