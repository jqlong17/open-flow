use crate::common::config::Config;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{error, info};

const DEFAULT_TTS_MODEL: &str = "microsoft/VibeVoice-Realtime-0.5B";
const VIBEVOICE_RUNTIME_PROBE: &str = r#"import torch
import soundfile
from vibevoice.modular.modeling_vibevoice_streaming_inference import VibeVoiceStreamingForConditionalGenerationInference
from vibevoice.processor.vibevoice_streaming_processor import VibeVoiceStreamingProcessor
"#;

pub fn synthesize_to_mp3(text: &str) -> Result<PathBuf, String> {
    if text.trim().is_empty() {
        return Err("文本为空，无法转音频".to_string());
    }

    let config = Config::load().unwrap_or_default();
    let provider = normalized_provider(&config.tts_provider);
    info!(
        "[TTS] synthesize request provider={} text_chars={}",
        provider,
        text.chars().count()
    );

    probe_runtime_for_provider(provider, &config)?;

    let out = if provider == "local_model" {
        synthesize_with_local_model(text, &config)
    } else {
        synthesize_with_system_tts(text)
    };

    match &out {
        Ok(path) => info!(
            "[TTS] synthesize success provider={} output={}",
            provider,
            path.display()
        ),
        Err(err) => error!(
            "[TTS] synthesize failed provider={} error={}",
            provider, err
        ),
    }

    out
}

pub fn probe_runtime() -> Result<(), String> {
    let config = Config::load().unwrap_or_default();
    probe_runtime_for_provider(normalized_provider(&config.tts_provider), &config)
}

fn probe_runtime_for_provider(provider: &str, config: &Config) -> Result<(), String> {
    info!("[TTS] dependency check start provider={}", provider);

    if provider == "local_model" {
        let _python = resolve_python_for_vibevoice()?;
        let _ffmpeg = resolve_binary_checked(
            "ffmpeg",
            &[
                "/opt/homebrew/bin/ffmpeg",
                "/usr/local/bin/ffmpeg",
                "/usr/bin/ffmpeg",
            ],
        )?;

        let script = app_resource_script_path().ok_or_else(|| "找不到 TTS 脚本路径".to_string())?;
        if !script.exists() {
            return Err(format!(
                "TTS 脚本不存在: {} (failed command: test -f {})",
                script.display(),
                script.display()
            ));
        }
        info!("[TTS] check bundled script ok path={}", script.display());

        if !config.tts_voice_path.trim().is_empty() {
            let voice = PathBuf::from(config.tts_voice_path.trim());
            if !voice.exists() {
                return Err(format!(
                    "voice embedding 不存在: {} (failed command: test -f {})",
                    voice.display(),
                    voice.display()
                ));
            }
            info!("[TTS] check voice file ok path={}", voice.display());
        } else {
            info!("[TTS] check voice file auto-detect mode");
        }

        info!("[TTS] dependency check ok provider=local_model");
        Ok(())
    } else {
        let _say = resolve_binary_checked("say", &["/usr/bin/say"])?;
        let _ffmpeg = resolve_binary_checked(
            "ffmpeg",
            &[
                "/opt/homebrew/bin/ffmpeg",
                "/usr/local/bin/ffmpeg",
                "/usr/bin/ffmpeg",
            ],
        )?;
        info!("[TTS] dependency check ok provider=system");
        Ok(())
    }
}

fn synthesize_with_system_tts(text: &str) -> Result<PathBuf, String> {
    let say_bin = resolve_binary_checked("say", &["/usr/bin/say"])?;
    let ffmpeg_bin = resolve_binary_checked(
        "ffmpeg",
        &[
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ],
    )?;

    let (mp3_path, txt_path, wav_path) = prepare_output_paths()?;
    let aiff_path = wav_path.with_extension("aiff");
    std::fs::write(&txt_path, text).map_err(|e| e.to_string())?;

    let say_out = Command::new(&say_bin)
        .arg("-f")
        .arg(&txt_path)
        .arg("-o")
        .arg(&aiff_path)
        .output()
        .map_err(|e| format!("执行 say 失败: {}", e))?;

    if !say_out.status.success() {
        let stderr = String::from_utf8_lossy(&say_out.stderr);
        return Err(format!(
            "系统 TTS 生成失败: {} (failed command: {} -f {} -o {})",
            stderr.trim(),
            say_bin.display(),
            txt_path.display(),
            aiff_path.display()
        ));
    }

    convert_to_mp3(&ffmpeg_bin, &aiff_path, &mp3_path)?;

    let _ = std::fs::remove_file(&aiff_path);
    let _ = std::fs::remove_file(&txt_path);

    Ok(mp3_path)
}

fn synthesize_with_local_model(text: &str, config: &Config) -> Result<PathBuf, String> {
    let python_bin = resolve_python_for_vibevoice()?;
    let ffmpeg_bin = resolve_binary_checked(
        "ffmpeg",
        &[
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ],
    )?;

    let script = app_resource_script_path().ok_or_else(|| "找不到 TTS 脚本路径".to_string())?;
    if !script.exists() {
        return Err(format!(
            "TTS 脚本不存在: {} (failed command: test -f {})",
            script.display(),
            script.display()
        ));
    }

    let model = if config.tts_model.trim().is_empty() {
        DEFAULT_TTS_MODEL.to_string()
    } else {
        config.tts_model.trim().to_string()
    };

    let (mp3_path, txt_path, wav_path) = prepare_output_paths()?;
    std::fs::write(&txt_path, text).map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&python_bin);
    cmd.arg(&script)
        .arg("--text-file")
        .arg(&txt_path)
        .arg("--wav-out")
        .arg(&wav_path)
        .arg("--model")
        .arg(&model);

    configure_python_model_cache_env(&mut cmd)?;

    if !config.tts_voice_path.trim().is_empty() {
        cmd.arg("--voice-file").arg(config.tts_voice_path.trim());
    }

    let py_out = cmd
        .output()
        .map_err(|e| format!("执行 Python TTS 失败: {}", e))?;

    if !py_out.status.success() {
        let stdout = String::from_utf8_lossy(&py_out.stdout);
        let stderr = String::from_utf8_lossy(&py_out.stderr);
        return Err(format!(
            "本地模型 TTS 失败\nstdout: {}\nstderr: {}\nfailed command: {} {} --text-file {} --wav-out {} --model {}",
            stdout.trim(),
            stderr.trim(),
            python_bin.display(),
            script.display(),
            txt_path.display(),
            wav_path.display(),
            model
        ));
    }

    convert_to_mp3(&ffmpeg_bin, &wav_path, &mp3_path)?;

    let _ = std::fs::remove_file(&wav_path);
    let _ = std::fs::remove_file(&txt_path);

    Ok(mp3_path)
}

fn configure_python_model_cache_env(cmd: &mut Command) -> Result<(), String> {
    let data_dir = Config::data_dir().map_err(|e| format!("读取 data dir 失败: {}", e))?;
    let hf_home = data_dir.join("hf_cache");
    let hf_hub_cache = hf_home.join("hub");
    let transformers_cache = hf_home.join("transformers");

    std::fs::create_dir_all(&hf_hub_cache)
        .map_err(|e| format!("创建 HuggingFace 缓存目录失败: {}", e))?;
    std::fs::create_dir_all(&transformers_cache)
        .map_err(|e| format!("创建 transformers 缓存目录失败: {}", e))?;

    cmd.env("HF_HOME", &hf_home);
    cmd.env("HF_HUB_CACHE", &hf_hub_cache);
    cmd.env("TRANSFORMERS_CACHE", &transformers_cache);

    Ok(())
}

fn resolve_python_for_vibevoice() -> Result<PathBuf, String> {
    let candidates = python_candidates();
    if candidates.is_empty() {
        return Err("缺少 python3 (failed command: command -v python3)".to_string());
    }

    info!(
        "[TTS] python candidates={}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    let mut errors = Vec::<String>::new();
    for py in &candidates {
        match check_python_runtime_detail(py) {
            Ok(_) => {
                info!("[TTS] check python runtime ok python={}", py.display());
                return Ok(py.clone());
            }
            Err(detail) => {
                error!("[TTS] python runtime check failed {}", detail);
                errors.push(detail);
            }
        }
    }

    Err(format!(
        "缺少本地模型 TTS 依赖（需要 torch + vibevoice）。请到设置页 Provider -> Text-to-Speech Provider 点击 Download 安装。\n{}",
        errors.join("\n")
    ))
}

fn python_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    if let Some(path) = env_python_override() {
        push_unique_existing_path(&mut candidates, path);
    }

    if let Some(path) = tts_venv_python_candidate() {
        push_unique_existing_path(&mut candidates, path);
    }

    if let Some(path) = resolve_binary("python3", &[]) {
        push_unique_existing_path(&mut candidates, path);
    }

    for p in [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ] {
        push_unique_existing_path(&mut candidates, PathBuf::from(p));
    }

    candidates
}

fn env_python_override() -> Option<PathBuf> {
    let value = std::env::var("OPEN_FLOW_TTS_PYTHON").ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn tts_venv_python_candidate() -> Option<PathBuf> {
    let data_dir = Config::data_dir().ok()?;
    Some(data_dir.join("tts-pyenv").join("bin").join("python3"))
}

fn push_unique_existing_path(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if path.exists() && !candidates.iter().any(|x| x == &path) {
        candidates.push(path);
    }
}

fn check_python_runtime_detail(python: &Path) -> Result<(), String> {
    let out = Command::new(python)
        .arg("-c")
        .arg(VIBEVOICE_RUNTIME_PROBE)
        .output();
    match out {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "failed command: {} -c <vibevoice runtime probe>; stderr: {}",
                python.display(),
                stderr.trim()
            ))
        }
        Err(err) => Err(format!(
            "failed command: {} -c <vibevoice runtime probe>; spawn error: {}",
            python.display(),
            err
        )),
    }
}

fn prepare_output_paths() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;
    let downloads = PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;

    let stamp = timestamp_string();
    let mp3_path = downloads.join(format!("open-flow-{}.mp3", stamp));
    let txt_path = std::env::temp_dir().join(format!("open-flow-{}.txt", stamp));
    let wav_path = std::env::temp_dir().join(format!("open-flow-{}.wav", stamp));
    Ok((mp3_path, txt_path, wav_path))
}

fn convert_to_mp3(ffmpeg_bin: &Path, wav_path: &Path, mp3_path: &Path) -> Result<(), String> {
    let ffmpeg_out = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-i")
        .arg(wav_path)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg("2")
        .arg(mp3_path)
        .output()
        .map_err(|e| format!("执行 ffmpeg 失败: {}", e))?;

    if ffmpeg_out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&ffmpeg_out.stderr);
        Err(format!(
            "音频转码失败: {} (failed command: {} -y -i {} -codec:a libmp3lame -q:a 2 {})",
            stderr.trim(),
            ffmpeg_bin.display(),
            wav_path.display(),
            mp3_path.display()
        ))
    }
}

fn resolve_binary_checked(name: &str, candidates: &[&str]) -> Result<PathBuf, String> {
    match resolve_binary(name, candidates) {
        Some(path) => {
            info!("[TTS] check {} ok path={}", name, path.display());
            Ok(path)
        }
        None => Err(format!(
            "缺少命令: {} (failed command: command -v {})",
            name, name
        )),
    }
}

fn resolve_binary(name: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Some(path) = resolve_command_v(name) {
        return Some(path);
    }

    for candidate in candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn resolve_command_v(name: &str) -> Option<PathBuf> {
    let check = Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .ok()?;
    if !check.success() {
        return None;
    }

    let out = Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {}", name))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn app_resource_script_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resources = exe.parent()?.parent()?.join("Resources");
    Some(resources.join("vibevoice_tts.py"))
}

fn timestamp_string() -> String {
    let out = Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    out.unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{}", secs)
    })
}

fn normalized_provider(v: &str) -> &str {
    let s = v.trim().to_lowercase();
    if s == "local_model" || s == "local" || s == "vibevoice" {
        "local_model"
    } else {
        "system"
    }
}

pub fn audio_duration_secs(path: &Path) -> Option<f64> {
    let ffprobe = resolve_binary(
        "ffprobe",
        &[
            "/opt/homebrew/bin/ffprobe",
            "/usr/local/bin/ffprobe",
            "/usr/bin/ffprobe",
        ],
    )?;

    let out = Command::new(ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    s.trim().parse::<f64>().ok()
}
