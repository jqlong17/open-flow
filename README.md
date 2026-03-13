# Open Flow

**一个面向 AI 编程场景的开源语音输入工具。** 在 macOS 上按一下键开始录音，再按一下转成文字并自动粘贴到当前输入框。

- **开源优先**：可审计、可修改、可扩展，欢迎社区一起参与
- **本地离线**：语音不离开本机，使用本地 SenseVoice 模型
- **全局热键**：默认右侧 Command 键，任意 App 里都能用
- **Rust 主程序**：单二进制、低内存、响应快，适合常驻后台

---

## 为什么做 Open Flow

市场上已有 [Wispr](https://www.wispr.ai/)、[Typeless](https://typeless.dev/)、[闪电说](https://www.shandianshuo.com/) 等优秀的语音输入产品，但它们都是闭源商业软件。我们相信，**只有开源才能让更多人参与进来**：大家可以查看实现、修改行为、接入自己的模型、提交改进，并共同把语音输入体验做得更好。

Open Flow 想做的是一个开源版本的「热键录音 -> 本地转写 -> 自动粘贴」工作流。它既可以作为日常使用的工具，也可以作为一个可研究、可二次开发的开源基础设施。

---

## 核心特点

- 在 Cursor、VS Code、终端、浏览器输入框里，用语音代替打字
- 支持中英混合说话，自动转写并带标点
- 转写结果会写入系统剪贴板并自动粘贴，也可随时再次粘贴
- 菜单栏托盘图标显示状态（灰/红/黄），使用时只需按热键
- 完整开源，方便自定义热键、输出方式、模型与集成方式

---

## Rust / Python 版本对比

仓库内同时维护 Rust 和 Python 两个版本，它们分工不同：

| 版本 | 主要用途 | 特点 |
| --- | --- | --- |
| `Rust` | 日常使用、CLI、后台 daemon | 启动快、资源占用低、适合常驻运行，当前默认推荐 |
| `Python` | 官方 FunASR 管线参考、效果对比、回归验证 | 更接近官方参考实现，适合做单次验证 |

如果你是第一次使用 Open Flow，优先使用 **Rust 版**。如果你想核对模型效果或做参考对比，可以使用 **Python 版**，详见 `python/README.md`。

---

## 适合谁使用

- 适合：macOS 用户、重视开源和可控性的开发者
- 首次运行会自动下载 ~230MB 模型（ModelScope 官方），无需手动配置

---

## 快速开始

```bash
# 1. 安装并启动（首次会自动下载 ~230MB 模型）
curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

# 2. 之后每次使用
open-flow start
```

安装脚本会下载二进制到 `~/.local/bin` 并立即启动。首次运行会自动从 [Hugging Face](https://huggingface.co/haixuantao/SenseVoiceSmall-onnx) 下载 ~230MB 模型。菜单栏出现灰色圆点即就绪，按右侧 Command 录音，再按一次转写并粘贴。

**若提示「已在运行」但看不到托盘图标**：先执行 `open-flow stop` 停止旧版本，再 `open-flow start`。

**首次使用**：在「系统设置 → 隐私与安全性 → 辅助功能」中为终端开启权限，否则热键和粘贴不生效。

**从源码构建**（需 [Rust](https://rustup.rs/)）：`git clone https://github.com/jqlong17/open-flow.git && cd open-flow && cargo build --release`

---

## 常用命令

| 命令 | 说明 |
| --- | --- |
| `open-flow setup` | 手动下载模型（可选，start 会自动下载） |
| `open-flow start` | 前台启动（托盘图标，Ctrl+C 或菜单退出） |
| `open-flow stop` | 停止后台服务 |
| `open-flow status` | 查看状态、PID、日志路径 |
| `open-flow config show` | 查看当前配置 |
| `open-flow config set-model <path>` | 设置模型目录 |
| `open-flow transcribe --file <wav>` | 对单个音频文件做一次转写 |

---

## 当前限制

- `open-flow start` 为前台运行，需保持终端打开（或使用 `open-flow stop` 在另一终端停止）
- 主要面向 macOS，需麦克风与辅助功能权限
- 打包 .app、流式转写等仍在规划中

---

## 文档

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：系统架构、组件设计、音频管线、开发说明

---

## 参与贡献

欢迎 fork、提 issue、提交 PR，一起把开源语音输入体验做得更好。

---

## License

MIT
