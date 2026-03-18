use std::path::{Path, PathBuf};
use std::process::Command;

pub const VIBEVOICE_MODEL_ID: &str = "microsoft/VibeVoice-Realtime-0.5B";

pub fn synthesize_to_mp3(text: &str) -> Result<PathBuf, String> {
    if text.trim().is_empty() {
        return Err("文本为空，无法转音频".to_string());
    }

    let script = app_resource_script_path().ok_or_else(|| "找不到 TTS 脚本路径".to_string())?;
    if !script.exists() {
        return Err(format!("TTS 脚本不存在: {}", script.display()));
    }

    ensure_command("python3")?;
    ensure_command("ffmpeg")?;
    ensure_python_runtime()?;

    let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;
    let downloads = PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;

    let stamp = timestamp_string();
    let mp3_path = downloads.join(format!("open-flow-{}.mp3", stamp));
    let wav_path = std::env::temp_dir().join(format!("open-flow-{}.wav", stamp));
    let txt_path = std::env::temp_dir().join(format!("open-flow-{}.txt", stamp));

    std::fs::write(&txt_path, text).map_err(|e| e.to_string())?;

    let py = Command::new("python3")
        .arg(script)
        .arg("--text-file")
        .arg(&txt_path)
        .arg("--wav-out")
        .arg(&wav_path)
        .arg("--model")
        .arg(VIBEVOICE_MODEL_ID)
        .output()
        .map_err(|e| format!("执行 python3 失败: {}", e))?;

    if !py.status.success() {
        let stderr = String::from_utf8_lossy(&py.stderr);
        let stdout = String::from_utf8_lossy(&py.stdout);
        return Err(format!(
            "VibeVoice 推理失败\nstdout: {}\nstderr: {}",
            stdout, stderr
        ));
    }

    let ffmpeg = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(&wav_path)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg("2")
        .arg(&mp3_path)
        .output()
        .map_err(|e| format!("执行 ffmpeg 失败: {}", e))?;

    if !ffmpeg.status.success() {
        let stderr = String::from_utf8_lossy(&ffmpeg.stderr);
        return Err(format!("wav 转 mp3 失败: {}", stderr));
    }

    let _ = std::fs::remove_file(&wav_path);
    let _ = std::fs::remove_file(&txt_path);

    Ok(mp3_path)
}

pub fn app_resource_script_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resources = exe.parent()?.parent()?.join("Resources");
    Some(resources.join("vibevoice_tts.py"))
}

fn ensure_command(name: &str) -> Result<(), String> {
    let status = Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map_err(|e| format!("检测 {} 失败: {}", name, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("缺少命令: {}", name))
    }
}

fn ensure_python_runtime() -> Result<(), String> {
    let code = "import torch; import vibevoice";
    let out = Command::new("python3")
        .arg("-c")
        .arg(code)
        .output()
        .map_err(|e| format!("检测 Python 运行时失败: {}", e))?;

    if out.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(format!(
        "Python 依赖未就绪（需要 torch + vibevoice）。建议执行:\n  git clone https://github.com/microsoft/VibeVoice.git\n  cd VibeVoice\n  pip install -e .[streamingtts]\n错误: {}",
        stderr.trim()
    ))
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

pub fn probe_runtime() -> Result<(), String> {
    let script = app_resource_script_path().ok_or_else(|| "找不到 TTS 脚本路径".to_string())?;
    if !script.exists() {
        return Err(format!("TTS 脚本不存在: {}", script.display()));
    }
    ensure_command("python3")?;
    ensure_command("ffmpeg")?;
    ensure_python_runtime()?;
    Ok(())
}

pub fn audio_duration_secs(path: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
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
