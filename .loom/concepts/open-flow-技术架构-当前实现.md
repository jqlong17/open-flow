---
created: 2026-03-19T02:09:13.744Z
updated: 2026-03-19T02:09:13.744Z
tags: architecture, technical, rust, asr, daemon, cli, cross-platform
category: concepts
status: active
---

# Open Flow 技术架构（当前实现）

# Open Flow 技术架构（当前实现）

## 1) 架构目标
Open Flow 的技术架构围绕以下目标设计：

- 低延迟语音输入（热键触发后快速进入录音/转写）
- 本地推理优先（隐私与离线能力）
- 跨平台可用（macOS 主体验，Windows/Linux 支持）
- 运行时稳定（后台常驻 + 明确状态机）
- 工程可维护（平台差异隔离、核心流程复用）

## 2) 架构分层
### 控制层（CLI）
由 `open-flow` 命令提供生命周期和运维入口：
- `start/stop/status`
- `setup`
- `model use/list`
- `transcribe`
- `config`
- `test-hotkey`

### 运行层（Daemon）
常驻进程负责核心业务链路：
- 全局热键监听
- 音频采集
- 状态机流转
- ASR 推理
- 文本注入（剪贴板/粘贴）
- 托盘状态同步

### 推理层（ASR）
SenseVoiceSmall ONNX 本地推理流程：
- fbank
- LFR
- CMVN
- ONNX Runtime 推理
- CTC greedy decode

## 3) 核心数据流
`Hotkey -> Recording -> Processing -> Clipboard/Text Injection`

更细化为：
1. 用户按全局热键进入录音
2. 音频以 16kHz mono f32 采集并写入 ring buffer
3. 停止录音后生成临时 wav
4. 预处理（fbank/LFR/CMVN）并进行 ONNX 推理
5. CTC 解码得到文本
6. 写入剪贴板，按平台策略自动粘贴或提示用户粘贴

## 4) 统一状态机
状态机统一为：
`Idle -> Recording -> Processing -> Idle`

- 热键事件触发状态切换
- 平台差异不改变状态机定义
- 托盘图标与状态机同步（Idle/Recording/Transcribing）

## 5) 跨平台实现策略
### macOS
- 热键：右 Command（CGEventTap）
- 事件循环：NSRunLoop
- 文本注入：剪贴板 + osascript Cmd+V
- 托盘：菜单栏 NSStatusItem

### Windows
- 热键：右 Alt（rdev AltGr）
- 事件循环：Win32 消息循环
- 文本注入：默认仅写剪贴板（用户 Ctrl+V）
- 托盘：系统托盘（tray-icon）

### Linux
- 热键：右 Alt（rdev AltGr）
- 事件循环：glib MainContext
- 文本注入：剪贴板 + xdotool/wtype
- 托盘：libappindicator（tray-icon）

## 6) 模型与预设
- `quantized`（默认，约 230MB）
- `fp16`（约 450MB）

模型按预设分目录存放，首次切换/使用时自动下载。ASR 加载时兼容 `model.onnx` 与 `model_quant.onnx` 命名。

## 7) 关键模块边界（代码）
- `src/main.rs`：CLI 入口与命令路由
- `src/daemon/mod.rs`：主业务编排
- `src/asr/`：预处理、推理、解码
- `src/audio/mod.rs`：录音采集
- `src/hotkey/mod.rs`：平台热键监听
- `src/text_injection/mod.rs`：文本注入与粘贴策略
- `src/tray/mod.rs`：托盘实现与状态同步
- `src/cli/daemon.rs`：daemon 生命周期控制
- `src/common/config.rs`：配置与跨平台路径

## 8) 关键工程决策
- 采用 `Daemon + CLI`：将常驻能力与控制面解耦
- 采用 `#[cfg(target_os = "...")]`：在边界层隔离平台差异
- 核心 ASR 链路统一：避免平台分叉导致行为不一致

## 9) 约束与风险点
- 平台权限/输入设备策略会影响热键稳定性（尤其 macOS 权限与 Linux input 组）
- 多平台事件循环维护成本较高（NSRunLoop/Win32/glib）
- Windows 自动粘贴能力受限于当前实现策略（默认手动 Ctrl+V）

## 10) 参考资料
- `docs/ARCHITECTURE.md`
- `README.md`
- `README.en.md`
