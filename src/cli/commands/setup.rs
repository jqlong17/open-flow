use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::common::config::ModelPreset;

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
    Ok(crate::common::config::Config::data_dir()?
        .join("models")
        .join(subdir))
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

/// 确保模型就绪：已有则直接返回路径，没有则按当前预设自动下载后写入 config。
///
/// 查找顺序：
/// 1. `model_override`（CLI --model 参数）
/// 2. config.toml 中已保存的 model_path（且目录内模型完整）
/// 3. 当前预设对应的默认目录（若存在则同步写入 config）
/// 4. 均不存在 → 按当前预设自动下载到默认目录并写入 config
pub async fn ensure_model_ready(model_override: Option<PathBuf>) -> Result<PathBuf> {
    use crate::common::config::Config;

    // 1. 显式指定
    if let Some(p) = model_override {
        return Ok(p);
    }

    let config = Config::load().unwrap_or_default();
    let preset = config.effective_preset();

    // 2. config 已保存（排除 Shandianshuo 等第三方路径）
    if let Some(ref p) = config.model_path {
        let path_str = p.to_string_lossy();
        if !path_str.contains("Shandianshuo") && !path_str.contains("shandianshuo") {
            if model_is_ready(p) {
                return Ok(p.clone());
            }
        }
    }

    // 3. 当前预设的默认目录（已存在则写入 config 并返回）
    let default_dir = default_model_dir(preset)?;
    if model_is_ready(&default_dir) {
        save_model_to_config(&default_dir)?;
        return Ok(default_dir);
    }

    // 4. 按预设自动下载
    println!(
        "🔍 未找到本地模型（预设: {}），正在自动下载...",
        preset.as_str()
    );
    println!();
    download_all(None, preset, false).await?;
    save_model_to_config(&default_dir)?;

    Ok(default_dir)
}

/// 将模型路径写入 config.toml（供 model use 等调用）
pub fn save_model_to_config(model_path: &Path) -> Result<()> {
    use crate::common::config::Config;
    let mut config = Config::load()?;
    config.model_path = Some(model_path.to_path_buf());
    config.save()?;
    Ok(())
}

/// `open-flow setup` 命令入口（手动触发，保留供高级用途）
/// 若未指定目录，按当前 config 的 model_preset 下载到默认目录。
pub async fn run(model_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let preset = crate::common::config::Config::load()
        .unwrap_or_default()
        .effective_preset();
    download_all(model_dir.clone(), preset, force).await?;

    if model_dir.is_none() {
        let default_dir = default_model_dir(preset)?;
        save_model_to_config(&default_dir)?;
        println!("✅ 模型路径已自动写入配置，可直接运行:");
        println!("   open-flow start");
    }

    Ok(())
}

/// 执行实际下载流程（按预设选择源与文件列表）；供 model use 等调用
pub async fn download_all(
    model_dir: Option<PathBuf>,
    preset: ModelPreset,
    force: bool,
) -> Result<()> {
    let dest_dir = match model_dir {
        Some(p) => p,
        None => default_model_dir(preset)?,
    };

    let (base_url, files) = match preset {
        ModelPreset::Quantized => (MODEL_BASE_QUANTIZED, MODEL_FILES_QUANTIZED),
        ModelPreset::Fp16 => (MODEL_BASE_FP16, MODEL_FILES_FP16),
    };

    println!("📦 Open Flow 模型下载（预设: {}）", preset.as_str());
    println!("   目标目录: {}", dest_dir.display());
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

        let url = format!("{}/{}", base_url, remote_name);
        download_file(&client, &url, &dest_path)
            .await
            .with_context(|| format!("下载失败: {} → {}", url, dest_path.display()))?;
        println!("   ✓ 完成: {}", local_name);
    }

    println!();
    println!("🎉 模型下载完成！");
    println!("   路径: {}", dest_dir.display());
    println!();

    Ok(())
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
