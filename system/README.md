# system/ — 平台源码快照（骨）

AginxOS→baize 二代封仓树（aginxos-next，tag `seal` 2026-10-27）的**源码
快照**——烤出 rootfs v0.0.2（release `rootfs-v0.0.2-redfin.img.gz`，
血统 sha f9c5c53）的那副骨。三代起本仓为真源续长。

- `crates/` — Rust 工作区：`aginx`（路由/系统脸）、`aginx-server`、
  `aginx-pkg`（四件套包工）、`aginx-svcd/svc`（监督者）、`aginx-sign`
  （ed25519 签名）、hwd（D14 机器档案单源）、voice/call/channels/term/
  wizard/qr/pair/download/update/done/secret + carrier 遗产件。
- `rootfs/` — 镜像配方（L0：kernel+init+svc+network+ssh+pkg，其余皆包；
  busybox/字体/C 源在树内）。
- `scripts/` — 烤链 `build-rootfs.sh`（`DEVICE=<codename>`）+ 验收套件。
- `devices/` — 机器即数据（D14）：`device.toml`/`modules.txt`/`boot/`
  配方/`cam/`。一台机器一个目录，boot 面随机器、rootfs 之上复用。

**不在此快照**（永不上架）：boot 侧二进制（vendor blob——真源在
`.local/device/<codename>/`，再生路径见 `devices/<codename>/boot/assets.md`）、
bringup 收据层、构建产物（`target/`）、任何秘密（串号/密钥/口令经
发布前审计门逐类扫描，零命中——测试桩除外）。

烤整机镜像：`DEVICE=redfin ./scripts/build-rootfs.sh`（需自备
`.local/` 资产；boot 件配方见 `devices/`）。License=Apache-2.0（同仓）。
