# Open Flow Architecture

## Overview

Open Flow 是一个 macOS 专属的语音输入工具，采用 `Daemon + CLI` 架构。核心设计理念是：**后台常驻、全局热键、快速响应**。

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
│          │          │  │ Listener │     │ Machine      │    │ │
│          │          │  └──────────┘     └──────┬───────┘    │ │
│          ▼          │           │               │            │ │
│  ┌─────────────┐    │           ▼               ▼            │ │
│  │  Recording  │    │  ┌──────────┐     ┌──────────────┐    │ │
│  │  State      │    │  │ Audio    │────▶│ ASR Engine   │    │ │
│  │  (Toggle)   │    │  │ Capture  │     │ (SenseVoice) │    │ │
│  └─────────────┘    │  └──────────┘     └──────┬───────┘    │ │
│                     │                           │            │ │
│  ┌─────────────┐    │                           ▼            │ │
│  │  Output     │    │                   ┌──────────────┐     │ │
│  │  (Paste)    │◀───│───────────────────│ Text Output  │     │ │
│  │             │    │                   │  - Clipboard │     │ │
│  │  ┌───────┐  │    │                   │  - Cmd+V     │     │ │
│  │  │Any App│  │    │                   └──────────────┘     │ │
│  │  └───────┘  │    │                                        │ │
│  └─────────────┘    └────────────────────────────────────────┘ │
│                                                                  │
│                     ┌────────────────────────────────────────┐ │
│                     │              CLI Tool                   │ │
│                     │  ┌────────────────────────────────────┐│ │
│                     │  │ Commands:                          ││ │
│                     │  │   - start/stop daemon              ││ │
│                     │  │   - config management              ││ │
│                     │  │   - one-shot transcribe            ││ │
│                     │  └────────────────────────────────────┘│ │
│                     └────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

## Key Components

### 1. Daemon (Core)

常驻后台进程，负责所有核心功能：

- **Hotkey Listener**: 监听全局热键（右侧 Command）
- **Audio Capture**: 使用 cpal 进行低延迟音频采集
- **ASR Engine**: 集成 SenseVoice 模型进行语音识别
- **Text Output**: 将识别结果注入当前输入框

### 2. CLI Tool (Control)

轻量级命令行接口：

- 管理 daemon 生命周期
- 配置管理（模型路径、热键等）
- 单次转写模式（非守护进程模式）

### 3. IPC Layer

Unix Domain Socket 用于 CLI 与 Daemon 通信：

```rust
enum DaemonMessage {
    StartRecording,
    StopRecording,
    GetStatus,
    StopDaemon,
}
```

### 4. State Machine

录音状态管理：

```
Idle ──[Right Cmd]──▶ Recording ──[Right Cmd]──▶ Processing ──▶ Idle
```

## Audio Pipeline

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Microphone │────▶│   Ring Buffer │────▶│   WAV File   │
│              │     │   (Real-time) │     │   (Temp)     │
└──────────────┘     └──────────────┘     └──────────────┘
                                                   │
                                                   ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Clipboard   │◀────│   Text       │◀────│  SenseVoice  │
│  (Paste)     │     │   Injection   │     │  Inference   │
└──────────────┘     └──────────────┘     └──────────────┘
```

1. **Capture**: 16kHz, mono, f32 PCM
2. **Buffer**: Ring buffer for real-time streaming
3. **Save**: Flush to temp WAV on stop
4. **Transcribe**: Run SenseVoice inference
5. **Paste**: 转写结果写入剪贴板并模拟 Cmd+V 粘贴（当前实现不恢复旧剪贴板）

## SenseVoice Integration

### Model Structure

支持直接加载现有的 SenseVoice 模型目录：

```
sensevoice-small/
├── model.onnx          # ONNX format model
├── config.yaml         # Model configuration
├── tokens.txt          # Vocabulary
└── am.mvn              # Feature normalization
```

### Inference Strategy

当前方案（MVP）：
- 使用 ONNX Runtime 或 Python bridge
- 优先兼容现有模型格式
- 支持量化模型以提升速度

未来优化：
- 原生 Rust 推理（Candle 或 burn）
- 模型缓存和预热
- 批处理和流式推理

## macOS Integration

### Permissions Required

1. **Microphone**: `NSMicrophoneUsageDescription`
2. **Accessibility**: For simulating Cmd+V keystrokes
3. **Input Monitoring**: For global hotkey detection

### Hotkey Implementation

使用 `CGEventTap` API 实现低级别全局热键监听：

```rust
// Detect right Command key
CGEventTap::new(
    kCGSessionEventTap,
    kCGHeadInsertEventTap,
    kCGEventTapOptionDefault,
    vec![kCGEventKeyDown, kCGEventKeyUp],
    callback,
);
```

### Text Injection

通过 Accessibility API 模拟 Cmd+V：

```rust
// 1. Set transcription to clipboard
set_clipboard_text(transcription);

// 2. Simulate Cmd+V
simulate_keypress(vec![kVK_Command, kVK_ANSI_V]);

// 当前实现：不恢复旧剪贴板，转写结果保留在剪贴板
```

## Configuration

### Config File Location

```
~/Library/Application Support/com.openflow.open-flow/config.toml
```

### Config Format

```toml
model_path = "/Users/ruska/Library/Application Support/Shandianshuo/models/sensevoice-small"
hotkey = "right-command"
output_mode = "paste"
language = "auto"
auto_paste = true
clipboard_restore = true
```

## Build & Distribution

### Build Targets

- `aarch64-apple-darwin` (Apple Silicon)
- `x86_64-apple-darwin` (Intel)

### Distribution

1. **GitHub Releases**: Pre-built binaries
2. **install.sh**: `curl | sh` installation
3. **Homebrew** (Future): `brew install open-flow`
4. **npm** (Future): Thin wrapper downloading binary

## Performance Goals

| Metric | Target |
|--------|--------|
| Hotkey latency | < 50ms |
| Recording start | < 100ms |
| Transcription (1s audio) | < 200ms |
| Total round-trip | < 500ms |
| Memory usage | < 200MB |

## Security Considerations

1. **No network calls**: 完全离线，无需联网
2. **Local model only**: 语音数据不上传
3. **Minimal permissions**: 仅请求必要权限
4. **Open source**: 代码可审计

## Future Roadmap

### Phase 1: MVP
- [x] Project scaffolding
- [ ] Basic daemon implementation
- [ ] Audio capture
- [ ] SenseVoice integration
- [ ] IPC communication

### Phase 2: Polish
- [ ] Error handling
- [ ] Config management
- [ ] Installation script
- [ ] Documentation

### Phase 3: Enhancements
- [ ] Multiple models support
- [ ] Custom hotkeys
- [ ] UI overlay (optional)
- [ ] Performance optimization

## Development Guidelines

### Code Structure

```
src/
├── main.rs              # CLI entry point
├── cli/
│   ├── mod.rs
│   ├── daemon.rs        # Daemon lifecycle
│   └── commands/        # CLI commands
│       ├── mod.rs
│       ├── config.rs
│       └── transcribe.rs
├── daemon/
│   └── mod.rs           # Daemon implementation
├── audio/
│   └── mod.rs           # Audio capture
├── hotkey/
│   └── mod.rs           # Global hotkey
└── common/
    ├── mod.rs
    ├── config.rs        # Config management
    ├── ipc.rs           # IPC protocol
    └── types.rs         # Shared types
```

### Error Handling

使用 `anyhow` 进行错误处理，确保所有错误都有上下文：

```rust
use anyhow::{Context, Result};

fn init_audio() -> Result<()> {
    let device = host
        .default_input_device()
        .context("Failed to get default input device")?;
    // ...
}
```

### Logging

使用 `tracing` 进行结构化日志：

```rust
use tracing::{info, warn, error};

info!(model_path = ?path, "Loading ASR model");
warn!("Model not found, using default");
error!(error = %e, "Failed to transcribe audio");
```
