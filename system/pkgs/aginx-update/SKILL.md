# aginx-update

A/B 自更新器（M14/M22）：`status` / `apply` / `capture` / `write-part` / `sha256` / `check` / `idle` / `auto`。

## 升级 ≠ 重装（2026-10-31 钉死）

| 路径 | 何时用 | /home 与业务数据 |
|---|---|---|
| **`aginx-update apply <manifest>`**（无 `rootfs`） | **日常系统升版**（boot/vendor_boot/…） | **原位保留**（不写 userdata） |
| **`apply` + manifest.`rootfs`** | 换根镜像 | capture→state-restore；research/ops 另路导出 |
| **`devices/*/dist/flash.sh` `GO=1`** | **仅**裸箱/出厂 | 洗 userdata 前部＝重装，**禁止当升级** |

`capture` 只服务换根/可选 CAPTURE=1 预武；A/B-only apply **不会**调用它。

## 检查 + 空闲自动升（2026-10-31）

1. **频道**：`/etc/aginx/update-channel`（见 `update-channel.example`）第一非注释行 = 签名 manifest 的 https URL 或本地绝对路径；旁路 `.sig` 必验。
2. **`check`**：拉签名 manifest → 对比 `/var/lib/aginx/update/applied-version`（无则 `/etc/aginx-version`）→ 不同则写 `pending`；同则清 pending。无频道 = 安静退出 0。
3. **空闲门（`idle`）**：gateway / gateway-local **spool 无在途文件**，且无 `morning-report` / `day-review.sh` / `baize night` / `aginx-update apply` 进程。忙 → 跳过，下次 cron 再试。
4. **`auto`**（cron `37 3 * * *`）：`check` → 有 pending 且空闲 → `apply`（会 reboot）；`--dry-run` 只报到 will-apply。**永不**走 L0/flash。失败打日志，不吞错。

## 正常升级步骤

1. 宿主：`scripts/ota-manifest.sh <codename> <ver> <outdir> boot=<img> [vendor_boot=…]`（签 ed25519）
2. 把 manifest(+.sig) 与镜像放到机上可读路径（或 https），并写入 `/etc/aginx/update-channel`
3. 机上：`/var/bin/aginx-update apply /path/manifest.json`（或等 `auto`）
4. 重启进对槽；`status` 看 slot；抽查 `/home` 仍在
5. 失败：ABL tries 耗尽自动回滚仍成功的旧槽

## 验证

- `/var/bin/aginx-update status` → slot + boot 表
- `/var/bin/aginx-update check` / `idle` / `auto --dry-run`
- host：`cargo test -p aginx-update`

## 回滚

`aginx-pkg rollback aginx-update`（首装无 .prev 时报 no_prev，重 sync 即回）
