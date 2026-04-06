# Open Flow

[中文](README.md) | **English**

![Open Flow](assets/open-flow-banner.png)

**Open-source voice input for AI coding.** Press a key to record on macOS, press again to transcribe and paste into any input field.

---

## Versions

Open Flow currently has two distribution tracks:

- **GitHub open-source edition**: the fuller version developed in this repository, with broader platform support and more experimental features such as meetings, draft workflows, and additional settings
- **Mac App Store edition**: a more focused build designed for review stability and simpler onboarding, currently centered on **local offline microphone transcription** with a bundled offline model

These two editions share the same project and core direction, but they do **not** aim to expose exactly the same surface area. If you see a feature in GitHub screenshots, issues, or docs that is missing from the App Store build, that is often an intentional product split rather than a broken installation.

The App Store build is paid, but the project remains open source:

- the App Store price helps cover Apple Developer Program membership, signing, distribution, and ongoing maintenance costs
- the source code remains available here on GitHub
- if you do not want to buy the App Store build, you are still free to build Open Flow from source yourself

The screenshots and feature descriptions below mainly reflect the **GitHub open-source / fuller edition** unless noted otherwise.

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

You can run `open-flow.exe start --foreground` in the same window right away; new terminals will also find the command. First run downloads ~230MB model. On Windows the default hotkey is **Right Win** (older configs are auto-migrated for compatibility), and you can switch it to **Right Alt** if preferred. Results go to the clipboard; press **Ctrl+V** to paste.

**Windows / Linux troubleshooting**

If startup, hotkey, recording, or audio device detection fails, run:

```bash
open-flow support
```

or on Windows:

```powershell
.\open-flow.exe support
```

This prints the executable path, config path, input-device snapshot, and the tail of `daemon.log`. Sending that output to the maintainer makes remote debugging much easier. Issues are welcome, and PRs to improve the Windows / Linux experience are especially appreciated.

**Windows / Linux model download fallback**

If the default Hugging Face download times out, Open Flow will now try both the official source and `https://hf-mirror.com` automatically. You can also override the download source yourself:

```powershell
$env:OPEN_FLOW_HF_MIRROR = "https://hf-mirror.com"
open-flow.exe setup
```

```powershell
$env:OPEN_FLOW_MODEL_BASE_URL = "https://hf-mirror.com/haixuantao/SenseVoiceSmall-onnx/resolve/main"
open-flow.exe setup
```

```powershell
$env:OPEN_FLOW_MODEL_BASE_URLS = "https://my-mirror.example.com/haixuantao/SenseVoiceSmall-onnx/resolve/main,https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main"
open-flow.exe setup
```

Manual model directory is also supported when remote download is unreliable:

```powershell
open-flow.exe setup --model-dir D:\open-flow-models\sensevoice-small
open-flow.exe start --foreground --model D:\open-flow-models\sensevoice-small
```

On Windows, the default quantized model directory is usually `%APPDATA%\openflow\open-flow\data\models\sensevoice-small`.

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

If you are using a build that includes meeting or system-audio capture, you may also need **Screen Recording** permission. The current Mac App Store edition usually only needs the three permissions above.

> **Troubleshooting tip**: At startup the log prints `🔎 权限诊断`. All three values (`Microphone / Accessibility / Input Monitoring`) must be `true` for full functionality. View live logs:
> ```bash
> tail -f ~/Library/Application\ Support/com.openflow.open-flow/daemon.log
> ```

**Build from source** ([Rust](https://rustup.rs/)): `git clone https://github.com/jqlong17/open-flow.git && cd open-flow && cargo build --release`

**Build .app locally**: `./scripts/build-app.sh` → builds `dist/Open Flow.app` and installs `/Applications/Open Flow.app` (set `OPEN_FLOW_SIGN_IDENTITY` to keep a stable signing identity)

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

Before opening a PR, please read:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CLA.md](CLA.md)

These documents explain that contributions submitted to this repository may be used in both open-source distributions and paid/commercial distributions of Open Flow, including app-store distribution.

---

## License

MIT. See [LICENSE](LICENSE).
