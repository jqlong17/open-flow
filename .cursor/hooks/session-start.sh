#!/usr/bin/env bash
# sessionStart hook: 注入 CURRENT_STATE.md；若今日尚未更新过索引，则提示 AI 在本轮更新
# 输出: { "additional_context": "..." }

set -e
INPUT=$(cat)
WORKSPACE_ROOT=$(echo "$INPUT" | jq -r '.workspace_roots[0] // empty')
if [[ -n "$WORKSPACE_ROOT" && -d "$WORKSPACE_ROOT" ]]; then
  ROOT="$WORKSPACE_ROOT"
else
  ROOT="."
fi
STATE_FILE="${ROOT}/.cursor/memory/CURRENT_STATE.md"
STATE_DIR="${ROOT}/.cursor/hooks/state"
LAST_INDEX_DATE_FILE="${STATE_DIR}/last-index-date"
TODAY=$(date +%Y-%m-%d)

NEED_DAILY_PROMPT=0
if [[ -f "$LAST_INDEX_DATE_FILE" ]]; then
  LAST=$(cat "$LAST_INDEX_DATE_FILE" | tr -d '\n')
  [[ "$LAST" != "$TODAY" ]] && NEED_DAILY_PROMPT=1
else
  NEED_DAILY_PROMPT=1
fi

PREFIX="以下为项目当前状态快照，请优先参考。

---

"
SUFFIX="

---
"

if [[ $NEED_DAILY_PROMPT -eq 1 ]]; then
  SUFFIX="${SUFFIX}

【今日尚未做记忆更新】请在本轮对话中根据当前对话内容更新项目记忆：更新 .cursor/memory/CURRENT_STATE.md、.cursor/memory/sessions/ 及 .cursor/memory/INDEX.md（若有必要）。完成后将今日日期（${TODAY}）写入 .cursor/hooks/state/last-index-date（若目录不存在请先创建）。"
fi

if [[ ! -f "$STATE_FILE" ]]; then
  if [[ $NEED_DAILY_PROMPT -eq 1 ]]; then
    printf '%s' "$SUFFIX" | jq -Rs '{ additional_context: . }'
  else
    echo '{"additional_context": ""}'
  fi
  exit 0
fi

BODY=$(cat "$STATE_FILE")
printf '%s' "$PREFIX$BODY$SUFFIX" | jq -Rs '{ additional_context: . }'
exit 0
