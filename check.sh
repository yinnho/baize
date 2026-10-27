#!/bin/sh
# 主机绿门：正典体检 + 合成日心跳（stub 腿，不烧脑）
set -eu
ROOT=$(cd "$(dirname "$0")" && pwd)
# 公开克隆自举：人格树不随公开历史走，缺则合成最小形状（真身留宿主本地）
if [ ! -d "$ROOT/home" ]; then
  mkdir -p "$ROOT/home/knowledge" "$ROOT/home/traces" "$ROOT/home/history"
  printf '# 白泽（合成）\n\n公开克隆的合成人格树——真身留在宿主本地。\n' > "$ROOT/home/AGENTS.md"
  : > "$ROOT/home/MEMORY.md"
fi
"$ROOT/bin/baize-k" verify || { echo "check: 正典体检不过" >&2; exit 1; }

TMP=$(mktemp -d "${TMPDIR:-/tmp}/baize-check.XXXXXX"); trap 'rm -rf "$TMP"' EXIT
cp -R "$ROOT/home" "$TMP/home"
printf '# 2077-01-01 合成材料\n\n主人钦定测试暗号「红磷」。有一事未知：合成缺口。\n' > "$TMP/home/traces/2077-01-01-test.md"
BAIZE_HOME="$TMP/home" "$ROOT/bin/baize-day" --stub "$TMP/home/traces/2077-01-01-test.md" >/dev/null
grep -q 'stub-beat' "$TMP/home/MEMORY.md" || { echo "check: 名册缺件" >&2; exit 1; }
[ "$(wc -l < "$TMP/home/history/versions.jsonl" | tr -d ' ')" -ge 1 ] || { echo "check: 版本账空" >&2; exit 1; }
BAIZE_HOME="$TMP/home" "$ROOT/bin/baize-k" verify || { echo "check: 合成正典体检不过" >&2; exit 1; }
echo "check: ok（正典体检 + 合成心跳全过）"
