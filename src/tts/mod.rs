use std::path::{Path, PathBuf};
use std::process::Command;

pub fn synthesize_to_mp3(text: &str) -> Result<PathBuf, String> {
    if text.trim().is_empty() {
        return Err("文本为空，无法转音频".to_string());
    }

    let say_bin =
        resolve_binary("say", &["/usr/bin/say"]).ok_or_else(|| "缺少系统命令: say".to_string())?;
    let ffmpeg_bin = resolve_binary(
        "ffmpeg",
        &[
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ],
    )
    .ok_or_else(|| "缺少 ffmpeg。请安装后重试（例如: brew install ffmpeg）".to_string())?;

    let home = std::env::var("HOME").map_err(|_| "无法获取 HOME 目录".to_string())?;
    let downloads = PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;

    let stamp = timestamp_string();
    let mp3_path = downloads.join(format!("open-flow-{}.mp3", stamp));
    let aiff_path = std::env::temp_dir().join(format!("open-flow-{}.aiff", stamp));
    let txt_path = std::env::temp_dir().join(format!("open-flow-{}.txt", stamp));

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
        return Err(format!("系统 TTS 生成失败: {}", stderr.trim()));
    }

    let ffmpeg_out = Command::new(&ffmpeg_bin)
        .arg("-y")
        .arg("-i")
        .arg(&aiff_path)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg("2")
        .arg(&mp3_path)
        .output()
        .map_err(|e| format!("执行 ffmpeg 失败: {}", e))?;

    if !ffmpeg_out.status.success() {
        let stderr = String::from_utf8_lossy(&ffmpeg_out.stderr);
        return Err(format!("音频转码失败: {}", stderr.trim()));
    }

    let _ = std::fs::remove_file(&aiff_path);
    let _ = std::fs::remove_file(&txt_path);

    Ok(mp3_path)
}

pub fn probe_runtime() -> Result<(), String> {
    let has_say = resolve_binary("say", &["/usr/bin/say"]).is_some();
    if !has_say {
        return Err("缺少系统命令: say".to_string());
    }

    let has_ffmpeg = resolve_binary(
        "ffmpeg",
        &[
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ],
    )
    .is_some();
    if !has_ffmpeg {
        return Err("缺少 ffmpeg。请安装后重试（brew install ffmpeg）".to_string());
    }

    Ok(())
}

fn resolve_binary(name: &str, candidates: &[&str]) -> Option<PathBuf> {
    for candidate in candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }

    let status = Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .ok()?;
    if !status.success() {
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
