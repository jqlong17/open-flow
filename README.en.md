# Open Flow

[中文](README.md) | **English**

![Open Flow](assets/open-flow-banner.png)

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

## Platform Support

| Platform | Install | Tray icon | Auto-paste |
| --- | --- | --- | --- |
| macOS Apple Silicon (M1/M2/M3) | One-click / .app download | ✅ | ✅ osascript |
| macOS Intel | Build from source | ✅ | ✅ osascript |
| Linux (X11) | Build from source | — | ✅ xdotool |
| Linux (Wayland) | Build from source | — | ✅ wtype |

---

## Quick Start

### macOS

```bash
# One-click install (Apple Silicon prebuilt, auto-downloads ~230MB model)
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# Start (runs in background; close terminal anytime)
open-flow start
```

First run downloads the model from [Hugging Face](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx). Gray dot in menu bar = ready. Right Command to record, again to transcribe and paste.

**Or download .app** (double-click to run): [Releases](https://github.com/jqlong17/open-flow/releases) → `Open-Flow-<version>-macos-aarch64.app.zip` → unzip and drag **Open Flow.app** to Applications.

### Linux

Linux is supported in CLI mode (no tray icon). Build from source:

**1. Install system dependencies**

```bash
# Ubuntu / Debian
sudo apt install libasound2-dev xdotool    # X11
# or Wayland
sudo apt install libasound2-dev wtype

# Fedora / RHEL
sudo dnf install alsa-lib-devel xdotool
```

**2. Install Rust** (skip if already installed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

**3. Clone and build**

```bash
git clone https://github.com/jqlong17/open-flow.git
cd open-flow
cargo build --release
sudo cp target/release/open-flow /usr/local/bin/
```

**4. First run** (downloads model and starts)

```bash
open-flow start
```

Downloads ~230MB model on first run. Then press Right Meta (Super) to record, press again to stop and paste the transcription into the focused input field.

> **Note**: Global hotkey listening requires access to input devices. If you get a permission error, add your user to the `input` group:
> ```bash
> sudo usermod -aG input $USER   # re-login to take effect
> ```

---

## macOS Permissions

Open Flow requires three system permissions. **After first launch, grant each one manually** in System Settings, then fully quit and reopen the app.

Go to **System Settings → Privacy & Security** and add `Open Flow.app` to each:

| Permission | Path | Purpose |
| --- | --- | --- |
| **Microphone** | Privacy & Security → Microphone | Record audio |
| **Accessibility** | Privacy & Security → Accessibility | Listen for global hotkey (Right Command) |
| **Input Monitoring** | Privacy & Security → Input Monitoring | Listen for global hotkey (Right Command) |

> **Troubleshooting tip**: At startup the log prints `🔎 权限诊断`. All three values (`Microphone / Accessibility / Input Monitoring`) must be `true` for full functionality. View live logs:
> ```bash
> tail -f ~/Library/Application\ Support/com.openflow.open-flow/daemon.log
> ```

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
