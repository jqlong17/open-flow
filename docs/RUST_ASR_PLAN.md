# Rust ASR 与 Python 并行开发计划

目标：**Rust 版效果逼近 Python 版（FunASR 官方管线）**，同时**持续优化性能**。两版并行维护，以 Python 为效果参考。

---

## 1. 参考基准

| 项目 | 说明 |
|------|------|
| **效果参考** | Python 版在同一音频上的转写结果（当前 `testdata/mixed_zh_en.wav` → `早上好，good morning.😔`） |
| **固定回归音频** | `testdata/mixed_zh_en.wav` |
| **模型** | SenseVoiceSmall（Rust 用 ONNX，Python 用 FunASR PyTorch/ONNX） |

---

## 2. 阶段规划

### Phase A：效果对齐（当前重点）

**目标**：同一段音频，Rust 输出与 Python 语义一致（允许标点/表情符号略有差异）。

**手段**：
1. **对比流水线**：用脚本对同一 wav 分别跑 Rust 和 Python，对比输出（见 `scripts/compare_transcribe.sh`）。
2. **前处理对齐**：以 FunASR/Kaldi 为准，逐项核对 Rust 的 fbank、LFR、CMVN（公式、顺序、数值范围）。必要时用 Python 导出一段音频的中间特征，与 Rust 逐帧/逐维对比。
3. **解码与后处理**：确认 ONNX 输入（speech/speech_lengths/language/textnorm）与 Python 一致；CTC 解码与 Python 的 rich_transcription 后处理对齐（strip 标签、空格等）。
4. **可选**：若 Rust 仍长期“blank 占优”，可尝试用 Python 导出一份 ONNX + 同构前处理，在 Rust 里只做推理+解码，验证 pipeline 一致性后再反推前处理差异。

**验收**：`./scripts/compare_transcribe.sh` 下，Rust 输出与 Python 在语义上一致（或 WER/CER 可接受）。

---

### Phase B：回归与自动化

**目标**：效果不倒退、两版可长期对比。

**手段**：
1. **固定测试集**：在 `testdata/` 增加若干条 wav + 预期文本（或以 Python 输出为 golden）。
2. **对比脚本**：每次改 Rust 后跑 `compare_transcribe.sh`，看 diff 或简单 WER。
3. **CI（可选）**：在 GitHub Actions 中跑 Rust 转写 + 与 Python/golden 的 diff，仅在有模型缓存或跳过下载时跑。

**验收**：文档化“如何跑回归”，且 Rust 改动后能快速看到与 Python 的差异。

---

### Phase C：性能优化

**目标**：在效果与 Python 对齐的前提下，优化延迟与资源占用。

**手段**：
1. **预热**：daemon 启动时加载模型，避免首次按键才加载。
2. **量化**：若 ONNX 提供 INT8/FP16，在 Rust 侧启用并回归效果。
3. **流式（可选）**：边录边送帧、流式解码，降低首字延迟。
4. ** profiling**：用 flamegraph 等定位热点（特征计算 vs ONNX 推理），针对性优化。

**验收**：在相同音频上，Rust 端到端延迟 & 内存占用有可量化的提升，且回归脚本通过。

---

## 3. 当前可执行项（建议顺序）

1. **跑通对比脚本**：`scripts/compare_transcribe.sh` 能稳定跑出 Rust / Python 两段输出并打印 diff。
2. **修 Rust 前处理/解码**：根据 diff 和（可选）特征对比，把 Rust 输出往 Python 靠拢，直到 Phase A 验收通过。
3. **加 1～2 条回归用例**：再选一条 wav，记下 Python 输出为 golden，写入文档或脚本。
4. **再做性能优化**：Phase A/B 稳定后再做 Phase C，避免“优化完效果又偏了”。

---

## 4. 双版并行维护约定

| 项目 | Rust 版 | Python 版 |
|------|--------|-----------|
| **代码** | `src/asr/`, `src/cli/commands/transcribe.rs` 等 | `python/open_flow/` |
| **入口** | `open-flow transcribe --file <wav>` | `python -m open_flow transcribe -f <wav>` |
| **效果基准** | 以 Python 为参考，尽量一致 | 官方 FunASR，不随意改 |
| **性能** | 持续优化（预热、量化、流式） | 不强制，以效果为主 |
| **回归** | 同一脚本跑两版，对比输出 | 同上 |

---

## 5. 参考命令

```bash
# 对比同一音频的 Rust vs Python 输出（Rust 需 OPEN_FLOW_MODEL 或 config 中有 model_path）
OPEN_FLOW_MODEL=/path/to/sensevoice-small ./scripts/compare_transcribe.sh testdata/mixed_zh_en.wav
# 或先 open-flow config set-model /path/to/sensevoice-small
./scripts/compare_transcribe.sh testdata/mixed_zh_en.wav

# Rust 单跑
cargo run --release -- transcribe -f testdata/mixed_zh_en.wav -o stdout --model /path/to/sensevoice-small

# Python 单跑（需 python/.venv）
python/.venv/bin/python -m open_flow transcribe -f testdata/mixed_zh_en.wav -o stdout
```
