# baize · 白泽

白泽是一个人的化身——不是操作系统，不是聊天机器人。机器是它的身体
（可换），引擎是它的劳力（可换），记忆是它（不可换）。

名字命给人，不命给机器；所以叫 baize，不加 OS。

本仓是白泽的人格核：

| 件 | 是什么 |
|----|--------|
| `bin/baize` | 唯一裸命令：`baize <day\|night\|harvest\|adopt\|k>` 派发各腿 |
| `bin/baize-harvest` | 收割当日会话真源（session log = truth source）为材料；秘密模式一律打码 |
| `bin/baize-day` | 心跳：材料交给引擎（codex 等）整理，候选过三道验收门（frontmatter / description 非空 / name=文件名）才落正典；考卷（ruling）必含「场景」「钦定」双段 |
| `bin/baize-k` | 知识面：`list / show / rulings / gaps / verify / roster` |
| `check.sh` | 主机绿门：正典体检 + 合成日心跳（stub，不烧脑）全链断言 |

只有 sh 与 jq。**文件面即数据模型，db 是弯路。**人格树（正典、名册、
版本账）与当日材料留在本地宿主，不进公开历史——记忆是私产。

## 文档三树（omacom 对照 · baize 分层）

对人 / 对 agent / 对系统——三入口，禁止混装：

| 树 | 入口 | 受众 | 放什么 |
|----|------|------|--------|
| **对 agent** | [`AGENTS.md`](AGENTS.md)（仓规）+ [`home/AGENTS.md`](home/AGENTS.md)（人格）+ [`home/knowledge/`](home/knowledge/) | 干活的 agent | 规程、「做 X 先读」、正典；安装全文见 [`manual/01-install.md`](manual/01-install.md) |
| **对系统** | [`docs/README.md`](docs/README.md) | 改机身 / 读形状 | DESIGN·PRODUCT·需求·进化轨；骨在 [`system/README.md`](system/README.md) |
| **对人** | [`manual/README.md`](manual/README.md) | 主人 | 只写人要做的三事（递机器 / 递秘密 / 收汇报）；**禁止**机身内幕 |

归位细则写在各入口 README。不搞大搬家：短索引就位，正文各留原处。
