# aginx-api — D17 api 腿运行时

扫 `channels/<kind>/leg/api-leg.toml`，经账户选择器读 secretd JSON，执行声明式 HTTP 工具。旁路 CLI `aginx-mp-material-upload` 做 multipart 封面。

## 面

| 命令 | 作用 |
|------|------|
| `aginx-api list [--channel] [--include-admin]` | 列工具；默认隐藏 admin |
| `aginx-api run <tool> … [--admin]` | 跑工具；stdout JSON 信封 |
| `aginx-mp-material-upload --account <号> [--type image] <file>` | 上传素材 → `media_id` |

## v0.1.1（2026-10-30）

- **联检**：`admin=true` 须 `--admin` **且** 频道 `publish_admin_gate=true`
- **list**：无 `--include-admin` 不见 freepublish；输出含 policy 摘要
- **`[tool.resolve]`**：账户 / secretd / 前步工具 / CLI 链式抽值

## 验证

- `list --channel weixin-oa` → 仅 token/draft；`--include-admin` 才见 freepublish
- `run weixin_mp_freepublish_submit …` 无 `--admin` → 拒（含 gate 实值）
- `run … --admin` 在 `publish_admin_gate=false` 频道 → 仍拒

## 回滚

`aginx-pkg rollback aginx-api`
