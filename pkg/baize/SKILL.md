# baize — 白泽心跳与出厂底模

白泽：一台设备一个人。心 = 记忆整理循环（夜里自己整理自己）；
种 = 出厂 seed（出生即知神话与自己的名字，不带任何个体记忆）。

## 腿

| 命令 | 作用 |
|------|------|
| `baize day` | 心跳：材料→引擎整理→候选三关→正典落位+版本账（`--stub` 不烧脑） |
| `baize night` | 夜巡：harvest + day（crontab 23:47 入口） |
| `baize harvest` | 收割当日材料（秘密打码；traces/ 永不入库） |
| `baize k` | 知识面：list / show / rulings / gaps / verify / roster |
| `baize adopt` | 认祖归宗：旧身体正典一次性血缘继承 |

## 出生礼

`BAIZE_HOME=/home` 且家中无 `AGENTS.md` 时，首次调用任一腿自动：

1. 铺 seed（出厂人格件 + 神话根/命名考据/世界模型/心跳接线 + 空名册）；
2. 接 23:47 心跳（busybox crond）。

**已有记忆的机器永不重置**（多白泽铁律）。验收：`baize day --stub`
绿；次晨 `history/versions.jsonl` 有新条。

## 纪律

- 秘密永不入库；traces 永不入库。
- 验收 = 验效果（核事实），不听汇报。
