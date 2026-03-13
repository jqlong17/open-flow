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
- 后台常驻，使用时只需要按热键，不需要频繁切换窗口
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

- 适合：使用 macOS、愿意本地部署模型、重视开源和可控性的开发者
- 暂不适合：希望零配置即用、完全不关心模型准备过程、只想安装 GUI 应用的用户

---

## 快速开始

> 当前版本已经可用，但 **模型仍需手动准备**。如果你还没有本地 SenseVoice ONNX 模型，请先看 [模型与下载说明](docs/MODEL_DOWNLOAD_TODO.md)。

### 1. 安装

```bash
curl -sSL https://raw.githubusercontent.com/open-flow-project/open-flow/main/install.sh | sh
```

或从 [Releases](https://github.com/open-flow-project/open-flow/releases) 下载对应架构的二进制。

### 2. 配置模型路径

```bash
open-flow config set-model /path/to/sensevoice-small
```

### 3. 启动后台服务

```bash
open-flow start
```

### 4. 开启系统权限

首次使用请在「系统设置 -> 隐私与安全性 -> 辅助功能」中为终端或运行 `open-flow` 的应用开启权限，否则全局热键和自动粘贴不会生效。

### 5. 开始使用

- 按右侧 Command：开始录音
- 再按一次右侧 Command：停止录音并转写
- 转写结果会自动粘贴到当前焦点输入框

---

## 常用命令

| 命令 | 说明 |
| --- | --- |
| `open-flow start` | 启动后台服务 |
| `open-flow stop` | 停止后台服务 |
| `open-flow status` | 查看状态、PID、日志路径 |
| `open-flow config show` | 查看当前配置 |
| `open-flow config set-model <path>` | 设置模型目录 |
| `open-flow transcribe --file <wav>` | 对单个音频文件做一次转写 |

---

## 当前限制

- 模型自动下载尚未完成，当前需要用户手动准备本地模型目录
- 当前主要面向 macOS 使用，权限依赖系统的麦克风与辅助功能设置
- 菜单栏 App、模型预热、流式转写等体验优化仍在规划中

---

## 文档

### 用户文档

- [docs/MODEL_DOWNLOAD_TODO.md](docs/MODEL_DOWNLOAD_TODO.md)：模型下载与准备说明
- [docs/INDEX.md](docs/INDEX.md)：项目知识索引与文档导航

### 开发文档

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：系统设计、组件、音频管线、开发说明
- [docs/RUST_ASR_PLAN.md](docs/RUST_ASR_PLAN.md)：Rust / Python 效果对齐与性能计划

---

## 参与贡献

欢迎 fork、提 issue、提交 PR，一起把开源语音输入体验做得更好。

---

## License

MIT
