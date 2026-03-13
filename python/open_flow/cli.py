"""
Open Flow (Python) CLI：转写文件或录音。
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .asr import SenseVoiceASR


def main():
    parser = argparse.ArgumentParser(
        description="Open Flow 语音转写 (Python)：SenseVoice 官方推理",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    sub = parser.add_subparsers(dest="command", help="命令")

    # transcribe
    p_transcribe = sub.add_parser("transcribe", help="转写音频文件（或录音后转写）")
    p_transcribe.add_argument(
        "--file", "-f",
        type=Path,
        default=None,
        help="输入 wav/mp3 等音频文件；不传则先录音再转写",
    )
    p_transcribe.add_argument(
        "--model", "-m",
        type=str,
        default=None,
        help="模型名或本地目录，默认 iic/SenseVoiceSmall；也可用环境变量 OPEN_FLOW_MODEL",
    )
    p_transcribe.add_argument(
        "--device",
        type=str,
        default="cpu",
        help="推理设备：cpu 或 cuda:0",
    )
    p_transcribe.add_argument(
        "--language", "-l",
        type=str,
        default="auto",
        choices=["auto", "zh", "en", "yue", "ja", "ko", "nospeech"],
        help="语种",
    )
    p_transcribe.add_argument(
        "--no-itn",
        action="store_true",
        help="关闭逆文本正则化（标点等）",
    )
    p_transcribe.add_argument(
        "--output", "-o",
        type=str,
        default="stdout",
        choices=["stdout", "clipboard"],
        help="输出方式",
    )
    p_transcribe.add_argument(
        "--duration", "-d",
        type=float,
        default=0,
        help="录音时长（秒），0 表示使用 --file 或提示错误",
    )

    args = parser.parse_args()

    if args.command != "transcribe":
        parser.print_help()
        return 0

    # 确定输入：文件优先
    audio_path = args.file
    if audio_path is None and args.duration > 0:
        try:
            audio_path = _record(args.duration)
        except Exception as e:
            print(f"录音失败: {e}", file=sys.stderr)
            return 1
    if audio_path is None:
        print("请指定 --file <音频文件> 或 --duration <秒数> 进行录音后转写。", file=sys.stderr)
        return 1

    if not audio_path.exists():
        print(f"文件不存在: {audio_path}", file=sys.stderr)
        return 1

    model = args.model or __import__("os").environ.get("OPEN_FLOW_MODEL", "iic/SenseVoiceSmall")
    device = args.device or __import__("os").environ.get("OPEN_FLOW_DEVICE", "cpu")

    print("正在转写...", file=sys.stderr)
    asr = SenseVoiceASR(model=model, device=device)
    text = asr.transcribe(audio_path, language=args.language, use_itn=not args.no_itn)

    if args.output == "stdout":
        print(text)
    elif args.output == "clipboard":
        try:
            import pyperclip
            pyperclip.copy(text)
            print("已复制到剪贴板。", file=sys.stderr)
            print(text)
        except ImportError:
            print("clipboard 需要: pip install pyperclip", file=sys.stderr)
            print(text)

    return 0


def _record(duration_sec: float) -> Path:
    """录音 duration_sec 秒，返回临时 wav 路径。"""
    import tempfile
    import wave
    import sounddevice as sd
    import numpy as np

    sample_rate = 16000
    rec = sd.rec(int(duration_sec * sample_rate), samplerate=sample_rate, channels=1, dtype=np.float32)
    sd.wait()
    path = Path(tempfile.mktemp(suffix=".wav"))
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes((np.clip(rec, -1, 1) * 32767).astype(np.int16).tobytes())
    return path


if __name__ == "__main__":
    sys.exit(main())
