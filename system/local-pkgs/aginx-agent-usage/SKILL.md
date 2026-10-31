# aginx-agent-usage — 引擎用量归一契约

> 引擎是商品劳力（claude/codex/grok 各自的会话档形状各异），主人的账
> 只有一种：**一个引擎一天烧了多少**。一个 schema、一个追加口、一个
> 聚合面；一个引擎一个采集器，不发明第二种记录格式。

## 唯一记录形状（zeroed-not-null：缺数=0，永不 null）

```json
{"ts":0,"engine":"","session":"","model":"","input_tokens":0,"output_tokens":0,"turns":0,"duration_ms":0}
```

新字段只能加尾并给默认（旧行按缺 0 读）。日切 UTC（账的粒度是「一天」，
不承诺时区语义）。

## 用法

```sh
# 采集器腿（读引擎自己的会话档（只读）→ 摘增量 → emit）
aginx-agent-usage emit --engine claude --session <id> --model <m> \
  --in 1234 --out 567 --turns 2 --ms 4500        # --ts 缺省=当时

# 看账
aginx-agent-usage report                          # 全部天，按引擎聚合
aginx-agent-usage report --day 2026-10-31 --engine claude
aginx-agent-usage --json report                   # D1 信封
```

记录家：`/var/lib/aginx/usage/<日>.jsonl`，`flock(LOCK_EX)` 串行追加
（多采集器并发安全）；**只追加不改写**——谁家的记录谁负责。

## 采集器（后挂，形状已定）

| 引擎 | 读哪 | 状态 |
|------|------|------|
| claude | `~/.claude/projects/*.jsonl` 的 assistant 用量字段增量 | 待落（第一个真采集器） |
| codex / grok | 各自会话档 | 待落 |

规则：采集器**只读**引擎档；增量靠各自记住上次读到哪（游标家自管）；
计数只归一家（"counted once under whoever bills it"——同一次劳力别
两个采集器重复记账）；记录跨夜会话时挂 emit 当时，不回填。

## 机上验收

- emit 两条不同引擎 → report 两行聚合、数字对。
- 手塞一条坏行 → report 照常出、尾部「跳过坏行 1 条」。
- 缺 engine → rc 1 拒记。
