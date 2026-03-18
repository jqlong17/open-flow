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
| Windows | Build from source / Releases | — | Clipboard (Ctrl+V to paste) |

---

## Quick Start

### macOS

```bash
# One-click install (Apple Silicon prebuilt, auto-downloads ~230MB model)
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# Start (runs in background; close terminal anytime)
open-flow start
```

First run downloads the quantized model from [Hugging Face](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx); switching to the full preset downloads the non-quantized model from [ruska1117/SenseVoiceSmall-onnx](https://huggingface.co/ruska1117/SenseVoiceSmall-onnx). Gray dot in menu bar = ready. Right Command to record, again to transcribe and paste.

**Or download .app** (double-click to run): [Releases](https://github.com/jqlong17/open-flow/releases) → `Open-Flow-<version>-macos-aarch64.app.zip` → unzip and drag **Open Flow.app** to Applications.

### Linux

Linux is supported with a **system tray** (notification area: idle / recording / transcribing; right-click to exit; requires libappindicator). One-line install (prebuilt) or build from source.

**One-line install (prebuilt, x86_64)**

Run in a terminal (downloads and extracts to `~/.local/bin`, adds to PATH):

```bash
mkdir -p ~/.local/bin && curl -sSL https://github.com/jqlong17/open-flow/releases/latest/download/open-flow-x86_64-unknown-linux-gnu.tar.gz | tar -xzf - -C ~/.local/bin && chmod +x ~/.local/bin/open-flow && (grep -q '.local/bin' ~/.bashrc 2>/dev/null || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc) && echo 'Done. Run source ~/.bashrc or open a new terminal, then: open-flow start --foreground'
```

Then run `open-flow start --foreground`. First run downloads ~230MB model. Hotkey: **Right Alt**; for paste, install xdotool (X11) or wtype (Wayland). Tray requires libappindicator (see build-from-source).

**Build from source** (install system deps and Rust first)

```bash
# Ubuntu / Debian: system deps
sudo apt install libasound2-dev xdotool libappindicator3-dev   # or libayatana-appindicator3-dev; xdotool for X11 paste, wtype for Wayland

# Install Rust (skip if installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh && source ~/.cargo/env

# Clone and build
git clone https://github.com/jqlong17/open-flow.git && cd open-flow
cargo build --release
sudo cp target/release/open-flow /usr/local/bin/
```

> **Note**: Global hotkey needs input device access. If you get a permission error: `sudo usermod -aG input $USER` (re-login to take effect).

### Windows

Windows is supported with a **system tray icon** (taskbar: idle / recording / transcribing; right-click to exit). Transcription is copied to the clipboard; press **Ctrl+V** in the target window to paste.

**One-line install (PowerShell, prebuilt)**

Run in **PowerShell** (downloads and extracts to `%LOCALAPPDATA%\Programs\open-flow`, adds to user PATH):

```powershell
$url = "https://github.com/jqlong17/open-flow/releases/latest/download/open-flow-x86_64-pc-windows-msvc.zip"; $dir = "$env:LOCALAPPDATA\Programs\open-flow"; New-Item -ItemType Directory -Force -Path $dir | Out-Null; Invoke-WebRequest -Uri $url -OutFile "$dir\open-flow.zip" -UseBasicParsing; Expand-Archive -Path "$dir\open-flow.zip" -DestinationPath $dir -Force; Remove-Item "$dir\open-flow.zip"; $path = [Environment]::GetEnvironmentVariable("Path", "User"); if ($path -notlike "*$dir*") { [Environment]::SetEnvironmentVariable("Path", "$path;$dir", "User"); Write-Host "Added $dir to PATH." }; $env:Path = [Environment]::GetEnvironmentVariable("Path", "User") + ";" + [Environment]::GetEnvironmentVariable("Path", "Machine"); Write-Host "Done. In this window run: open-flow.exe start --foreground"
```

You can run `open-flow.exe start --foreground` in the same window right away; new terminals will also find the command. First run downloads ~230MB model. Hotkey: **Right Alt key**; result in clipboard, **Ctrl+V** to paste.

**Build from source** (install [Rust](https://rustup.rs/) first)

```powershell
git clone https://github.com/jqlong17/open-flow.git
cd open-flow
cargo build --release
# Binary at target\release\open-flow.exe; add to PATH or copy to a folder in PATH
```

| Command | Description |
|---------|-------------|
| `open-flow.exe start` | Start in background |
| `open-flow.exe start --foreground` | Start in foreground (Ctrl+C to stop) |
| `open-flow.exe stop` | Stop background daemon |
| `open-flow.exe transcribe --duration 5` | One-shot record 5s and transcribe |

> **Note**: On Windows, global hotkey (rdev) may require **Run as administrator** in some apps. If it doesn’t work, use the `transcribe` command for one-shot recording.

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

**Build .app locally**: `./scripts/build-app.sh` → builds `dist/Open Flow.app` and installs `/Applications/Open Flow.app`

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
