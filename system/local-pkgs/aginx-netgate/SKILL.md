# aginx-netgate — 监听盘点门

> NETWORK-SURFACE v2 的 P0 常驻断言：**清单外多一个 TCP 监听口即红**。
> L0 的缩面是构造性的（镜像 svc.d 只两单元）；本门把它变成每晚可验收的账。

## 用法

```sh
aginx-netgate                 # 盘点 /proc/net/tcp{,6} LISTEN，判红绿
aginx-netgate --list          # 只盘点不判（谁在听、听哪个面、inode）
aginx-netgate --require       # 清单写了没人听也算红（收紧后证明口起来了）
aginx-netgate --json          # D1 信封（verdict/listeners/offenders/…）
```

rc：`0` 绿 / `3` 红（查询的合法答案） / `1` 盘点不能（无 /proc、坏清单行） / `2` usage。

## 清单（谁说了算）

union 三源，都可选：

| 源 | 谁写 |
|----|------|
| `/etc/aginx/listeners.allow` | 平台根正本（镜像烤入；人/ops 手改） |
| `/etc/aginx/listeners.allow.d/*.allow` | drop-in（按名序） |
| `/var/lib/aginx/pkgfiles/*/netgate.allow` | **包自表**——开监听的包在自己的 files 树里自带 |

行语法：`port@addr`，`#` 起注释。addr：`*` / `0.0.0.0` / `::` = 口放行
不限面；具体 IP = 只许那个面（绑 any 或别的面都红——**清单即策略**）。
无策略 + 有监听 = 全员红（fail-closed）；坏行拒判不跳过。

## 收口目标（P0，收完删本段）

- 8089（引擎面）/8686（gateway prompt face）→ `@127.0.0.1`：先把面上
  bind 改 loopback（8089 若无 knob = 向引擎线提需求；8686 走 gateway
  config），再把正本里 `@*` 行换成 `@127.0.0.1`，`--require` 验口在。
- ssh 22：不关；LAN bind + 仅 pubkey 是 ops 腿（动 ssh 的宪法地位另行裁决）。

## 夜巡接线

`bin/baize-gate`（baize 包）每晚清点：rc 3 → 红进当日 traces 材料直通
晨报自查段；rc 1（盘点不能）→ 黄（当查）；未装 → 黄（挂账）。

## 机上验收

- 故意在清单外开一个口（`nc -l` 类）→ 夜巡红，晨报见。
- 清单里写 `8089@127.0.0.1` 而 8089 绑 `0.0.0.0` → 红，原因行直说面不符。
- 三源任一写坏行 → rc 1 拒判（不是装绿）。
