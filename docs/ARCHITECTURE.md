# Open Flow Architecture

## Overview

Open Flow 是一个 macOS 专属的开源语音输入工具，采用 `Daemon + CLI` 架构。核心设计理念：**后台常驻、全局热键、本地推理、快速响应**。

---

## System Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                          User Space                             │
│                                                                  │
│  ┌─────────────┐    ┌────────────────────────────────────────┐ │
│  │  Global     │    │           open-flow Daemon             │ │
│  │  Hotkey     │────│                                        │ │
│  │  (Right Cmd)│    │  ┌──────────┐     ┌──────────────┐    │ │
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
│  │  (Paste)    │    │           │  arboard + osascript     │ │ │
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

核心功能循环（方案 A：在专用线程运行，主线程留给托盘）：

- **Hotkey Listener**：监听全局热键（右侧 Command），基于 `rdev` CGEventTap
- **Audio Capture**：`cpal` 实时采集麦克风音频到 ring buffer
- **State Machine**：`Idle → Recording → Processing → Idle`
- **ASR Engine**：完整 Rust 推理管线，~82ms（见 ASR Pipeline 章节）
- **Text Injection**：`arboard` 写入剪贴板 + `osascript` 发送 Cmd+V（避免与 CGEventTap 冲突）

### 2. CLI Tool (`src/main.rs`)

clap 驱动的命令行接口：

| 命令 | 说明 |
|------|------|
| `open-flow setup` | 自动下载官方量化 ONNX 模型（~230MB） |
| `open-flow start` | 后台启动 daemon（默认，关终端不影响） |
| `open-flow start --foreground` | 前台启动（终端占用，可看日志） |
| `open-flow stop` | 发送 SIGTERM 停止 daemon |
| `open-flow status` | 查看 PID、运行状态、日志路径 |
| `open-flow transcribe` | 单次录音或文件转写 |
| `open-flow config` | 模型路径、热键等配置管理 |

### 3. State Machine

```
Idle ──[Right Cmd]──▶ Recording ──[Right Cmd]──▶ Processing ──▶ Idle
```

### 4. Tray Icon（方案 A）

- **主线程**：创建 `TrayState`，驱动 NSRunLoop，处理菜单事件
- **专用线程**：tokio `current_thread` + daemon（含 cpal::Stream，非 Send）
- **TrayHandle**：Send+Sync，daemon 通过 channel 发送状态更新（灰/红/黄）
- **菜单**：版本项可点击打开 GitHub；状态项随图标同步（待机/录音中/转写中）

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
    ▼ Text → Clipboard (arboard) → Cmd+V (osascript)
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

### 模型来源

官方量化 ONNX（推荐）：[ModelScope / iic/SenseVoiceSmall-onnx](https://www.modelscope.cn/models/iic/SenseVoiceSmall-onnx)

```bash
open-flow setup   # 自动下载到默认数据目录
```

### 模型目录结构

```
sensevoice-small/
├── model.onnx          # 或 model_quant.onnx（两种文件名均支持）
├── tokens.json         # 词表（~344KB）
├── am.mvn              # CMVN 特征均值/方差（~11KB）
└── config.yaml         # 模型配置
```

ASR 引擎通过 `find_model_file()` 依次查找 `model.onnx` → `model_quant.onnx`，两种命名均可直接使用。

### 量化版 vs 未量化版

| 对比项 | 量化版（官方） | 未量化版 |
|--------|--------------|---------|
| 文件大小 | ~230MB | ~894MB |
| 推理耗时（M3 Pro） | ~83ms | ~79ms |
| 识别效果 | 与未量化版一致（已回归验证） | 基准 |

---

## macOS Integration

### 所需权限

| 权限 | 用途 |
|------|------|
| **麦克风** | 录音（`NSMicrophoneUsageDescription`） |
| **辅助功能** | 模拟 Cmd+V 粘贴文字 |
| **输入监控** | 全局热键监听（CGEventTap） |

首次运行需在「系统设置 → 隐私与安全性」中手动开启辅助功能权限。

### 热键实现

使用 `rdev::listen` 监听 CGEventTap 事件，检测 `Key::MetaRight` 的 `Pressed` / `Released`：

```rust
rdev::listen(move |event| {
    if let EventType::KeyPress(Key::MetaRight) = event.event_type {
        // toggle recording
    }
});
```

### 文本注入

使用 `osascript` 发送 Cmd+V（而非 rdev::simulate），避免与 CGEventTap 热键监听冲突：

```rust
// 1. 转写结果写入剪贴板
arboard::Clipboard::new()?.set_text(text)?;

// 2. osascript 发送 Cmd+V（走 Accessibility API，不经过 CGEventTap）
std::process::Command::new("osascript")
    .arg("-e")
    .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
    .output()?;
```

---

## Configuration

### 配置文件位置

```
~/Library/Application Support/com.openflow.open-flow/config.toml
```

### 默认值

```toml
model_path    = ""            # 未设置时从 OPEN_FLOW_MODEL 环境变量读取
hotkey        = "right-command"
output_mode   = "paste"
language      = "auto"
auto_paste    = true
clipboard_restore = false     # 当前实现不恢复旧剪贴板
```

### 模型下载默认路径

```
~/Library/Application Support/com.openflow.open-flow/models/sensevoice-small/
```

---

## Code Structure

```
src/
├── main.rs                    CLI 入口，clap 命令路由
├── asr/
│   ├── mod.rs                 AsrEngine：find_model_file/load_model/transcribe
│   ├── preprocess.rs          fbank→LFR→CMVN（N_FFT=512，dither=0，Kaldi 对齐）
│   ├── onnx_inference.rs      ORT session，输入 speech/speech_lengths/language/textnorm
│   └── decoder.rs             CTC 贪婪解码，blank=0，postprocess_tokens
├── hotkey/
│   └── mod.rs                 rdev::listen，监听 Key::MetaRight
├── audio/
│   └── mod.rs                 AudioCapture：build_live_stream/record_to_file/save_wav
├── daemon/
│   └── mod.rs                 Daemon 主循环：热键→录音→转写→注入
├── tray/
│   └── mod.rs                 托盘图标（灰/红/黄）、菜单、状态同步
├── text_injection/
│   └── mod.rs                 arboard 写剪贴板 + osascript Cmd+V
├── cli/
│   ├── daemon.rs              start_background(spawn+setsid)/start_foreground/stop/status
│   └── commands/
│       ├── setup.rs           open-flow setup：HTTPS 下载模型文件，带进度显示
│       ├── config.rs          set-model/set-hotkey/show
│       └── transcribe.rs      单次转写文件或录音
└── common/
    ├── config.rs              Config toml，data_dir()/config_path()
    └── types.rs               RecordingState/HotkeyEvent/TranscriptionResult
```

---

## Build & Distribution

### 编译

```bash
cargo build --release
# 产物: target/release/open-flow
```

### 目标架构

- `aarch64-apple-darwin`（Apple Silicon，预编译包支持）
- `x86_64-apple-darwin`（Intel，需从源码编译，ONNX Runtime 无 x86_64 预编译）

### 发布方式

1. **GitHub Releases**：预编译二进制
2. **install.sh**：`curl | sh` 一键安装
3. **Homebrew**（规划中）：`brew install open-flow`

---

## Performance

| 指标 | 实测值（M3 Pro） | 目标 |
|------|----------------|------|
| 热键延迟 | < 10ms | < 50ms |
| 转写耗时（~5s 音频） | ~82ms | < 200ms |
| 内存占用（含模型） | ~150MB | < 200MB |

---

## Security

- **完全离线**：无任何网络调用（setup 命令下载模型除外）
- **本地模型**：语音数据不上传
- **最小权限**：仅请求麦克风 + 辅助功能
- **代码开源**：逻辑完全可审计

---

## Python Reference Implementation

仓库同时维护 Python 版本（`python/`），基于官方 FunASR 管线，作为效果对比基准：

```bash
# 同一音频 Rust vs Python 效果对比
OPEN_FLOW_MODEL=/path/to/model ./scripts/compare_transcribe.sh testdata/mixed_zh_en.wav
```

Python 版不用于日常使用，用于回归验证 Rust 管线与官方实现的语义对齐。
