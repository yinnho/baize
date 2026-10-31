# aginx-gateway

对外面守护（v0.2.0 换芯，2026-09-27 刀1）：真身=生态仓 aginx
（独立项目、专人开发，本仓只装配不仿制）aarch64-musl 静态原样上机。
8443 relay 长连 + ACP 对外面 + pair/auth 凭据面。v0.1.x 仿制品
（crates/gateway，单门 relay_secret、裸根可达）退役。

- **真身**：/var/lib/aginx/pkgfiles/aginx-gateway/bin/aginx-gateway
  （[service] cmd 直指）；面 /var/bin/aginx-gateway symlink（安装器
  自动带 .aginxmd）。
- **配置两坑（生态源码定案）**：
  1. 必须落 `/etc/aginx/config.toml`（0600：[relay] id/relay_secret、
     [server] access=private、[auth] jwt_secret）——CLI 子命令
     （pair/auth）**不认 -c**，走 find_default_config 四级候选，守护与
     CLI 必须同一份文件。relay 注册握手={id, token: relay_secret}，
     relay_secret 从旧 env 的 AGINX_RELAY_SECRET 搬入。
  2. **无 agent 拒启**：agents 目录空则交互式 setup（stdin EOF→跳过）
     →exit(1)→5 连崩断路器（停摆=显式 `aginx-svc start aginx-gateway`
     才醒）。首启前先铺 ≥1 个 $AGINX_DATA_DIR/agents/<名>/aginx.toml。
- **状态**：AGINX_DATA_DIR=/var/lib/aginx/gateway（绑定台账、丢件
  柜台、agents 注册表——装备入网一份 toml 即直通，刀3 铺）。
- **运维面**（都要带 AGINX_DATA_DIR env；在干净 CWD 跑，./aginx.toml
  会抢先）：`AGINX_DATA_DIR=... /var/bin/aginx-gateway pair / devices /
  unbind / auth / auths / revoke`（auth 发 scoped token：-n 客名 -a 点名
  allowed_agents -m 方法白名单 --system -e 天数）。

## 验证

- `aginx-svc status aginx-gateway` → ready
- /proc/net/tcp 8443（:20FB）有 ESTABLISHED（netstat 禁用——busybox 必炸）
- `AGINX_DATA_DIR=/var/lib/aginx/gateway /var/bin/aginx-gateway devices`
  列绑定台账（CWD 干净——CLI 走 find_default_config 找 /etc/aginx/config.toml）

## 换装（v0.1.x → v0.2.0，flat 裸包 → 树包跨形态）

relay id 唯一注册方——先 `aginx-svc stop aginx-gateway` 再装；装后
验 /var/bin/aginx-gateway 是 symlink（readlink 指 pkgfiles 真身）非
flat 旧件，旧 flat 件若残留即删（刀2 切流序全录 docs/HARDWARE.md）。

## 回滚

`aginx-pkg rollback aginx-gateway`（回 v0.1.x 仿制品=临时避难所——
仿制品无权限模型，正向路永远是修新包）

## v0.2.4 api-leg MCP（2026-10-30）

- 子命令：`aginx-gateway mcp-api-leg`（stdio MCP；引擎回合由适配器注入）
- 可见工具：`weixin_mp_token`、`weixin_mp_draft_add`（`freepublish` 等 admin 不进 list）
- 依赖机上 `/var/bin/aginx-api` + `/home/channels/*/leg/api-leg.toml`
- 验收：`printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | AGINX_HOME=/home /var/bin/aginx-gateway mcp-api-leg`
- 回滚：`aginx-pkg rollback aginx-gateway`（若无 .prev：从 `/var/lib/aginx/pkgfiles/aginx-gateway/bin/aginx-gateway.bak-<ts>` 拷回并 `aginx-svc restart`）

## v0.2.5 api-leg 细过滤（2026-10-30）

- 生态仓 `d99aba9`：`ToolFilter`（`AGINX_API_LEG_CHANNELS` / `AGINX_API_LEG_TOOLS` /
  `AGINX_API_LEG_INCLUDE_ADMIN` + 工位 `dependencies.api_legs`/`api_tools`）
- 默认 `tools/list` 仍只要非 admin（token+draft）；白名单可再收窄
- 验收：
  - 默认：`printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | AGINX_HOME=/home /var/bin/aginx-gateway mcp-api-leg`
  - 收窄：同上命令前加 `AGINX_API_LEG_TOOLS=weixin_mp_draft_add`
- 回滚：`aginx-pkg rollback aginx-gateway` 或从 `/var/lib/aginx/backup/aginx-gateway.bin.bak-*` 拷回并 `aginx-svc restart aginx-gateway aginx-gateway-local`

## v0.2.6 INCLUDE_ADMIN → --admin（2026-10-31）

- 生态仓 `379a0b4`：`mcp-api-leg` 仅当 `AGINX_API_LEG_INCLUDE_ADMIN=1`（或工位注入）时对 admin 工具透传 `aginx-api run --admin`
- 默认 list 仍无 freepublish；联检仍在 aginx-api（`publish_admin_gate`）
- 验收：
  - 默认：`tools/list` = token + draft
  - `AGINX_API_LEG_INCLUDE_ADMIN=1`：list 含 `weixin_mp_freepublish_submit`
