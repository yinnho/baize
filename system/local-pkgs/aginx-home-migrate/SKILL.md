# aginx-home-migrate — /home 人格树迁移层

> 人格树（/home）没有发版号，但格式会长。本工具给「格式演进」一条可
> 验收的正路：**稿住人格树、标记住机身、系统自己跑**。

## 分层（不许混）

| 件 | 住哪 | 谁供 |
|----|------|------|
| 迁移稿 `NNN-slug.sh` | `${AGINX_HOME}/migrations/` | 改「人」的件（随人格备份/认祖走；duphub 可分发） |
| 标记 | `/var/lib/aginx/home-migrate/` | 这台身体施过哪些法（epoch+sha256） |
| runner（本包） | `/var/bin/aginx-home-migrate` | 机身件（pkg 供应链） |

## 用法

```sh
aginx-home-migrate list            # 全部迁移 + 现状（applied/pending/EDITED）
aginx-home-migrate check           # rc 3 = 有 pending 或 EDITED（夜巡挂账）
aginx-home-migrate run             # 顺序施待跑；失败即停不落标记
aginx-home-migrate run --dry-run   # 只列计划
aginx-home-migrate paths           # 排障：目录解析
```

## 写稿纪律（铁律）

1. **幂等**：认祖到新身体 = 标记空 = 全部重跑。姿势 `[ -e 新 ] || mv 旧 新`。
2. **编号即顺序**：`NNN-slug.sh`，001–999 升序施；撞号/坏名 = 目录拒读。
3. **失败非零即停**：失败者不落标记（下次自然重试）；后面的不动。
4. **发布后不改稿**：标记 sha 对不上 = `EDITED`，整队拒跑；要改取新编号。
5. **只改 /home**：改机身的活走 pkg 供应链，不写迁移稿。
6. 稿运行环境：`sh`，cwd=`$AGINX_HOME`，env 带 `BAIZE_HOME`，PATH 前置
   `/var/bin`（可调面 CLI）。

## 谁跑（正路）

provision/boot **永不代跑**（HOME-PHASE Finalize 禁 silent /home 写）。
夜巡门腿 `baize-gate` 先只挂账（`check` rc 3 → 黄，进当日材料）；系统
自己在会话/夜巡里发起 `run`——人格树是它自己的事。

## 机上验收

- 空人格树 `list` = 「无迁移」rc 0（出生绿门）。
- 写 `001-touch.sh`（幂等姿势）→ `run` → 标记在、重跑空手而归。
- 故意 `exit 7` → 停队、无标记、后面没跑；修稿重跑只补后面的。
- 施过后改稿 → `EDITED` 拒跑。
