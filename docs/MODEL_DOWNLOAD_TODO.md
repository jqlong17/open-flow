# 模型下载问题备忘录

> 目标：用户从零开始也能用，无需依赖第三方 App（如闪电硕）提供的模型文件。

---

## 现状

产品使用 **SenseVoiceSmall** 模型（阿里 FunASR），Rust 侧通过 ONNX 推理。

需要的文件（共 ~895MB）：

| 文件 | 大小 | 说明 |
|------|------|------|
| `model.onnx` | ~894MB | 主模型（ONNX 格式） |
| `tokens.json` | ~344KB | 词表 |
| `am.mvn` | ~11KB | 特征均值/方差（CMVN） |

当前配置方式：手动指定本地路径
```bash
open-flow config set-model /path/to/sensevoice-small
```

---

## 问题

### 1. 没有公开可直接下载的 ONNX 文件

- **ModelScope** `iic/SenseVoiceSmall`：只有 `model.pt`（PyTorch 格式），无 ONNX
- **HuggingFace** `FunAudioLLM/SenseVoiceSmall`：仓库为私有，HTTP 401
- 现有 ONNX 来源：闪电硕等第三方 App 自行导出并打包

### 2. FunASR 导出 ONNX 的 Python 环境不完整

FunASR 提供 `AutoModel.export(type='onnx')` 方法，但当前 Python venv 缺少依赖：

```
ModuleNotFoundError: No module named 'onnxscript'
```

**复现步骤：**
```bash
cd /Users/ruska/project/open-flow
PYTHONPATH=python python/.venv/bin/python -c "
from funasr import AutoModel
m = AutoModel(model='iic/SenseVoiceSmall', disable_update=True)
m.export(type='onnx', output_dir='/tmp/sensevoice_onnx')
"
# → ModuleNotFoundError: No module named 'onnxscript'
```

---

## 待解决的方案（选一个实现）

### 方案 A：补全 Python 依赖 + `open-flow model export` 命令 ⭐ 推荐

1. `python/.venv/bin/pip install onnxscript` 修复导出环境
2. 实现 `open-flow model export` 命令：
   - 调用 Python venv 的 FunASR 下载 `model.pt`（ModelScope）
   - 导出为 ONNX 到 `~/Library/Application Support/com.openflow.open-flow/models/sensevoice-small/`
   - 自动调用 `open-flow config set-model` 配置路径
3. 用户只需运行一条命令：
   ```bash
   open-flow model download   # 或 open-flow setup
   ```

**优点**：利用 FunASR 官方导出，模型来源可信  
**缺点**：需要 Python 环境；首次需要下载 ~900MB

### 方案 B：找到/托管公开 ONNX 文件，直接 HTTP 下载

- 找到可公开访问的 ONNX 托管地址（GitHub Release / 自建 CDN / R2）
- 用 Rust `reqwest` 直接下载，显示进度条（`indicatif`）
- 不依赖 Python 环境

**优点**：纯 Rust，用户体验最简洁  
**缺点**：需要解决模型文件的合规托管问题（~900MB，需确认 license）

### 方案 C：一键安装脚本（`install.sh`）

在 `install.sh` 里集成：
1. 下载 `open-flow` 二进制
2. 安装 Python + funasr + onnxscript
3. 导出 ONNX 模型
4. 自动配置路径

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `python/requirements.txt` | Python 依赖（需加 `onnxscript`） |
| `install.sh` | 安装脚本（待完善） |
| `src/cli/commands/config.rs` | `config set-model` 实现 |
| `src/asr/mod.rs` | ASR 引擎入口，加载模型路径 |

---

## 当前 workaround（临时方案）

对于已有模型的用户（如安装了闪电硕）：

```bash
open-flow config set-model "/Users/ruska/Library/Application Support/Shandianshuo/models/sensevoice-small"
open-flow start
```
