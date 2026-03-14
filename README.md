# Open Flow

**中文** | [English](README.en.md)

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
- 转写结果写入剪贴板并自动粘贴，可随时再次粘贴
- 菜单栏托盘图标（灰/红/黄）
- 可自定义热键、输出方式、模型与集成

---

## 快速开始

```bash
# 1. 安装并启动（首次会自动下载 ~230MB 模型）
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# 2. 之后每次使用（后台运行，可随时关掉终端）
open-flow start
```

首次运行会从 [Hugging Face](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx) 下载模型。菜单栏灰色圆点即就绪，按右侧 Command 录音，再按一次转写并粘贴。

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

**平台说明**：一键安装仅提供 **Apple Silicon（M1/M2/M3）** 预编译包。**Intel Mac** 用户请从源码构建。

**下载 .app**（双击即运行）：[Releases](https://github.com/jqlong17/open-flow/releases) 页面下载 `Open-Flow-<版本>-macos-aarch64.app.zip`，解压后将 **Open Flow.app** 拖入「应用程序」即可。

**从源码构建**（需 [Rust](https://rustup.rs/)）：`git clone https://github.com/jqlong17/open-flow.git && cd open-flow && cargo build --release`

**本地打 .app 包**：`./scripts/build-app.sh` → 生成 `dist/Open Flow.app`

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

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 系统架构、音频管线、开发说明

---

## 参与贡献

欢迎 fork、提 issue、提交 PR，一起把开源语音输入体验做得更好。

---

## License

MIT
