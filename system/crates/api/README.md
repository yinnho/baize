# aginx-api — D17 api 腿运行时

扫 `channels/<kind>/leg/api-leg.toml`，账户选择器读
`accounts/<id>/account.toml` → secretd JSON，执行声明 HTTP 工具。

## CLI

```
aginx-api list [--channel weixin-oa] [--include-admin]
aginx-api run <tool> --account <appid|nickname> [--channel weixin-oa] \
  [--param k=v ...] [--admin] [--show-token]
```

- stdout JSON 信封；`weixin_mp_token` 默认只打印 token **前缀**（`--show-token` 才完整，危险）。
- **admin ↔ publish_admin_gate 联检**：
  - `list` 默认隐藏 `admin=true`（freepublish）；`--include-admin` 才列出。
  - `run` 无 `--admin` → 拒（错误含频道 `publish_admin_gate` / `draft_only` 实值）。
  - `run --admin` 但频道 `publish_admin_gate=false`（或缺省）→ 仍拒；须
    `channel.toml` `[policy] publish_admin_gate = true` 才开放人审后门。
- 旁路：`aginx-mp-material-upload --account <号> [--type image] <file>` → `media_id`。

## `[tool.resolve]` 链式抽值

调用前按声明填参数（封面文件→upload→media_id→draft 等）：

| 键 | 作用 |
|----|------|
| `tool` + `param` + `extract` | 调同腿另一工具，抽前步输出 |
| `cli` + `cli_args` + `extract` | 调外部 CLI（multipart 上传） |
| `from_param` | 输入取自另一入参（默认=被 resolve 的键） |
| `from_account` | 账户 meta：`id` / `nickname` / … |
| `from_secret` | secretd JSON 字段 |
| `condition` | `missing` / `empty` / `is_file` / `always` / `not_empty` |

示例（draft 封面）：

```toml
[tool.resolve.thumb_media_id]
cli = "aginx-mp-material-upload"
from_param = "cover_file"
extract = "media_id"
condition = "is_file"
cli_args = ["--type", "image"]
```

## secretd policy

scope `weixin-oa:<appid>` 须授权本二进制真实路径，例如：

- `/var/bin/aginx-api`
- `/var/bin/aginx-mp-material-upload`

（及 `/usr/bin`、`/usr/libexec/aginx`、pkgfiles 同名路径）。**不要**再只写幽灵 `aginx-server`。
交互 `aginx-secret get` 仍应 denied。

## 环境

- `AGINX_HOME`（默认 `/home`）→ channels 根
- `AGINX_SECRET_SOCKET`（默认 `/run/aginx/secret.sock`）
- `AGINX_API_TOKEN_CACHE`（默认 `/run/aginx/api-leg-token`）

## 构建

```
cargo check -p aginx-api
cargo zigbuild --release --target aarch64-unknown-linux-musl -p aginx-api
```
