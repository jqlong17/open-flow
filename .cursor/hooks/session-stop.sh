#!/usr/bin/env bash
# stop hook: 对话结束时触发一次 follow-up，请 AI 更新记忆文件
# 输入: JSON (status, loop_count 等)
# 输出: { "followup_message": "..." } 仅当 loop_count=0 时返回，避免循环

INPUT=$(cat)
LOOP_COUNT=$(echo "$INPUT" | jq -r '.loop_count // 0')
STATUS=$(echo "$INPUT" | jq -r '.status // "completed"')

if [[ "$LOOP_COUNT" != "0" ]]; then
  echo '{}'
  exit 0
fi

# 仅当正常完成时请求更新记忆
if [[ "$STATUS" == "completed" ]]; then
  MSG="请根据本次对话，更新项目记忆：1) 若状态有变化，更新 docs/CURRENT_STATE.md 的「已完成功能」「当前最大阻塞」「最近修改的文件」「下一步计划」等小节；2) 若有重要决策或实现，在 docs/sessions/ 下追加或新建本次会话摘要，并更新 docs/sessions/INDEX.md。完成后无需再回复。"
  echo "{\"followup_message\": $(echo "$MSG" | jq -Rs .)}"
else
  echo '{}'
fi
exit 0
#!/usr/bin/env bash
# stop hook: 对话结束时触发一次 follow-up，请 AI 更新记忆文件
# 输入: JSON (status, loop_count 等)
# 输出: { "followup_message": "..." } 仅当 loop_count=0 时返回，避免循环

INPUT=$(cat)
LOOP_COUNT=$(echo "$INPUT" | jq -r '.loop_count // 0')
STATUS=$(echo "$INPUT" | jq -r '.status // "completed"')

if [[ "$LOOP_COUNT" != "0" ]]; then
  echo '{}'
  exit 0
fi

# 仅当正常完成时请求更新记忆
if [[ "$STATUS" == "completed" ]]; then
  MSG="请根据本次对话，更新项目记忆：1) 若状态有变化，更新 docs/CURRENT_STATE.md 的「已完成功能」「当前最大阻塞」「最近修改的文件」「下一步计划」等小节；2) 若有重要决策或实现，在 docs/sessions/ 下追加或新建本次会话摘要，并更新 docs/sessions/INDEX.md。完成后无需再回复。"
  echo "{\"followup_message\": $(echo "$MSG" | jq -Rs .)}"
else
  echo '{}'
fi
exit 0
