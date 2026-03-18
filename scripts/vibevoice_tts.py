#!/usr/bin/env python3
# pyright: reportMissingImports=false
import argparse
import copy
import glob
import os
import sys


def find_voice_file() -> str | None:
    candidates = []

    env_voice = os.environ.get("OPEN_FLOW_VIBEVOICE_VOICE")
    if env_voice and os.path.exists(env_voice):
        return env_voice

    search_roots = [
        os.path.expanduser(
            "~/Library/Application Support/com.openflow.open-flow/vibevoice"
        ),
        os.path.expanduser("~/.cache/vibevoice"),
        os.getcwd(),
    ]

    for root in search_roots:
        pattern = os.path.join(root, "**", "voices", "streaming_model", "**", "*.pt")
        candidates.extend(glob.glob(pattern, recursive=True))

    if not candidates:
        return None

    candidates.sort()
    return candidates[0]


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Open Flow VibeVoice TTS helper")
    p.add_argument("--text-file", required=True)
    p.add_argument("--wav-out", required=True)
    p.add_argument("--model", default="microsoft/VibeVoice-Realtime-0.5B")
    return p.parse_args()


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
        print(
            "Missing VibeVoice runtime. Install with:\n"
            "  git clone https://github.com/microsoft/VibeVoice.git\n"
            "  cd VibeVoice\n"
            "  pip install -e .[streamingtts]\n"
            f"Import error: {e}",
            file=sys.stderr,
        )
        return 3

    voice_file = find_voice_file()
    if not voice_file:
        print(
            "Cannot find streaming voice embedding (.pt).\n"
            "Set OPEN_FLOW_VIBEVOICE_VOICE to a valid .pt file, or keep a VibeVoice repo checkout with demo/voices/streaming_model.",
            file=sys.stderr,
        )
        return 4

    device = (
        "cuda"
        if torch.cuda.is_available()
        else ("mps" if torch.backends.mps.is_available() else "cpu")
    )
    if device == "cuda":
        dtype = torch.bfloat16
        attn_impl = "flash_attention_2"
    else:
        dtype = torch.float32
        attn_impl = "sdpa"

    processor = VibeVoiceStreamingProcessor.from_pretrained(args.model)

    if device == "mps":
        model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
            args.model,
            torch_dtype=dtype,
            attn_implementation=attn_impl,
            device_map=None,
        )
        model.to("mps")
    else:
        model = VibeVoiceStreamingForConditionalGenerationInference.from_pretrained(
            args.model,
            torch_dtype=dtype,
            attn_implementation=attn_impl,
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
