#!/usr/bin/env bash
# 同一音频分别用 Rust 和 Python 转写，对比输出（效果对齐用）
set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
AUDIO="${1:-$REPO_ROOT/testdata/mixed_zh_en.wav}"

if [[ ! -f "$AUDIO" ]]; then
  echo "Usage: $0 [path/to/audio.wav]"
  echo "  default: testdata/mixed_zh_en.wav"
  echo "File not found: $AUDIO"
  exit 1
fi

echo "=== 音频 ==="
echo "$AUDIO"
echo ""

# Rust：若有 OPEN_FLOW_MODEL 则传 --model
RUST_EXTRA=()
[[ -n "${OPEN_FLOW_MODEL:-}" ]] && RUST_EXTRA=(--model "$OPEN_FLOW_MODEL")

echo "=== Rust 转写 ==="
RUST_OUT=""
_run_rust() {
  local out
  out=$("$REPO_ROOT/target/release/open-flow" transcribe --file "$AUDIO" --output stdout "${RUST_EXTRA[@]}" 2>/dev/null)
  echo "$out" | awk '/转写结果/{getline; print; exit}' | sed 's/^   //'
}
if command -v "$REPO_ROOT/target/release/open-flow" &>/dev/null; then
  RUST_OUT=$(_run_rust)
fi
if [[ -z "$RUST_OUT" ]]; then
  cargo build --release -q 2>/dev/null || true
  RUST_OUT=$(_run_rust) || true
fi
echo "${RUST_OUT:-[无输出或未编译]}"
echo ""

echo "=== Python 转写 ==="
PY_OUT=""
if [[ -d "$REPO_ROOT/python/.venv" ]]; then
  # Python 使用默认 SenseVoiceSmall（不继承 OPEN_FLOW_MODEL，避免本地目录结构不兼容）
  PY_RAW=$(PYTHONUNBUFFERED=1 PYTHONPATH="$REPO_ROOT/python" env -u OPEN_FLOW_MODEL "$REPO_ROOT/python/.venv/bin/python" -u -m open_flow transcribe -f "$AUDIO" -o stdout 2>&1) || true
  # 转写结果：排除已知日志行后取最后非空行（FunASR 等会先打日志再 print 结果）
  PY_OUT=$(echo "$PY_RAW" | grep -v -E '^(funasr|Check |You are|Downloading|Loading|WARNING|  )' | awk 'NF{last=$0} END{print last}')
  [[ -z "$PY_OUT" ]] && PY_OUT=$(echo "$PY_RAW" | awk 'NF{last=$0} END{print last}')
fi
echo "${PY_OUT:-[未找到 python/.venv 或运行失败]}"
echo ""

echo "=== 对比 ==="
echo "Rust:   ${RUST_OUT:-—}"
echo "Python: ${PY_OUT:-—}"
