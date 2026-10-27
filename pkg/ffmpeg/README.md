# ffmpeg 包配方（钉表）

- 源：johnvansickle release **arm64-static**，ffmpeg **7.0.2**
  https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz
  上游 md5 `807afe21601db0a73e426121c7d636ea`；裸件 sha256：
  ffmpeg `6bb182d0d75d23028db82e9e4f723ca69b853d055698486e6984ddb2c06fb8ce`
  ffprobe `d17ae9b4c297d48e2521ba14e417bb0537c6ff77c584cdbcd6bb0d8d0307a2e8`
- 4pc：GitHub yinnho/baize release v0.0.5 `ffmpeg-v7.0.2-4pc.tar`
  sha256 `b29884576643c6f0d717ef6a8459c60d12ff5e5d8b198094e4f8b194fd2294c4`；
  镜像 REL5（sync.sh fetch5）+ 双侧 manifest 钉行。
- 全静态（musl/glibc 通吃），PATH 面 CLI 无守护；烟测=抽帧/转码
  (x264+aac)/ffprobe 探测（RECEIPT-exam §14，2026-10-28）。

## 升版

1. 换上游 tar.xz（md5 对表）→ 重抽 ffmpeg/ffprobe → stage `chmod +x`
   → 重打 4pc（**tar 内 mode 即真理**，换 binary 五律⑥）→ sha。
2. `gh release upload`（新 tag 或同 tag）→ sync.sh 改钉行跑 → 双侧
   manifest 改钉 + aginx-sign 重签 → 设备 `aginx-pkg install ffmpeg
   <tar> <sha>`。

## 已知缺口（挂账）

- **多面缺口**：`exec` 只铸一条脸——ffprobe 的 `/var/bin` 软链 +
  `.aginxmd` sidecar 是装机时手补的，**reflash 即丢**（CAPTURE 丢
  faces 案族，#427 立案的同一个结构性堵）。正法=aginx-pkg 支持
  exec 面列表；三代 baize install 薄腿一并裁。
