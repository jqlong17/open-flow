# Open Flow 知识索引

> 3层树形结构，方便快速定位任何信息。
> 更新规则：每次修改产品功能/文档/配置后，同步更新本文件对应条目。

---

## 1. 产品

### 1.1 核心功能（已实现 ✅）

| 功能 | 描述 | 关键文件 |
|------|------|---------|
| **热键监听** | 右侧 Command 键触发，基于 CGEventTap | `src/hotkey/mod.rs` |
| **实时录音** | 按键开流/停流，cpal 采集 | `src/audio/mod.rs` |
| **ASR 转写** | SenseVoiceSmall ONNX，~82ms，中英混合 | `src/asr/` |
| **文本注入** | 写入剪贴板 + 模拟 Cmd+V 粘贴 | `src/text_injection/mod.rs` |
| **后台 daemon** | setsid 脱离终端，PID 文件管理 | `src/cli/daemon.rs` |
| **CLI 控制** | start/stop/status/transcribe/config | `src/main.rs` |

### 1.2 CLI 命令速查

```bash
open-flow start                          # 后台启动 daemon
open-flow stop                           # 停止 daemon
open-flow status                         # 查看状态 + PID + 日志路径
open-flow config set-model <path>        # 设置模型路径
open-flow config show                    # 查看所有配置
open-flow transcribe --file x.wav        # 单次转写文件
open-flow transcribe --duration 5        # 录 5 秒再转写
```

### 1.3 运行时路径

| 类型 | 路径 |
|------|------|
| 配置文件 | `~/Library/Application Support/com.openflow.open-flow/config.toml` |
| PID 文件 | `~/Library/Application Support/com.openflow.open-flow/daemon.pid` |
| 日志文件 | `~/Library/Application Support/com.openflow.open-flow/daemon.log` |
| 当前模型 | `~/Library/Application Support/Shandianshuo/models/sensevoice-small/` |

### 1.4 待办（优先级排序）

| 优先级 | 功能 | 说明 |
|--------|------|------|
| P0 | 模型自动下载 | 见 `docs/MODEL_DOWNLOAD_TODO.md` |
| P0 | 模型预热 | daemon 启动时预加载，避免首次延迟 |
| P1 | 视觉/听觉反馈 | 录音中显示状态，转写完成有提示音 |
| P1 | 菜单栏 App | 不需要打开终端使用 |
| P2 | 流式转写 | 边录边送帧，降低首字延迟 |
| P2 | Homebrew 分发 | `brew install open-flow` |

---

## 2. 代码

### 2.1 Rust（`src/`）

```
src/
├── main.rs                    CLI 入口，clap 命令路由
├── asr/
│   ├── mod.rs                 AsrEngine：加载模型/调用推理/返回结果
│   ├── preprocess.rs          fbank→LFR→CMVN（N_FFT=512，dither=0，Kaldi 对齐）
│   ├── onnx_inference.rs      ORT session，输入 speech/speech_lengths/language/textnorm
│   └── decoder.rs             CTC 贪婪解码，blank=0，postprocess_tokens
├── hotkey/
│   └── mod.rs                 rdev::listen，监听 Key::MetaRight，Pressed 事件
├── audio/
│   └── mod.rs                 AudioCapture：build_live_stream/record_to_file/save_wav
├── daemon/
│   └── mod.rs                 Daemon 主循环：热键→start_recording→stop_and_transcribe
├── text_injection/
│   └── mod.rs                 arboard 写剪贴板 + rdev::simulate MetaLeft+V
├── cli/
│   ├── daemon.rs              start(spawn+setsid)/stop(SIGTERM)/status(PID 检查)
│   └── commands/
│       ├── config.rs          set-model/set-hotkey/show
│       └── transcribe.rs      单次转写文件或录音
└── common/
    ├── config.rs              Config toml，data_dir()/config_path()
    └── types.rs               RecordingState/HotkeyEvent/TranscriptionResult
```

### 2.2 Python（`python/`）—— golden 效果参考，不轻易改

```
python/
├── requirements.txt           funasr/torch/torchaudio/soundfile
└── open_flow/
    ├── asr.py                 SenseVoiceASR，FunASR AutoModel，官方管线
    └── cli.py                 transcribe 命令，-f/-o/-l/--no-itn
```

### 2.3 关键依赖（Cargo.toml）

| crate | 用途 |
|-------|------|
| `ort 2.0.0-rc.12` | ONNX Runtime 推理 |
| `rdev 0.5` | CGEventTap 热键 + simulate 按键 |
| `cpal 0.15` | 跨平台音频采集 |
| `arboard 3` | 剪贴板读写 |
| `realfft 3.4` | FFT 功率谱（fbank） |
| `libc 0.2` | setsid/kill 信号 |

---

## 3. 配置与知识

### 3.1 项目文档（`docs/`）

| 文件 | 内容 |
|------|------|
| `docs/INDEX.md` | **本文件**：3层知识索引 |
| `docs/CURRENT_STATE.md` | 当前项目状态快照（每次对话后更新） |
| `docs/ARCHITECTURE.md` | 架构与设计：系统架构、组件、音频管线、SenseVoice、macOS 集成、配置与开发指南 |
| `docs/sessions/INDEX.md` | 所有对话章节目录 |
| `docs/sessions/YYYY-MM-DD.md` | 每次对话的详细记录 |
| `docs/RUST_ASR_PLAN.md` | Phase A/B/C 效果对齐与性能优化计划 |
| `docs/MODEL_DOWNLOAD_TODO.md` | 模型下载问题备忘录（待解决） |

### 3.2 Cursor Skills（`~/.cursor/skills/`）

| skill | 触发时机 |
|-------|---------|
| `architecture-documentation` | 写架构文档/技术方案 |
| `code-documentation-generator` | 生成项目代码说明文档 |
| `markdown-format-guidelines` | 输出 Markdown 内容时 |
| `push-code-workflow` | 用户说「推送代码」时 |
| `technical-discussion` | 技术方案讨论（只读不改） |

### 3.3 Cursor Hooks（`.cursor/hooks/`）

| hook | 脚本 | 作用 |
|------|------|------|
| `sessionStart` | `session-start.sh` | 自动注入 `CURRENT_STATE.md`；若今日未更新索引则附加「今日尚未做记忆更新」提示 |

### 3.4 对比与回归工具

```bash
# 同一音频 Rust vs Python 效果对比
OPEN_FLOW_MODEL=/path/to/model ./scripts/compare_transcribe.sh testdata/mixed_zh_en.wav

# 导出 Python fbank 特征（调试用）
PYTHONPATH=python python/.venv/bin/python scripts/dump_features.py testdata/mixed_zh_en.wav
```

---

## 4. 架构与设计

> 面向开发与维护：概念、系统边界、技术选型。新用户入门见仓库根目录 [README.md](../README.md)。

### 4.1 设计文档

| 文档 | 内容 |
|------|------|
| [docs/ARCHITECTURE.md](ARCHITECTURE.md) | 系统架构图、Daemon/CLI/状态机、音频管线、SenseVoice 集成、macOS 权限与热键/粘贴实现、配置与构建、性能与安全、开发规范 |

### 4.2 与 README 的分工

| 文件 | 侧重 |
|------|------|
| **README.md** | 新用户：项目是什么、能做什么、如何安装与使用、常用命令；不展开技术实现 |
| **docs/ARCHITECTURE.md** | 设计与实现：架构、组件、管线、配置、开发与扩展 |
| **docs/INDEX.md** | 知识导航：产品/代码/配置/设计 的索引入口 |
