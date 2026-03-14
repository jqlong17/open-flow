# Open Flow

[中文](README.zh-CN.md) | **English**

**Open-source voice input for AI coding.** Press a key to record on macOS, press again to transcribe and paste into any input field.

---

## Why Open Flow

| | Open Flow | Wispr / Typeless / 闪电说 |
| --- | --- | --- |
| **Open source** | ✅ MIT, full codebase | ❌ Closed-source |
| **Local model** | ✅ Speech never leaves your machine | Often cloud-based |
| **Performance** | ✅ Rust, ~83ms for 5s audio | Varies |
| **Customizable** | ✅ Hotkey, model, output | Limited |

We believe **only open source enables real participation**: inspect the code, change behavior, plug in your own models, contribute. Open Flow is the open implementation of "hotkey → record → local transcribe → auto paste."

---

## Highlights

### 🦀 Rust performance

- **~83ms** transcription for ~5s audio (M3 Pro)
- Single binary, no runtime; **low memory**
- Fast startup, suitable for always-on daemon

### 🔓 Fully open source

- **MIT license**; audit, fork, modify
- No vendor lock-in; community-driven
- Compare with closed tools: [Wispr](https://www.wispr.ai/), [Typeless](https://typeless.dev/), [闪电说](https://www.shandianshuo.com/)

### 🔒 Local model, private by design

- **SenseVoiceSmall** runs entirely on your Mac
- No cloud API; speech never leaves your machine
- Works offline after first model download (~230MB)

---

## Features

- Voice instead of typing in Cursor, VS Code, terminal, browser
- Mixed Chinese/English with automatic punctuation
- Results to clipboard + auto-paste; paste again anytime
- Menu bar tray icon (gray/red/yellow)
- Customize hotkey, output mode, model, integrations

---

## Quick Start

```bash
# 1. Install and start (auto-downloads ~230MB model on first run)
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# 2. Next time (runs in background; close terminal anytime)
open-flow start
```

First run downloads the model from [Hugging Face](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx). Gray dot in menu bar = ready. Right Command to record, again to transcribe and paste.

**First use**: Grant Accessibility in System Settings → Privacy & Security → Accessibility.

**Platform**: One-click install provides **Apple Silicon (M1/M2/M3)** prebuilt binaries only. **Intel Mac** users: build from source below.

**Download .app** (double-click to run): [Releases](https://github.com/jqlong17/open-flow/releases) → download `Open-Flow-<version>-macos-aarch64.app.zip` → unzip and drag **Open Flow.app** to Applications.

**Build from source** ([Rust](https://rustup.rs/)): `git clone https://github.com/jqlong17/open-flow.git && cd open-flow && cargo build --release`

**Build .app locally**: `./scripts/build-app.sh` → `dist/Open Flow.app`

---

## Commands

| Command | Description |
| --- | --- |
| `open-flow start` | Start in background (default; no terminal needed) |
| `open-flow start --foreground` | Start in foreground (terminal stays open for logs) |
| `open-flow stop` | Stop daemon |
| `open-flow status` | Status, PID, log path |
| `open-flow setup` | Manually download model |
| `open-flow transcribe --file <wav>` | Transcribe a file |

**Debug hotkey**: `RUST_LOG=info open-flow start` prints `[Hotkey]` logs for key presses and recording state.

---

## Docs

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Architecture, audio pipeline, development

---

## Contributing

Fork, open issues, submit PRs. Let's improve open-source voice input together.

---

## License

MIT
