#!/usr/bin/env python3
"""
导出 FunASR WavFrontend 的 fbank/LFR/CMVN 特征，供 Rust 对比。
用法: PYTHONPATH=python python/.venv/bin/python scripts/dump_features.py testdata/mixed_zh_en.wav
"""
import sys
import os
import numpy as np
import json

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))

def main():
    import torch
    import torchaudio

    wav_path = sys.argv[1] if len(sys.argv) > 1 else "testdata/mixed_zh_en.wav"
    out_dir = sys.argv[2] if len(sys.argv) > 2 else "/tmp/open_flow_features"
    os.makedirs(out_dir, exist_ok=True)

    # 1. 加载音频（用 soundfile 避免 torchcodec 依赖）
    import soundfile as sf
    samples, sr = sf.read(wav_path, dtype='float32')
    if samples.ndim > 1:
        samples = samples.mean(axis=1)
    waveform = torch.from_numpy(samples).unsqueeze(0)  # [1, N]
    print(f"原始音频: shape={waveform.shape}, sr={sr}")
    if sr != 16000:
        waveform = torchaudio.functional.resample(waveform, sr, 16000)
        sr = 16000
    if waveform.shape[0] > 1:
        waveform = waveform.mean(0, keepdim=True)
    print(f"重采样后: shape={waveform.shape}, sr={sr}")

    # 2. 计算 fbank（与 FunASR WavFrontend 参数完全一致）
    fbank = torchaudio.compliance.kaldi.fbank(
        waveform,
        num_mel_bins=80,
        frame_length=25,
        frame_shift=10,
        dither=0.0,
        sample_frequency=16000,
        window_type="hamming",
        preemphasis_coefficient=0.97,
        energy_floor=1.0,      # 注意：FunASR energy_floor=1.0
    )
    print(f"fbank: shape={fbank.shape}")  # [T, 80]

    # 3. LFR (Low Frame Rate): m=7, n=6
    T, D = fbank.shape
    M, N = 7, 6
    left_pad = (M - 1) // 2  # 3
    T_eff = T + left_pad
    T_lfr = (T_eff + N - 1) // N

    lfr = torch.zeros(T_lfr, M * D)
    for i in range(T_lfr):
        for j in range(M):
            idx = i * N + j
            if idx < left_pad:
                src = 0
            elif idx - left_pad < T:
                src = idx - left_pad
            else:
                src = T - 1
            lfr[i, j * D:(j + 1) * D] = fbank[src]
    print(f"LFR: shape={lfr.shape}")  # [T_lfr, 560]

    # 4. CMVN（从 am.mvn 文件读取，与 Rust 一致）
    model_dir = os.environ.get("OPEN_FLOW_MODEL_DIR", None)
    cmvn_path = None
    if model_dir and os.path.exists(os.path.join(model_dir, "am.mvn")):
        cmvn_path = os.path.join(model_dir, "am.mvn")
    else:
        # 从 funasr 缓存目录找
        cache_dir = os.path.expanduser("~/.cache/modelscope/hub/models/iic/SenseVoiceSmall")
        candidate = os.path.join(cache_dir, "am.mvn")
        if os.path.exists(candidate):
            cmvn_path = candidate
    if cmvn_path:
        print(f"加载 CMVN: {cmvn_path}")
        shift, scale = parse_kaldi_cmvn(cmvn_path)
        shift_t = torch.tensor(shift)
        scale_t = torch.tensor(scale)
        lfr_cmvn = (lfr + shift_t) * scale_t
    else:
        print("未找到 am.mvn，跳过 CMVN")
        lfr_cmvn = lfr

    print(f"CMVN 后: shape={lfr_cmvn.shape}")

    # 5. 保存
    np.save(os.path.join(out_dir, "fbank.npy"), fbank.numpy())
    np.save(os.path.join(out_dir, "lfr.npy"), lfr.numpy())
    np.save(os.path.join(out_dir, "lfr_cmvn.npy"), lfr_cmvn.numpy())

    # 打印前几帧统计
    print("\n--- fbank[:3] 均值/std ---")
    for i in range(min(3, fbank.shape[0])):
        row = fbank[i].numpy()
        print(f"  frame {i}: mean={row.mean():.4f}, std={row.std():.4f}, min={row.min():.4f}, max={row.max():.4f}")

    print("\n--- lfr_cmvn[:3] 均值/std ---")
    for i in range(min(3, lfr_cmvn.shape[0])):
        row = lfr_cmvn[i].numpy()
        print(f"  frame {i}: mean={row.mean():.4f}, std={row.std():.4f}, min={row.min():.4f}, max={row.max():.4f}")

    print(f"\n特征已保存到: {out_dir}")

    # 6. 打印 torchaudio kaldi 实际使用的 FFT 参数（推断）
    import math
    frame_len_samples = int(16000 * 25 / 1000)  # 400
    padded = 2 ** math.ceil(math.log2(frame_len_samples))  # 512
    print(f"\ntorchaudio kaldi 实际 FFT 大小: {padded} (frame_length={frame_len_samples} → 下一个 2^n)")
    print(f"  频率分辨率: {16000/padded:.2f} Hz/bin")
    print(f"  n_freqs: {padded//2+1}")


def parse_kaldi_cmvn(path):
    content = open(path).read()

    def extract_after(content, marker):
        m = content.find(marker)
        if m < 0:
            return None
        start = content.find('[', m) + 1
        end = content.find(']', start)
        vals = [float(x) for x in content[start:end].split()]
        return vals if vals else None

    shift = extract_after(content, "<AddShift>")
    scale = extract_after(content, "<Rescale>")
    return shift, scale


if __name__ == "__main__":
    main()
