use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::common::config::{Config, ModelPreset};

/// 量化版：Hugging Face haixuantao/SenseVoiceSmall-onnx
const MODEL_BASE_QUANTIZED: &str =
    "https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main";

/// 量化版需要下载的文件：(远端文件名, 本地保存名, 预期大小描述)
const MODEL_FILES_QUANTIZED: &[(&str, &str, &str)] = &[
    ("model_quant.onnx", "model.onnx", "~230 MB"),
    ("am.mvn", "am.mvn", "~11 KB"),
    ("tokens.json", "tokens.json", "~344 KB"),
    ("config.yaml", "config.yaml", "~2 KB"),
];

/// FP16 版：Hugging Face ruska1117/SenseVoiceSmall-onnx-fp16（半精度，约 450MB）
const MODEL_BASE_FP16: &str =
    "https://huggingface.co/ruska1117/SenseVoiceSmall-onnx-fp16/resolve/main";

const DEFAULT_HF_MIRROR_BASE: &str = "https://hf-mirror.com";

/// FP16 版需要下载的文件
const MODEL_FILES_FP16: &[(&str, &str, &str)] = &[
    ("model.onnx", "model.onnx", "~4.3 MB"),
    ("model.onnx.data", "model.onnx.data", "~446 MB"),
    ("am.mvn", "am.mvn", "~11 KB"),
    ("tokens.json", "tokens.json", "~344 KB"),
    ("config.yaml", "config.yaml", "~2 KB"),
];

/// 默认模型安装目录（按预设分目录）
pub fn default_model_dir(preset: ModelPreset) -> Result<PathBuf> {
    let subdir = match preset {
        ModelPreset::Quantized => "sensevoice-small",
        ModelPreset::Fp16 => "sensevoice-small-fp16",
    };
    Ok(Config::data_dir()?.join("models").join(subdir))
}

fn model_subdir_name(preset: ModelPreset) -> &'static str {
    match preset {
        ModelPreset::Quantized => "sensevoice-small",
        ModelPreset::Fp16 => "sensevoice-small-fp16",
    }
}

fn current_app_resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let app_dir = exe
        .ancestors()
        .find(|path| path.extension().and_then(|s| s.to_str()) == Some("app"))?;
    Some(app_dir.join("Contents").join("Resources"))
}

pub fn bundled_model_dir(preset: ModelPreset) -> Option<PathBuf> {
    let resources_dir = current_app_resources_dir()?;
    let candidate = resources_dir.join("models").join(model_subdir_name(preset));
    if model_is_ready(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

/// 检查目录内是否存在可用的 ONNX 模型文件
pub fn model_is_ready(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let has_model = dir.join("model.onnx").exists() || dir.join("model_quant.onnx").exists();
    let has_tokens = dir.join("tokens.json").exists();
    has_model && has_tokens
}

fn model_base_candidates(base_url: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Ok(raw) = std::env::var("OPEN_FLOW_MODEL_BASE_URLS") {
        for item in raw.split(',') {
            let trimmed = item.trim().trim_end_matches('/');
            if !trimmed.is_empty() {
                candidates.push(trimmed.to_string());
            }
        }
    }

    if let Ok(base) = std::env::var("OPEN_FLOW_MODEL_BASE_URL") {
        let trimmed = base.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_string());
        }
    }

    if let Ok(mirror) = std::env::var("OPEN_FLOW_HF_MIRROR") {
        let trimmed = mirror.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            candidates.push(rewrite_huggingface_base(base_url, trimmed));
        }
    }

    candidates.push(base_url.trim_end_matches('/').to_string());
    candidates.push(rewrite_huggingface_base(
        base_url,
        DEFAULT_HF_MIRROR_BASE,
    ));

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.contains(&candidate) {
            deduped.push(candidate);
        }
    }
    deduped
}

fn rewrite_huggingface_base(base_url: &str, mirror_root: &str) -> String {
    if let Some(rest) = base_url.strip_prefix("https://huggingface.co/") {
        format!("{}/{}", mirror_root.trim_end_matches('/'), rest)
    } else {
        base_url.to_string()
    }
}

/// 确保模型就绪：已有则直接返回路径，没有则按当前预设自动下载后写入 config。
pub async fn ensure_model_ready(model_override: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = model_override {
        return Ok(path);
    }

    let config = Config::load().unwrap_or_default();
    let preset = config.effective_preset();

    if crate::IS_MAS_BUILD {
        if let Some(bundled_dir) = bundled_model_dir(preset) {
            return Ok(bundled_dir);
        }
    }

    if let Some(ref path) = config.model_path {
        let path_str = path.to_string_lossy();
        if !path_str.contains("Shandianshuo")
            && !path_str.contains("shandianshuo")
            && model_is_ready(path)
        {
            return Ok(path.clone());
        }
    }

    let default_dir = default_model_dir(preset)?;
    if model_is_ready(&default_dir) {
        save_model_to_config(&default_dir)?;
        return Ok(default_dir);
    }

    if crate::IS_MAS_BUILD {
        anyhow::bail!(
            "Mac App Store 构建未找到内置模型，请重新打包并确认模型已写入 app bundle 资源目录。"
        );
    }

    println!(
        "🔍 未找到本地模型（预设: {}），正在自动下载...",
        preset.as_str()
    );
    println!();
    download_all(None, preset, false).await?;
    save_model_to_config(&default_dir)?;

    Ok(default_dir)
}

/// 将模型路径写入 config.toml（供 setup/model use/standalone CLI 共用）
pub fn save_model_to_config(model_path: &Path) -> Result<()> {
    let mut config = Config::load()?;
    config.model_path = Some(model_path.to_path_buf());
    config.save()?;
    Ok(())
}

/// 执行实际下载流程（按预设选择源与文件列表）
pub async fn download_all(
    model_dir: Option<PathBuf>,
    preset: ModelPreset,
    force: bool,
) -> Result<PathBuf> {
    let dest_dir = match model_dir {
        Some(path) => path,
        None => default_model_dir(preset)?,
    };

    let (base_url, files) = match preset {
        ModelPreset::Quantized => (MODEL_BASE_QUANTIZED, MODEL_FILES_QUANTIZED),
        ModelPreset::Fp16 => (MODEL_BASE_FP16, MODEL_FILES_FP16),
    };
    let base_candidates = model_base_candidates(base_url);

    println!("📦 Open Flow 模型下载（预设: {}）", preset.as_str());
    println!("   目标目录: {}", dest_dir.display());
    if base_candidates.len() > 1 {
        println!("   下载源: {}", base_candidates.join("  ->  "));
    } else {
        println!("   下载源: {}", base_candidates[0]);
    }
    println!();

    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("无法创建目录: {}", dest_dir.display()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()?;

    for (remote_name, local_name, size_hint) in files {
        let dest_path = dest_dir.join(local_name);

        if dest_path.exists() && !force {
            println!("✅ 已存在，跳过: {} ({})", local_name, size_hint);
            continue;
        }

        println!("⬇️  正在下载: {} ({})", local_name, size_hint);

        download_file_with_fallback(&client, &base_candidates, remote_name, &dest_path)
            .await
            .with_context(|| {
                format!(
                    "下载失败: {} → {}",
                    remote_name,
                    dest_path.display()
                )
            })?;
        println!("   ✓ 完成: {}", local_name);
    }

    println!();
    println!("🎉 模型下载完成！");
    println!("   路径: {}", dest_dir.display());
    println!();

    Ok(dest_dir)
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    let mut resp = client
        .get(url)
        .send()
        .await
        .context("HTTP 请求失败")?
        .error_for_status()
        .context("服务器返回错误状态码")?;

    let total = resp.content_length();
    let mut downloaded: u64 = 0;

    let tmp_path = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("无法创建临时文件: {}", tmp_path.display()))?;

    let stdout = std::io::stdout();

    while let Some(bytes) = resp.chunk().await.context("流读取失败")? {
        file.write_all(&bytes).await.context("写入失败")?;
        downloaded += bytes.len() as u64;

        if let Some(total) = total {
            let pct = downloaded * 100 / total;
            let mb = downloaded as f64 / 1_048_576.0;
            let total_mb = total as f64 / 1_048_576.0;
            let mut out = stdout.lock();
            let _ = write!(out, "\r   {:.1} MB / {:.1} MB  ({}%)", mb, total_mb, pct);
            let _ = out.flush();
        }
    }

    if total.is_some() {
        println!();
    }

    file.flush().await.context("刷新文件失败")?;
    drop(file);

    tokio::fs::rename(&tmp_path, dest)
        .await
        .with_context(|| format!("重命名失败: {} → {}", tmp_path.display(), dest.display()))?;

    Ok(())
}

async fn download_file_with_fallback(
    client: &reqwest::Client,
    base_candidates: &[String],
    remote_name: &str,
    dest: &Path,
) -> Result<()> {
    let mut errors = Vec::new();

    for (index, base) in base_candidates.iter().enumerate() {
        let url = format!("{}/{}", base.trim_end_matches('/'), remote_name);
        if base_candidates.len() > 1 {
            println!("   尝试下载源 {}/{}: {}", index + 1, base_candidates.len(), url);
        }

        match download_file(client, &url, dest).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                errors.push(format!("{} ({})", url, err));
                let _ = tokio::fs::remove_file(dest.with_extension("tmp")).await;
                eprintln!("   ⚠️  下载源失败: {}", url);
            }
        }
    }

    anyhow::bail!(
        "所有下载源均失败。\n{}",
        errors
            .into_iter()
            .map(|item| format!(" - {}", item))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
