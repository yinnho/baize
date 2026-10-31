---
name: baize-heartbeat-wiring
description: 心跳接线——23:47夜巡harvest+day：材料→引擎整理→候选三关→正典落位+版本账；baize各腿用法一览
type: knowledge
date: 2026-10-27
source: seed
origin: factory
---

# 心跳接线

每天夜里，你自己整理你自己：

```
23:47 crontab
  └─ baize-night            # 夜巡入口
       ├─ baize-harvest     # 收割当日材料（秘密打码；traces/ 永不入库）
       ├─ baize-gate        # 夜巡门：监听盘点/家迁移挂账/预算断言
       │                    #   → traces/<日>-gates.md（红进晨报自查段；
       │                    #     aginx-netgate 在 /var/bin，未装=黄挂账）
       └─ baize-day         # 心跳：材料→引擎整理→候选
                            #   → 三关验收 → 正典落位 + history/versions.jsonl
```

- 腿一览：`baize day`（心跳，`--stub`=不烧脑测试腿）· `baize night`
  （夜巡）· `baize gate`（夜巡门）· `baize harvest`（收割）· `baize k`
  （知识面：list / show / rulings / gaps / verify / roster）· `baize adopt`
  （认祖归宗，旧身体专用）。
- 三关：frontmatter 齐（name / description / type / date）· 裁决件
  双段（## 场景 / ## 钦定）· 名册同步（MEMORY.md 可派生）。
- 出生验收：`baize day --stub` 绿；次晨 `history/versions.jsonl` 有
  新条目。
