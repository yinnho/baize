# ffmpeg — 媒体处理 CLI（7.0.2 全静态）

ffmpeg + ffprobe 双 CLI，PATH 面（`/var/bin/ffmpeg`、`/var/bin/ffprobe`），
无守护进程。johnvansickle arm64 全静态原样，musl/glibc 通吃。

## 何时用

- **抽帧**（封面/缩略图）：`ffmpeg -ss 00:00:05 -i in.mp4 -frames:v 1 -y out.png`
- **转码/压码**：`ffmpeg -i in.mov -c:v libx264 -preset veryfast -c:a aac -y out.mp4`
- **音频抽取**：`ffmpeg -i in.mp4 -vn -c:a libmp3lame -y out.mp3`
- **探测**：`ffprobe -v error -show_entries format=duration,format_name -of csv=p=0 in.mp4`
- **缩略链**：先 ffprobe 读时长，再 `-ss <时长/2>` 抽中点帧。

## 注意

- 本机 grep 实为 ugrep、pkill 全哑（redfin busybox 雷区族）——管道写法保守。
- 转码吃满核：大活放夜里心跳窗跑，别与取材/渲染腿抢时段。
- HTML→PNG 封面渲染不走 ffmpeg——那是 diting `/screenshot` 的腿。
