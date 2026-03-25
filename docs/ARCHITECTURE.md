# Open Flow Architecture

## Overview

Open Flow 是一个**跨平台**开源语音输入工具（macOS 为主、Windows / Linux 支持），采用 `Daemon + CLI` 架构。核心设计理念：**后台常驻、全局热键、本地推理、快速响应**。平台差异通过 `#[cfg(target_os = "...")]` 隔离，**Mac 体验与代码路径独立**，不受 Win/Linux 实现影响。

---

## Platform Matrix（平台差异速查）

| 能力 | macOS | Windows | Linux |
|------|-------|---------|-------|
| **热键** | 右 Command（CGEventTap） | 右 Alt（rdev AltGr） | 右 Alt（rdev AltGr） |
| **托盘** | 菜单栏图标（灰/红/黄 + 菜单） | 任务栏系统托盘（同三态 + 菜单） | 系统托盘（libappindicator，同三态 + 菜单） |
| **事件循环** | NSRunLoop（主线程） | Win32 PeekMessage/DispatchMessage | glib MainContext.iteration |
| **文本注入** | 剪贴板 + osascript Cmd+V | 仅剪贴板，提示用户 Ctrl+V | 剪贴板 + xdotool/wtype Ctrl+V |
| **粘贴依赖** | 无（系统自带） | 无 | xdotool（X11）或 wtype（Wayland） |
| **发版产物** | CLI + .app | CLI zip | CLI tar.gz |

---

## System Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                          User Space                             │
│                                                                  │
│  ┌─────────────┐    ┌────────────────────────────────────────┐ │
│  │  Global     │    │           open-flow Daemon             │ │
│  │  Hotkey     │────│  (macOS: 右Cmd / Win+Linux: 右Alt)      │ │
│  │             │    │  ┌──────────┐     ┌──────────────┐    │ │
│  └─────────────┘    │  │ Hotkey   │────▶│ State        │    │ │
│                     │  │ Listener │     │ Machine      │    │ │
│                     │  └──────────┘     └──────┬───────┘    │ │
│                     │                          │             │ │
│                     │           ┌──────────────▼──────────┐ │ │
│                     │           │       Audio Capture      │ │ │
│                     │           │  (cpal, 16kHz mono f32)  │ │ │
│                     │           └──────────────┬──────────┘ │ │
│                     │                          │             │ │
│                     │           ┌──────────────▼──────────┐ │ │
│                     │           │       ASR Engine         │ │ │
│                     │           │  SenseVoiceSmall ONNX    │ │ │
│                     │           │  fbank→LFR→CMVN→ORT→CTC │ │ │
│                     │           └──────────────┬──────────┘ │ │
│                     │                          │             │ │
│  ┌─────────────┐    │           ┌──────────────▼──────────┐ │ │
│  │  Any App    │◀───│───────────│    Text Injection        │ │ │
│  │  (Paste)    │    │           │  arboard + 平台粘贴方式     │ │ │
│  └─────────────┘    │           └─────────────────────────┘ │ │
│                     └────────────────────────────────────────┘ │
│                                                                  │
│                     ┌────────────────────────────────────────┐ │
│                     │              CLI Tool                   │ │
│                     │  start / stop / status / setup         │ │
│                     │  config / transcribe / test-record     │ │
│                     └────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

---

## Key Components

### 1. Daemon (`src/daemon/mod.rs`)

核心功能循环（专用线程运行 daemon，主线程驱动托盘/事件循环）：

- **Hotkey Listener**：**macOS** 使用 CGEventTap 监听右 Command；**Windows/Linux** 使用 `rdev::listen` 监听右 Alt（AltGr）。入口在 `run_listen_loop` 内按 `target_os` 分派。
- **Audio Capture**：`cpal` 实时采集麦克风音频到 ring buffer（跨平台一致）。
- **State Machine**：`Idle → Recording → Processing → Idle`（与平台无关）。
- **ASR Engine**：完整 Rust 推理管线，~82ms（见 ASR Pipeline 章节）。
- **Text Injection**：`arboard` 写剪贴板 + 平台粘贴：**macOS** `osascript` Cmd+V；**Linux** `xdotool`/`wtype` Ctrl+V；**Windows** 仅剪贴板，提示用户 Ctrl+V。

### 2. CLI Tool (`src/main.rs`)

clap 驱动的命令行接口：

| 命令 | 说明 |
|------|------|
| `open-flow setup` | 按当前预设下载模型（quantized ~230MB / fp16 ~450MB） |
| `open-flow start` | 后台启动 daemon（默认，关终端不影响） |
| `open-flow start --foreground` | 前台启动（终端占用，可看日志） |
| `open-flow stop` | 发送 SIGTERM 停止 daemon |
| `open-flow status` | 查看 PID、运行状态、日志路径 |
| `open-flow transcribe` | 单次录音或文件转写 |
| `open-flow model use <预设>` | 切换 quantized / fp16，未就绪时自动下载 |
| `open-flow model list` | 列出当前预设与可用预设 |
| `open-flow config` | 模型路径、热键等配置管理 |
| `open-flow test-hotkey` | 自动化热键测试（模拟按键多轮，配合 daemon 日志排查） |

### 3. State Machine

```
Idle ──[热键]──▶ Recording ──[热键]──▶ Processing ──▶ Idle
```
（热键：macOS 右 Command，Windows/Linux 右 Alt。）

### 4. Tray Icon（多平台）

- **统一抽象**：`TrayHandle`（Send+Sync）、`TrayState`、`TrayIconState`（Idle/Recording/Transcribing）。Daemon 只依赖抽象，通过 channel 发送状态更新。
- **平台实现**（`src/tray/mod.rs` 内按 `#[cfg(target_os = "...")]` 分三块）：
  - **macOS**：菜单栏 `NSStatusItem`，主线程创建并驱动 **NSRunLoop**，菜单事件 + 三态图标。
  - **Windows**：任务栏系统托盘（tray-icon），主线程创建并驱动 **Win32 消息循环**（`PeekMessageW`/`DispatchMessageW`）。
  - **Linux**：系统托盘（tray-icon + libappindicator），主线程创建并驱动 **glib MainContext**（`iteration(false)`）。
- **主线程**：创建 `TrayState` 后进入 `run_main_loop`，按平台调用 `pump_run_loop_100ms` / `pump_win32_messages` / `pump_glib_linux`，并处理 `flush_state_updates`、`flush_menu_events`、退出条件。
- **菜单**：版本项可点击打开 GitHub（macOS `open`、Windows `cmd /c start`、Linux `xdg-open`）；状态项与三态图标同步；退出项统一置 `exit_requested`。

---

## ASR Pipeline

```
Microphone (16kHz mono f32)
    │
    ▼ Ring Buffer (real-time capture)
    │
    ▼ Temp WAV File (on stop)
    │
    ▼ fbank (N_FFT=512, n_mels=80, Hamming window, pre-emphasis)
    │
    ▼ LFR (frame stacking, factor=7, shift=6)
    │
    ▼ CMVN (am.mvn, Kaldi-compatible)
    │
    ▼ ORT Session (SenseVoiceSmall, language_id + textnorm_id)
    │
    ▼ CTC Greedy Decode (blank=0, postprocess_tokens)
    │
    ▼ Text → Clipboard (arboard) → 平台粘贴（macOS osascript / Linux xdotool|wtype / Windows 提示 Ctrl+V）
```

**关键实现参数**：

| 参数 | 值 | 说明 |
|------|----|------|
| 采样率 | 16kHz | SenseVoice 要求 |
| FFT 大小 | 512 | 对齐 Python FunASR |
| Mel bins | 80 | — |
| Mel fmax | 8000 Hz | — |
| LFR factor | 7 | — |
| LFR shift | 6 | — |
| dither | 0（关闭）| 保证确定性输出 |
| language_id | 0=auto | 可通过 env 覆盖 |
| CTC blank | 0 | — |

---

## SenseVoice Model

### 模型来源与预设

- **quantized**（默认）：[Hugging Face haixuantao/SenseVoiceSmall-onnx](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx)（~230MB）
- **fp16**：[Hugging Face ruska1117/SenseVoiceSmall-onnx-fp16](https://huggingface.co/ruska1117/SenseVoiceSmall-onnx-fp16)（~450MB，含 model.onnx + model.onnx.data）
- 官方：ModelScope [iic/SenseVoiceSmall-onnx](https://www.modelscope.cn/models/iic/SenseVoiceSmall-onnx)

配置由 `config.toml` 的 `model_preset` 决定（quantized | fp16）。**首次使用某预设时**，`open-flow start` 或 `open-flow model use <预设>` 会从对应 HF 仓库自动下载到默认数据目录。

```bash
open-flow setup              # 按当前预设下载到默认目录
open-flow model use fp16     # 切换到 fp16 并自动下载（若未就绪）
open-flow model list         # 查看当前预设与可用预设
```

### 模型目录结构

- **quantized**：`models/sensevoice-small/`（含 model.onnx 或 model_quant.onnx、tokens.json、am.mvn、config.yaml）
- **fp16**：`models/sensevoice-small-fp16/`（含 model.onnx、model.onnx.data、tokens.json、am.mvn、config.yaml）

ASR 引擎通过 `find_model_file()` 依次查找 `model.onnx` → `model_quant.onnx`，两种命名均可直接使用。

### 量化版 vs fp16

| 对比项 | quantized（默认） | fp16 |
|--------|------------------|------|
| 文件大小 | ~230MB | ~450MB |
| 推理耗时（M3 Pro） | ~83ms | ~79ms |
| 识别效果 | 与 fp16 一致（已回归验证） | 基准 |

---

## Platform Integration

### macOS

- **权限**：麦克风、辅助功能、输入监控（系统设置 → 隐私与安全性）。热键为 **右 Command**。
- **热键**：`src/hotkey/mod.rs` 中 `run_listen_loop_macos` 使用 **CGEventTap**（非 rdev），监听 `KeyCode::RIGHT_COMMAND` 与 `CGEventFlagCommand`。
- **文本注入**：`arboard` 写剪贴板 + `osascript` 发送 Cmd+V（走 Accessibility API，避免与 CGEventTap 冲突）。

### Windows

- **热键**：`rdev::listen` 监听 `Key::AltGr`（右 Alt）。部分环境需「以管理员身份运行」才能全局生效。
- **托盘**：tray-icon + Win32 消息循环（`cli/daemon.rs` 中 `pump_win32_messages`）。
- **文本注入**：仅写剪贴板，提示用户 Ctrl+V。

### Linux

- **热键**：`rdev::listen` 监听 `Key::AltGr`（右 Alt）。需将用户加入 `input` 组以访问输入设备。
- **托盘**：tray-icon + libappindicator，主线程跑 glib `MainContext::iteration(false)`（`pump_glib_linux`）。
- **文本注入**：剪贴板 + `xdotool`（X11）或 `wtype`（Wayland）发送 Ctrl+V；未安装时提示安装。

---

## Configuration

### 配置文件与数据目录

由 `directories` crate 按 OS 约定解析（`ProjectDirs::from("com", "openflow", "open-flow")`）：

- **macOS**：`~/Library/Application Support/com.openflow.open-flow/config.toml`
- **Windows**：`%APPDATA%\openflow\open-flow\config.toml`
- **Linux**：`~/.config/openflow/open-flow/config.toml`

模型默认目录：与 config 同级的 `models/` 下，按预设分目录：`sensevoice-small/`（quantized）、`sensevoice-small-fp16/`（fp16）。

### 默认值

```toml
model_path    = ""            # 未设置时从 OPEN_FLOW_MODEL 环境变量读取
```
（热键、output_mode 等当前由代码固定：macOS 右 Command，Win/Linux 右 Alt。）

---

## Code Structure

```
src/
├── main.rs                    CLI 入口，clap 命令路由；is_app_bundle_launch() 仅 macOS
├── asr/
│   ├── mod.rs                 AsrEngine：find_model_file/load_model/transcribe
│   ├── preprocess.rs          fbank→LFR→CMVN（N_FFT=512，dither=0，Kaldi 对齐）
│   ├── onnx_inference.rs      ORT session，输入 speech/speech_lengths/language/textnorm
│   └── decoder.rs             CTC 贪婪解码，blank=0，postprocess_tokens
├── hotkey/
│   └── mod.rs                 macOS: CGEventTap 右 Command；非 macOS: rdev 右 Alt(AltGr)
├── audio/
│   └── mod.rs                 AudioCapture：build_live_stream/record_to_file/save_wav
├── daemon/
│   └── mod.rs                 Daemon 主循环：热键→录音→转写→注入（与平台无关，用 TrayHandle 抽象）
├── tray/
│   └── mod.rs                 三平台 #[cfg]：macOS 菜单栏 / Windows·Linux 系统托盘；统一 TrayHandle/TrayState
├── text_injection/
│   └── mod.rs                 arboard + 平台粘贴：macOS osascript / Linux xdotool|wtype / Windows 仅剪贴板
├── cli/
│   ├── daemon.rs              start/stop/status；run_main_loop 按平台 pump（NSRunLoop/Win32/glib）
│   └── commands/
│       ├── setup.rs           open-flow setup：HTTPS 下载模型，带进度
│       ├── config.rs          set-model/set-hotkey/show
│       ├── transcribe.rs      单次转写文件或录音
│       ├── test_hotkey.rs     模拟热键多轮测试（macOS MetaRight / Win+Linux AltGr）
│       └── test_record.rs     测试录音
└── common/
    ├── config.rs              Config toml，data_dir()/config_path()（directories 跨平台）
    └── types.rs               RecordingState/HotkeyEvent/TranscriptionResult
```

---

## Build & Distribution

### 编译

```bash
cargo build --release
# 产物: target/release/open-flow（或 .exe on Windows）
```

### 目标架构与发版产物

| 平台 | 架构 | 发版产物 |
|------|------|----------|
| macOS | aarch64-apple-darwin | CLI tar.gz（CI）+ `.app` zip（开发者本机签名后手动上传） |
| macOS | x86_64-apple-darwin | 需从源码编译（ONNX Runtime 无 x86 预编译时） |
| Windows | x86_64-pc-windows-msvc | CLI zip |
| Linux | x86_64-unknown-linux-gnu | CLI tar.gz |

### 发布方式

1. **GitHub Releases**：push tag `v*` 触发 CI，构建 macOS/Linux/Windows CLI 预编译包并创建 Release。
2. **macOS `.app`**：由开发者本机执行 `./scripts/release.sh` 打包并上传到对应 Release，避免 CI 生成 `ad-hoc` 签名导致权限身份漂移。
3. **install.sh**：macOS `curl | sh` 一键安装。
4. **README 一键安装**：Windows（PowerShell）、Linux（bash）各一条命令下载解压并加入 PATH。
5. **Homebrew**（规划中）：`brew install open-flow`。

---

## Performance

| 指标 | 实测值（M3 Pro） | 目标 |
|------|----------------|------|
| 热键延迟 | < 10ms | < 50ms |
| 转写耗时（~5s 音频） | ~82ms | < 200ms |
| 内存占用（含模型） | ~150MB | < 200MB |

---

## Security

- **完全离线**：无任何网络调用（setup 命令下载模型除外），三平台一致。
- **本地模型**：语音数据不上传。
- **最小权限**：macOS 麦克风 + 辅助功能 + 输入监控；Windows/Linux 麦克风 + 输入设备（热键/托盘所需）。
- **代码开源**：逻辑完全可审计。

---

## 解耦与维护

- **平台隔离**：热键、托盘、文本注入、事件循环均通过 `#[cfg(target_os = "...")]` 分平台实现，**macOS 主产品代码路径与依赖独立**，Win/Linux 修改不影响 Mac 体验。
- **统一抽象**：Daemon 只依赖 `TrayHandle`/`TrayIconState`、`TextInjector`、`HotkeyListener`，不关心具体平台；平台差异在 tray、hotkey、text_injection、cli/daemon 边界收口。
- **知识索引**：本文档（ARCHITECTURE.md）为当前实现的知识索引；平台差异以「Platform Matrix」和「Platform Integration」为准，代码结构以「Code Structure」为准。

---

## Python Reference Implementation

仓库同时维护 Python 版本（`python/`），基于官方 FunASR 管线，作为效果对比基准：

```bash
# 同一音频 Rust vs Python 效果对比
OPEN_FLOW_MODEL=/path/to/model ./scripts/compare_transcribe.sh testdata/mixed_zh_en.wav
```

Python 版不用于日常使用，用于回归验证 Rust 管线与官方实现的语义对齐。
