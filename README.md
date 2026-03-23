# Open Flow

**简体** | [繁體](README.zh-TW.md) | [English](README.en.md)

![Open Flow](assets/open-flow-banner.png)

**面向 AI 编程场景的开源语音输入工具。** 按一下键录音，再按一下转写并粘贴。

---

## 为什么选 Open Flow

| | Open Flow | Wispr / Typeless / 闪电说 |
| --- | --- | --- |
| **开源** | ✅ MIT，完整代码可审计 | ❌ 闭源 |
| **本地模型** | ✅ 语音不离开本机 | 多为云端 |
| **性能** | ✅ Rust，~5 秒音频约 83ms 转写 | 各异 |
| **可定制** | ✅ 热键、模型、输出方式 | 受限 |

我们相信**只有开源才能让更多人参与**：查看实现、修改行为、接入自己的模型、提交改进。Open Flow 是「热键 → 录音 → 本地转写 → 自动粘贴」的开源实现。

---

## 核心亮点

### 🦀 Rust 性能

- **~83ms** 转写约 5 秒音频（M3 Pro 实测）
- 单二进制、无运行时，**内存占用低**
- 启动快，适合常驻后台

### 🔓 完全开源

- **MIT 协议**；可审计、可 fork、可修改
- 无厂商锁定，社区驱动
- 对比闭源产品：[Wispr](https://www.wispr.ai/)、[Typeless](https://typeless.dev/)、[闪电说](https://www.shandianshuo.com/)

### 🔒 本地模型，隐私优先

- **SenseVoiceSmall** 完全在本地运行
- 无需云端 API，语音不离开你的电脑
- 首次下载模型后，可离线使用（约 230MB）

---

## 功能

- 在 Cursor、VS Code、终端、浏览器中用语音代替打字
- 中英混合，自动标点
- 转写结果写入剪贴板并自动粘贴（macOS 可选 CGEvent 模拟打字），可随时再次粘贴
- 菜单栏托盘图标（灰/红/黄），录音时可选**浮动指示器**（光标旁「录音中…」「转写中…」）
- 可配置热键（右 Command / Fn / F13）、触发模式（按一次开关 toggle / 按住录 hold）、**简繁转换**（简→繁 / 繁→简）
- 可选本地 SenseVoice 或 **Groq Whisper** 云端识别；可切换模型预设（quantized / fp16）
- 可选 **BigModel 轻量纠错**：支持个人词表，修正常见术语与明显 ASR 错误（需用户自行配置 API Key）
- **macOS**：托盘菜单「偏好设置…」打开 **SwiftUI 设置界面**，图形化管理热键、Provider、模型、权限与日志
- **macOS**：托盘菜单支持自动更新（后台下载，下载完成后点击重启应用更新）

### 浮动录音指示器（macOS）

录音时会在光标附近显示药丸形浮层，红色圆点 +「Recording…」表示正在录音，转写时显示「转写中…」，不挡鼠标操作。

![录音时浮动指示器](assets/recording-overlay.png)

---

## 设置界面（macOS）

从托盘菜单点击 **「偏好设置…」** 可打开图形化设置窗口，无需改 config 文件即可管理以下内容。

![Open Flow 设置界面 - 一般](assets/settings-app-general.png)

| 分页 | 功能 |
|------|------|
| **一般** | 热键（右 Command / Fn / F13）、触发模式（Toggle / Hold）、简繁转换（无 / 简→繁 / 繁→简）、macOS 权限状态与「打开设置」跳转 |
| **Provider** | 本地 / Groq 切换、Groq API Key、Whisper 模型与语言、保存并应用 |
| **模型** | 模型状态、下载/重新下载、路径显示 |
| **测试** | 实时热键事件监听，验证按键是否被识别 |
| **日志** | 查看 daemon 最近约 100 行日志 |

窗口顶部显示 **daemon 状态**（运行中 PID、运行时间），并提供 **Start / Stop / Restart / Quit Open Flow** 等控制。权限项会显示是否已授权（绿色勾）或可点击跳转系统设置。修改后点击 **Save** 保存；部分项需重启 daemon 后生效。

### Personal Vocabulary / BigModel 纠错

- `General -> Personal Vocabulary -> Configure` 可开启轻量云纠错
- 为避免泄露，仓库与发布包**不内置 API Key**，需用户自行生成并填写
- BigModel API Key 申请地址：`https://bigmodel.cn/usercenter/proj-mgmt/apikeys`
- 关闭该开关时，Open Flow 仅使用本地/Provider 转写结果，不会调用 BigModel

---

## 自动更新（macOS .app）

从托盘菜单点击 **「检查更新并升级...」**：

1. 应用会在后台检查 GitHub Releases 并下载最新安装包（不影响当前继续使用）。
2. 下载完成后，菜单项会变为 **「重启以应用更新」**。
3. 点击后会自动退出当前版本、替换 App，并重新打开新版本。

如果已经是最新版本，会弹窗提示 **「已是最新版本」**。

---

## 平台支持

| 平台 | 安装方式 | 托盘图标 | 自动粘贴 |
| --- | --- | --- | --- |
| macOS Apple Silicon（M1/M2/M3） | 一键安装 / .app 下载 | ✅ | ✅ osascript |
| macOS Intel | 从源码构建 | ✅ | ✅ osascript |
| Linux（X11） | 从源码构建 | — | ✅ xdotool |
| Linux（Wayland） | 从源码构建 | — | ✅ wtype |
| Windows | 从源码构建 / Releases 下载 | — | 剪贴板（需手动 Ctrl+V） |

---

## 快速开始

### macOS

```bash
# 一键安装（Apple Silicon 预编译包，首次自动下载 ~230MB 模型）
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# 启动（后台运行，可随时关掉终端）
open-flow start
```

首次运行会从 Hugging Face 按当前预设自动下载模型（默认 quantized）。支持两种预设，**首次使用对应预设时都会自动下载**：

- **quantized**（默认）：~230MB，体积小
- **fp16**：~450MB，更高精度，来自 [ruska1117/SenseVoiceSmall-onnx-fp16](https://huggingface.co/ruska1117/SenseVoiceSmall-onnx-fp16)，需手动切换

切换为高精度：`open-flow model use fp16`（若未下载会先自动拉取）；列出预设：`open-flow model list`。菜单栏灰色圆点即就绪，按右侧 Command 录音，再按一次转写并粘贴。

**或下载 .app**（双击即运行）：[Releases](https://github.com/jqlong17/open-flow/releases) 页面下载 `Open-Flow-<版本>-macos-aarch64.app.zip`，解压后将 **Open Flow.app** 拖入「应用程序」。运行后点击菜单栏托盘图标，选择 **「偏好设置…」** 即可打开图形化设置界面（热键、Provider、模型、权限、日志等）。

### Linux

Linux 版支持 **系统托盘**（通知区域图标显示待机/录音/转写状态，右键可退出；需安装 libappindicator）。可选：一键安装预编译包，或从源码构建。

**一键安装（预编译，x86_64）**

在终端执行（下载并解压到 `~/.local/bin`，并写入 PATH）：

```bash
mkdir -p ~/.local/bin && curl -sSL https://github.com/jqlong17/open-flow/releases/latest/download/open-flow-x86_64-unknown-linux-gnu.tar.gz | tar -xzf - -C ~/.local/bin && chmod +x ~/.local/bin/open-flow && (grep -q '.local/bin' ~/.bashrc 2>/dev/null || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc) && echo '安装完成。执行 source ~/.bashrc 或重新打开终端，然后运行: open-flow start --foreground'
```

安装后运行 `open-flow start --foreground`，首次会自动下载 ~230MB 模型。热键为 **右侧 Alt 键**；粘贴需安装 xdotool（X11）或 wtype（Wayland）。托盘需安装 libappindicator（见下方从源码构建）。

**从源码构建**（需先安装系统依赖与 Rust）

```bash
# Ubuntu / Debian：系统依赖（含托盘：libappindicator）
sudo apt install libasound2-dev xdotool libappindicator3-dev   # 或 libayatana-appindicator3-dev；X11 粘贴用 xdotool，Wayland 用 wtype

# 安装 Rust（已安装可跳过）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh && source ~/.cargo/env

# 克隆并编译
git clone https://github.com/jqlong17/open-flow.git && cd open-flow
cargo build --release
sudo cp target/release/open-flow /usr/local/bin/
```

> **注意**：Linux 上全局热键监听需要读取输入设备权限。如遇权限不足，将当前用户加入 `input` 组：`sudo usermod -aG input $USER`（重新登录后生效）。

### Windows

Windows 版支持 **系统托盘**（任务栏右侧图标显示待机/录音/转写状态，右键可退出）。转写结果会写入剪贴板，需在目标窗口按 **Ctrl+V** 粘贴。

**一键安装（PowerShell，预编译）**

在 **PowerShell** 中执行（下载并解压到 `%LOCALAPPDATA%\Programs\open-flow`，并加入用户 PATH）：

```powershell
$url = "https://github.com/jqlong17/open-flow/releases/latest/download/open-flow-x86_64-pc-windows-msvc.zip"; $dir = "$env:LOCALAPPDATA\Programs\open-flow"; New-Item -ItemType Directory -Force -Path $dir | Out-Null; Invoke-WebRequest -Uri $url -OutFile "$dir\open-flow.zip" -UseBasicParsing; Expand-Archive -Path "$dir\open-flow.zip" -DestinationPath $dir -Force; Remove-Item "$dir\open-flow.zip"; $path = [Environment]::GetEnvironmentVariable("Path", "User"); if ($path -notlike "*$dir*") { [Environment]::SetEnvironmentVariable("Path", "$path;$dir", "User"); Write-Host "已把 $dir 加入 PATH。" }; $env:Path = [Environment]::GetEnvironmentVariable("Path", "User") + ";" + [Environment]::GetEnvironmentVariable("Path", "Machine"); Write-Host "安装完成。本窗口可直接运行: open-flow.exe start --foreground"
```

安装完成后**当前窗口**即可运行 `open-flow.exe start --foreground`；新开的终端也会自动识别命令。首次运行会自动下载约 230MB 模型。热键为 **右侧 Alt 键**，转写结果在剪贴板，在任意输入框按 **Ctrl+V** 粘贴。

**从源码构建**（需先安装 [Rust](https://rustup.rs/)）

```powershell
git clone https://github.com/jqlong17/open-flow.git
cd open-flow
cargo build --release
# 二进制在 target\release\open-flow.exe，可加入 PATH 或复制到常用目录
```

**常用命令**

| 命令 | 说明 |
|------|------|
| `open-flow.exe start` | 后台启动 |
| `open-flow.exe start --foreground` | 前台启动（终端看日志，Ctrl+C 停止） |
| `open-flow.exe stop` | 停止后台 daemon |
| `open-flow.exe status` | 查看状态 |
| `open-flow.exe transcribe --duration 5` | 单次录音 5 秒并转写 |

> **说明**：Windows 上全局热键（rdev）可能需要**以管理员身份运行**才能在某些应用中生效；若无效可改用 `transcribe` 命令做单次录音转写。

---

## macOS 权限设置

Open Flow 需要以下三项系统权限才能正常工作。**首次启动后请依次在系统设置中手动开启**，每项授权后需完全退出并重新打开 App。

前往 **系统设置 → 隐私与安全性**，依次添加 `Open Flow.app`：

| 权限 | 路径 | 用途 |
| --- | --- | --- |
| **麦克风** | 隐私与安全性 → 麦克风 | 录制语音 |
| **辅助功能** | 隐私与安全性 → 辅助功能 | 监听全局热键（右侧 Command） |
| **输入监控** | 隐私与安全性 → 输入监控 | 监听全局热键（右侧 Command） |

> **排查提示**：启动日志会打印 `🔎 权限诊断`，`Microphone / Accessibility / Input Monitoring` 均为 `true` 即表示授权完整。实时查看日志：
> ```bash
> tail -f ~/Library/Application\ Support/com.openflow.open-flow/daemon.log
> ```

> **重要说明（当前未使用开发者签名/公证分发）**：
> 更新后系统可能将新版本视为不同应用，导致麦克风、辅助功能、输入监控授权失效。遇到权限异常时，请在系统设置中先删除旧授权项，再重新添加 `Open Flow.app` 并重新授权三项权限。
>
> 若不确定授权是否生效，可在托盘 **「偏好设置…」 -> 「一般」** 页面查看实时权限状态（绿色勾表示已授权）。

**从源码构建**（需 [Rust](https://rustup.rs/)）：`git clone https://github.com/jqlong17/open-flow.git && cd open-flow && cargo build --release`（macOS / Linux / Windows 通用）

**macOS 本地打 .app 包**：`./scripts/build-app.sh` → 构建 `dist/Open Flow.app` 并安装到 `/Applications/Open Flow.app`（可设置 `OPEN_FLOW_SIGN_IDENTITY` 使用固定签名身份）

---

## 常用命令

| 命令 | 说明 |
| --- | --- |
| `open-flow start` | 后台启动（默认，无需保持终端） |
| `open-flow start --foreground` | 前台启动（终端保持，可看日志） |
| `open-flow stop` | 停止 daemon |
| `open-flow status` | 状态、PID、日志路径 |
| `open-flow setup` | 手动下载模型 |
| `open-flow transcribe --file <wav>` | 转写单个音频文件 |

**排查热键**：`RUST_LOG=info open-flow start` 可输出 `[Hotkey]` 日志，便于确认按键与录音状态。

**自动化热键测试**：终端 1 运行 `RUST_LOG=info open-flow start`，终端 2 运行 `open-flow test-hotkey --cycles 3`，可自动模拟多轮「按 Command 开始 → 等 3s → 按 Command 停止 → 等转写」，对照终端 1 的 `[Hotkey]` 日志排查问题。

---

## 文档

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 知识索引与系统架构：平台矩阵、Daemon/CLI/ASR、托盘与热键解耦、配置与发版

---

## 模型信息与 Hugging Face 地址

两种预设均从 Hugging Face 拉取，**配置了对应预设后首次启动或执行 `open-flow model use <预设>` 时会自动下载**，无需手动下载。

| 预设 | 说明 | 体积 | Hugging Face |
|------|------|------|--------------|
| **quantized**（默认） | 量化版，体积小 | ~230MB | [haixuantao/SenseVoiceSmall-onnx](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx) |
| **fp16** | 高精度，非量化 | ~450MB | [ruska1117/SenseVoiceSmall-onnx-fp16](https://huggingface.co/ruska1117/SenseVoiceSmall-onnx-fp16) |

切换预设：`open-flow model use fp16`；列出预设：`open-flow model list`。

---

## 参与贡献

欢迎 fork、提 issue、提交 PR，一起把开源语音输入体验做得更好。

---

## License

MIT
