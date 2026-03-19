#!/usr/bin/env python3
import argparse
import copy
import glob
import os
import sys
from typing import Optional


def parse_args():
    p = argparse.ArgumentParser()
    p.add_argument("--text-file", required=True)
    p.add_argument("--wav-out", required=True)
    p.add_argument("--model", default="microsoft/VibeVoice-Realtime-0.5B")
    p.add_argument("--voice-file", default="")
    return p.parse_args()


def find_voice_file(explicit_path: str) -> Optional[str]:
    if explicit_path and os.path.exists(explicit_path):
        return explicit_path

    env_path = os.environ.get("OPEN_FLOW_TTS_VOICE_PATH", "").strip()
    if env_path and os.path.exists(env_path):
        return env_path

    roots = [
        os.path.expanduser(
            "~/Library/Application Support/com.openflow.open-flow/vibevoice"
        ),
        os.path.expanduser("~/.cache/huggingface"),
        os.getcwd(),
    ]
    found = []
    for root in roots:
        pattern = os.path.join(root, "**", "voices", "streaming_model", "**", "*.pt")
        found.extend(glob.glob(pattern, recursive=True))

    if not found:
        return None
    found.sort()
    return found[0]


def main() -> int:
    args = parse_args()

    with open(args.text_file, "r", encoding="utf-8") as f:
        text = f.read().strip()
    if not text:
        print("text is empty", file=sys.stderr)
        return 2

    try:
        import torch
        from vibevoice.modular.modeling_vibevoice_streaming_inference import (
            VibeVoiceStreamingForConditionalGenerationInference,
        )
        from vibevoice.processor.vibevoice_streaming_processor import (
            VibeVoiceStreamingProcessor,
        )
    except Exception as e:
        print(f"import failed: {e}", file=sys.stderr)
        return 3

    voice_file = find_voice_file(args.voice_file)
    if not voice_file:
        print("cannot find vibevoice voice embedding (.pt)", file=sys.stderr)
        return 4

    if torch.cuda.is_available():
        device = "cuda"
        dtype = torch.bfloat16
        attn = "flash_attention_2"
    elif torch.backends.mps.is_available():
        device = "mps"
        dtype = torch.float32
        attn = "sdpa"
    else:
        device = "cpu"
        dtype = torch.float32
        attn = "sdpa"

    processor = VibeVoiceStreamingProcessor.from_pretrained(args.model)

    if device == "mps":
        model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
            args.model,
            torch_dtype=dtype,
            attn_implementation=attn,
            device_map=None,
        )
        model.to("mps")
    else:
        model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
            args.model,
            torch_dtype=dtype,
            attn_implementation=attn,
            device_map=device,
        )

    model.eval()
    model.set_ddpm_inference_steps(num_steps=5)

    all_prefilled_outputs = torch.load(
        voice_file, map_location=device, weights_only=False
    )

    inputs = processor.process_input_with_cached_prompt(
        text=text,
        cached_prompt=all_prefilled_outputs,
        padding=True,
        return_tensors="pt",
        return_attention_mask=True,
    )
    for k, v in inputs.items():
        if torch.is_tensor(v):
            inputs[k] = v.to(device)

    outputs = model.generate(
        **inputs,
        max_new_tokens=None,
        cfg_scale=1.5,
        tokenizer=processor.tokenizer,
        generation_config={"do_sample": False},
        verbose=False,
        all_prefilled_outputs=copy.deepcopy(all_prefilled_outputs),
    )

    os.makedirs(os.path.dirname(os.path.abspath(args.wav_out)), exist_ok=True)
    processor.save_audio(outputs.speech_outputs[0], output_path=args.wav_out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
