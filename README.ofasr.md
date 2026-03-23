# ofasr

`ofasr` 是一个基于 Open Flow ASR 核心抽出的独立命令行工具，用来把本地音频文件转成文本。

它只做一件事：

- 输入一个音频文件
- 调用本地 SenseVoice 模型转写
- 输出文本或 JSON

当前打包产物为：

- `macOS arm64`

如果你要发给别人直接用，需要满足这两个条件：

- 对方也是 `macOS arm64`
- 对方机器可以联网下载模型，或者你另外把模型目录也一起给他

## 1. 快速开始

把压缩包解压后，在终端进入目录：

```bash
cd ofasr-v0.2.3-macos-arm64
```

先检查工具是否正常：

```bash
./ofasr check
```

如果本地还没有模型，第一次运行会自动下载模型。

也可以手动先下载：

```bash
./ofasr setup --preset quantized
```

下载完成后，转写一个 wav 文件：

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav
```

如果希望输出 JSON：

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav --json
```

## 2. 支持的命令

### 转写音频

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav
```

常用参数：

- `--file`：要转写的音频文件路径
- `--model`：指定模型目录
- `--json`：以 JSON 格式输出结果

示例：

```bash
./ofasr transcribe --file /Users/you/Desktop/demo.wav --json
```

输出示例：

```json
{
  "text": "你好，这是转写结果。",
  "confidence": 0.95,
  "language": "zh",
  "duration_ms": 87
}
```

### 下载模型

```bash
./ofasr setup --preset quantized
```

可选参数：

- `--preset quantized`：默认量化版，体积更小
- `--preset fp16`：更大的 FP16 版本
- `--model-dir /path/to/model`：下载到指定目录
- `--force`：强制重新下载

### 检查模型状态

```bash
./ofasr check
```

如果希望机器可读输出：

```bash
./ofasr check --json
```

## 3. 模型说明

默认情况下，`ofasr` 不会把模型直接打包进二进制。

也就是说：

- 你发给别人的压缩包里主要是程序本身
- 对方第一次运行时会自动下载模型
- 模型会下载到系统默认数据目录

如果你想完全离线交付，也可以把模型目录一起发给对方，然后让对方这样使用：

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav --model /absolute/path/to/model-dir
```

## 4. 注意事项

- 当前这版底层文件读取逻辑优先面向 `wav`
- 最稳妥的输入格式是 `16kHz / 单声道 wav`
- 如果输入是别的格式，建议先自行转成 wav 再使用
- 首次加载模型会比后续慢一些

## 5. 常见问题

### 运行时报“音频文件不存在”

请确认 `--file` 后面传的是绝对路径，且文件真实存在。

### 首次运行比较慢

通常是因为正在下载模型，或者第一次加载 ONNX 模型。

### 能不能让大模型直接调用它

可以。最简单的方式是让上层工具执行：

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav --json
```

因为 JSON 输出比较稳定，适合给 AI 工具解析。

## 6. 我给别人时该怎么说

可以直接告诉对方：

1. 解压压缩包
2. 在终端进入目录
3. 先运行 `./ofasr check`
4. 再运行 `./ofasr transcribe --file /绝对路径/xxx.wav`

如果对方也是工程师，你也可以直接给他这条命令：

```bash
./ofasr transcribe --file /absolute/path/to/audio.wav --json
```
