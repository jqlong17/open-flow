# Open Flow 当前状态快照

> 每次对话结束时更新。AI 通过 Cursor sessionStart hook 自动读取本文件。
> 最后更新：2026-03-13

---

## 状态：可用（功能完整，模型需手动配置）

## 已完成功能

- ✅ Rust ASR 管线（SenseVoiceSmall ONNX），效果与 Python FunASR 基本一致
- ✅ 热键监听（macOS 右侧 Command 键，基于 rdev CGEventTap）
- ✅ 实时录音（cpal，按键开流/停流）
- ✅ 文本注入（arboard + Cmd+V，transcribed text 保留在剪贴板）
- ✅ 后台 daemon（setsid 脱离终端，PID 文件管理，SIGTERM/SIGKILL 停止）
- ✅ CLI 完整：start/stop/status/transcribe/config
- ✅ 知识索引系统（`docs/INDEX.md` + Cursor hooks 注入上下文 + 手动触发更新）
- ✅ Cursor Hooks 配置（`sessionStart` 自动注入状态，并在每日首次对话时提示更新索引）
- ✅ 元规则（`.cursor/rules/memory-system.mdc`，指导 AI 何时/如何更新记忆）

## 当前最大阻塞

**模型下载（P0）**：SenseVoiceSmall 无官方 ONNX 公开下载 URL，用户需要手动用 FunASR 导出，详见 `docs/MODEL_DOWNLOAD_TODO.md`。

## 性能数据（M3 Pro，2025-01-实测）

| 版本 | 转写耗时（~5s 音频） |
|------|-------------------|
| Python (FunASR) | ~2.5s（含首次模型加载） |
| Rust (ORT)      | ~82ms（模型已在内存） |

## 关键配置值

```toml
# ~/Library/Application Support/com.openflow.open-flow/config.toml
model_path = "~/Library/Application Support/Shandianshuo/models/sensevoice-small"
hotkey = "MetaRight"
```

## 最近修改的文件

- `README.md` — 重构为项目首页：突出开源定位、核心特点、Rust/Python 分工、快速开始、当前限制与文档入口
- `docs/ARCHITECTURE.md` — 从仓库根目录移入 docs/，并修正剪贴板/粘贴描述与当前实现一致
- `docs/INDEX.md` — 新增「4. 架构与设计」层级，收录 ARCHITECTURE.md；3.1 增加 docs/ARCHITECTURE.md；3.3 更新为仅 sessionStart
- `.cursor/hooks.json` — 仅保留 sessionStart
- `.cursor/hooks/session-start.sh` — 增加「每日首次对话」未更新索引时的提示逻辑
- `.cursor/rules/memory-system.mdc` — 更新触发方式（更新索引 / 每日首次）
- 其余见上文及 sessions 记录

## 下一步计划

1. **P0** 解决模型自动下载（Python export script 或 HuggingFace 直链）
2. **P0** 模型预热（daemon 启动时在后台加载 ORT session）
3. **P1** 录音状态视觉反馈（菜单栏图标变色）
4. **P1** 打包为 macOS .app

**建议推进顺序**：先做模型预热（改动小）→ 再定模型下载方案并实现 → 然后 P1 体验（菜单栏/状态/提示音）→ 最后 .app 打包与 Homebrew。
