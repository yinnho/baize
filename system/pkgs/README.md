# baize/system/pkgs — 配方快照（自有，非封仓桥）

本目录是 **baize 树内自有**的包配方副本（`pkg.toml` / `SKILL.md` / 少量
`files/`），供 `build-pkg.sh` / `build-rootfs.sh` OPT_ADD 在缺
`local-pkgs/<名>` 时读取。

## 律

- **不是** `aginxos-next/pkgs` 的符号链接；pack/烤盘不得再解析进封仓。
- **不改封仓源码**：需要跟进上游配方时，人肉 rsync 快照进本目录（一次性
  拷贝），或把 baize 特供变更放进 `system/local-pkgs/`（overlay 优先）。
- **lookup 序**：`local-pkgs/<名>` → `pkgs/<名>`（见 `scripts/build-pkg.sh`、
  `scripts/build-rootfs.sh`）。例：`aginx-gateway` / `aginx-api` 真源在
  `local-pkgs/`，本目录里的同名条目只是上游快照，烤盘不会读到它。

## 来源戳

- 2026-10-31：拆除 `system/pkgs → aginxos-next/pkgs`；自
  `aginxos-next/pkgs` 拷入本树（约 268K，26 包）。此后 inode 与封仓分离。
