"""
SenseVoice 转写：使用 FunASR 官方接口，保证效果与官方一致。
支持本地模型目录或 hub 模型名。
"""
from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

# 延迟导入，避免未安装 funasr 时 import 即报错
def _get_model():
    from funasr import AutoModel
    return AutoModel

def _get_postprocess():
    from funasr.utils.postprocess_utils import rich_transcription_postprocess
    return rich_transcription_postprocess


class SenseVoiceASR:
    """基于 FunASR 的 SenseVoice 转写（官方管线，效果稳定）。"""

    def __init__(
        self,
        model: str = "iic/SenseVoiceSmall",
        device: str = "cpu",
        vad_model: str = "fsmn-vad",
        vad_kwargs: Optional[dict] = None,
    ):
        """
        model: 模型名（如 iic/SenseVoiceSmall）或本地目录路径（含 model 配置的 SenseVoice 目录）
        device: "cpu" 或 "cuda:0"
        """
        self.model_id = model
        self.device = device
        self.vad_model = vad_model
        self.vad_kwargs = vad_kwargs or {"max_single_segment_time": 30000}
        self._model = None

    def _ensure_loaded(self):
        if self._model is not None:
            return
        AutoModel = _get_model()
        self._model = AutoModel(
            model=self.model_id,
            trust_remote_code=True,
            device=self.device,
            vad_model=self.vad_model,
            vad_kwargs=self.vad_kwargs,
        )

    def transcribe(
        self,
        audio_input: str | Path,
        language: str = "auto",
        use_itn: bool = True,
    ) -> str:
        """
        转写音频文件或 URL。
        audio_input: 本地路径或 URL
        language: "auto" | "zh" | "en" | "yue" | "ja" | "ko" | "nospeech"
        返回纯文本（已做 rich_transcription 后处理）。
        """
        self._ensure_loaded()
        postprocess = _get_postprocess()
        res = self._model.generate(
            input=str(audio_input),
            cache={},
            language=language,
            use_itn=use_itn,
            batch_size_s=60,
            merge_vad=True,
            merge_length_s=15,
        )
        if not res or not res[0].get("text"):
            return ""
        text = postprocess(res[0]["text"])
        return text.strip()

    def transcribe_bytes(self, wav_bytes: bytes, language: str = "auto", use_itn: bool = True) -> str:
        """从 wav 字节流转写（先写临时文件再调 generate）。"""
        import tempfile
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
            f.write(wav_bytes)
            path = f.name
        try:
            return self.transcribe(path, language=language, use_itn=use_itn)
        finally:
            os.unlink(path)


def transcribe_file(
    path: str | Path,
    model: str = "iic/SenseVoiceSmall",
    device: str = "cpu",
    language: str = "auto",
    use_itn: bool = True,
) -> str:
    """
    单次转写：读文件并返回文本。
    可通过环境变量 OPEN_FLOW_MODEL 覆盖 model，OPEN_FLOW_DEVICE 覆盖 device。
    """
    model = os.environ.get("OPEN_FLOW_MODEL", model)
    device = os.environ.get("OPEN_FLOW_DEVICE", device)
    asr = SenseVoiceASR(model=model, device=device)
    return asr.transcribe(path, language=language, use_itn=use_itn)
