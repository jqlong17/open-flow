# Open Flow（Python 版）

使用 **FunASR/SenseVoice 官方接口** 做语音转写，效果与官方一致，无需自研前处理与解码。

## 安装

```bash
cd python
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

首次运行会从 ModelScope 拉取模型（约 900MB，如 `iic/SenseVoiceSmall`），需网络；若缺 `torch`/`torchaudio` 请先 `pip install torch torchaudio`。

## 使用

### 转写本地文件

```bash
# 在 python 目录下（推荐）
cd open-flow/python
pip install -r requirements.txt
python -m open_flow transcribe --file ../testdata/mixed_zh_en.wav --output stdout

# 或在仓库根目录
cd open-flow
PYTHONPATH=python python -m open_flow transcribe --file testdata/mixed_zh_en.wav --output stdout

# 安装为包后可在任意目录调用
pip install -e python
open-flow-py transcribe -f /path/to/audio.wav -o stdout
```

### 指定模型

```bash
# 使用环境变量
export OPEN_FLOW_MODEL=iic/SenseVoiceSmall
python -m open_flow transcribe -f your.wav

# 或命令行
python -m open_flow transcribe -f your.wav --model iic/SenseVoiceSmall
```

### 语种与输出

```bash
python -m open_flow transcribe -f zh.wav --language zh
python -m open_flow transcribe -f en.wav --language en -o clipboard
```

### 先录音再转写

```bash
python -m open_flow transcribe --duration 5 --output stdout
```

（需安装 `sounddevice`，且系统有麦克风权限。）

## 环境变量

| 变量 | 说明 |
|------|------|
| `OPEN_FLOW_MODEL` | 模型名或本地路径，默认 `iic/SenseVoiceSmall` |
| `OPEN_FLOW_DEVICE` | 推理设备，默认 `cpu` |

## 与 Rust 版对比

| 项目 | Rust 版 | Python 版 |
|------|--------|-----------|
| 效果 | 依赖自研前处理对齐，需兜底策略 | 官方管线，效果稳定 |
| 依赖 | 仅 Rust 运行时 | Python + FunASR/ModelScope |
| 性能 | 常驻更省内存、冷启动快 | 推理耗时与 Rust 同量级（同 ONNX），首启略慢 |
| 适用 | 追求单进程、无 Python 环境 | 追求效果与开发效率 |
