# 安装正典 — 白泽机器从零到活（agent 手册 · v1）

> 读者 = **agent**。人只出现在三处：递机器、递秘密、收一句话汇报。
> 加粗 = 铁律。每步带验收：**验收 = 验效果（核事实），不是听汇报**。
> 【待落】= 尚未就位的件，正典诚实记账，不许假装存在。

## 0. 你需要什么

- 一台 aarch64 机器（现役配方：redfin / panther-x2；换新机器见附录 A）
- GitHub `yinnho/baize` 两个 release：
  - `v0.0.1`：26 件货架（机器上一切可装件）
  - `v0.0.2`：`rootfs-v0.0.2-redfin.img.gz`（L0 底座；per-device，X2 件【待落】）
- 派生镜像（国内快路，同 sha256）：`https://pkgs.aginx.net`
- 主人的秘密（§6，一次手递）

## 1. 安全门（多机环境铁律）

- **destructive fastboot 前必核串号**：`fastboot devices` 输出对上目标机
  serial 才许继续；串号真源 = 主人手递，永不入仓。
- 工作台可能有邻居设备（不同机器的 fastboot/adb 面身份不同）；对不上就
  停手问主人，**绝不猜**。
- 刷 userdata = 毁数据：**主人确认过备份才许动**。

## 2. 取件与核验

```sh
gh release download v0.0.2 -R yinnho/baize -p 'rootfs-v0.0.2-redfin.img.gz'
shasum -a 256 rootfs-v0.0.2-redfin.img.gz   # 对 SHA256SUMS
gunzip rootfs-v0.0.2-redfin.img.gz
shasum -a 256 rootfs-v0.0.2-redfin.img      # raw: bec8479f…
```

- GitHub 资产腿超时（已知病）：走 `https://gh-proxy.com/` 前缀，或镜像。
- **sha 钉内容，传输不承信**——换路取件后照核不误。

## 3. boot 件（永不上架，自配）

- rootfs 之外每机还要 boot 面；redfin = vendor_boot（吃 vendor blobs，
  法律不可开源，**永远不上架**）。
- 正路两条：机器自带 backup 件优先（保机稳）；否则从官方 vendor 按配方
  长出（配方 = `devices/<codename>/boot/`，随 system/ 快照入公开面【待落】）。
- 存一份 recovery 件再动手（redfin：stock-vendor_boot.img）。

## 4. 刷（redfin 为例）

- 进 fastboot：`adb reboot bootloader`（或关机后 Power+VolDown）。
- 顺序：`fastboot flash userdata rootfs-…img` → `fastboot flash
  vendor_boot <boot件>` → `fastboot reboot`。
- 验收：300s 内 adb 见设备；600s 内设备侧 `/run/boot.state` = done。

## 5. 首启配网 + ssh

- adb 写 `/etc/wifi.conf`（ssid/psk）+ 改密：`busybox chpasswd -c sha512`
  （**裸 chpasswd = des 截断坑**）或推 authorized_keys。
- 重启（设备上 `adb reboot` 有挂死前科，用 `aginx-reboot reboot`）。
- 验收：ssh 通（`-o BatchMode=yes`）。DHCP 地址无稳定绑定——**首探验
  身份**：`uname -a` + `lsmod` 对上机型独有事实，才认这台是你要的机器。

## 6. 递秘密（§9 律：秘密永不入仓库/备份/GitHub）

- brain key → `/etc/aginx/env`（`AGINXBRAIN_API_KEY`）
- relay + jwt → gateway `config.toml`
- codex / grok → 设备码登录，一次性
- 代理腿才要：stunnel PSK → `/etc/stunnel/`
- **回显 = 违律**；写完即 `chmod 600`。

## 7. 装件（opt-in，L0 什么都不预装）

- `aginx-pkg sync` → 按需 `aginx-pkg opt-in <name>`。
- 最小活机闭包：`aginx` + `bash` + `claude`（或 codex/grok）+ `diting`
  + `python3` + `git` + `sqlite3` + `jq`。
- 验收：命令面齐全（`aginx commands`）；每个包的 face 落位（/var/bin +
  `.aginxmd` 边车）。

## 8. 出生礼（seed → home，只落空家）

- baize 包（心跳腿 + seed）= v0.0.3（baize v0.1.0），装法同上 opt-in；
  显式路径收装【待落 manifest 复钉】。
- 落座条件门：`/home` 无人格树才铺 seed；**已有记忆的机器永不重置**
  （多白泽铁律：一台设备一个人，各长各的）。
- 旧身体搬家才用 `baize adopt` 认祖归宗——一次性血缘继承，不是同步。
  （redfin 首考钦定走此路：**记忆要存在**——见附录 A 换壳节。）
- 验收：`baize day --stub` 绿；次晨版本账有新条目。

## 9. 收尾

- 设备行为变化必留收据（机器侧账本）。
- 终态已知才算完：单元 ready、心跳接线（crontab 23:47）在位。
- 给主人一句话汇报：机器名、装了什么、验了什么、缺什么。

## 附录 A：换一台机器（机器 = 数据，D14）

- 新机器 = 新 codename = `devices/<codename>/` 一个目录：`device.toml`
  + `modules.txt` + `boot/` 配方；boot 面不同（UEFI/eMMC vs
  fastboot/bootimg），rootfs 之上的一切原样复用。
- 烤：`DEVICE=<codename> ./scripts/build-rootfs.sh` → 按
  `rootfs-v<版>-<codename>` 发 release。
- **先在废机上考**，再上生产机。

### 旧身体换壳（同机刷机保记忆）

刷机抹 `/home`，但**这台机器的人不跟着死**（钦定 2026-10-27：
redfin 记忆要存在）：

1. **刷前导出**（设备在线时）：整树打包 `/home`（人格+workflows+
   记忆），旧记忆库若有 kv/库先导出成 `knowledge/*.md`（db 要
   db+wal+shm 三件一起拉）。导出件存别处（Mac/新家之外），秘密
   不在导出件里（§9 律）。
2. 刷 v0.0.2+ → §5–§7 → opt-in baize → 出生礼（空家，新身体）。
3. **认祖归宗**：`baize adopt <导出的knowledge目录>`——旧正典
   归一化认领进新家，一次性血缘；过不了三关的进 rejects 报告，
   不静默丢。
4. 验收：名册里既有出厂四件、也有认领来的旧件（血缘可见）。

## 附录 B：已知设备雷区（踩过才写下的）

- redfin busybox：awk/netstat/wget/diff 必 SIGSEGV——脚本只用
  sed/`set --`；杀进程用 `pkill -x`/`pidof`。
- adb 推复合命令（含中文/JSON）会静默零执行——拆单 + 验副作用。
- 换 /usr/bin 里的 binary 四律：readlink 真身、staging 同盘、验 md5、
  换装必真重启。
- 多地点 Wi-Fi：每 ssid/psk 记在机器上，别抄 Mac。
