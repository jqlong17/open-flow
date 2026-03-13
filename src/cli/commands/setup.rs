use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// 模型下载源：Hugging Face（haixuantao 量化版，与 ModelScope 官方一致）
const MODEL_BASE: &str = "https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main";

/// 需要下载的文件列表：(远端文件名, 本地保存名, 预期大小描述)
const MODEL_FILES: &[(&str, &str, &str)] = &[
    ("model_quant.onnx", "model.onnx", "~230 MB"),
    ("am.mvn", "am.mvn", "~11 KB"),
    ("tokens.json", "tokens.json", "~344 KB"),
    ("config.yaml", "config.yaml", "~2 KB"),
];

/// 默认模型安装目录
pub fn default_model_dir() -> Result<PathBuf> {
    Ok(crate::common::config::Config::data_dir()?
        .join("models")
        .join("sensevoice-small"))
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

/// 确保模型就绪：已有则直接返回路径，没有则自动下载后写入 config。
///
/// 查找顺序：
/// 1. `model_override`（CLI --model 参数）
/// 2. config.toml 中已保存的 model_path
/// 3. 默认安装目录（若存在则同步写入 config）
/// 4. 均不存在 → 自动下载到默认目录并写入 config
pub async fn ensure_model_ready(model_override: Option<PathBuf>) -> Result<PathBuf> {
    use crate::common::config::Config;

    // 1. 显式指定
    if let Some(p) = model_override {
        return Ok(p);
    }

    // 2. config 已保存（排除 Shandianshuo 等第三方路径，仅用 open-flow 自己的目录）
    if let Ok(config) = Config::load() {
        if let Some(ref p) = config.model_path {
            let path_str = p.to_string_lossy();
            if !path_str.contains("Shandianshuo") && !path_str.contains("shandianshuo") {
                if model_is_ready(p) {
                    return Ok(p.clone());
                }
            }
        }
    }

    // 3. 默认下载目录（已存在但还没写入 config）
    let default_dir = default_model_dir()?;
    if model_is_ready(&default_dir) {
        save_model_to_config(&default_dir)?;
        return Ok(default_dir);
    }

    // 4. 自动下载
    println!("🔍 未找到本地模型，正在自动下载（首次运行）...");
    println!();
    download_all(None, false).await?;
    save_model_to_config(&default_dir)?;

    Ok(default_dir)
}

/// 将模型路径写入 config.toml
fn save_model_to_config(model_path: &Path) -> Result<()> {
    use crate::common::config::Config;
    let mut config = Config::load()?;
    config.model_path = Some(model_path.to_path_buf());
    config.save()?;
    Ok(())
}

/// `open-flow setup` 命令入口（手动触发，保留供高级用途）
pub async fn run(model_dir: Option<PathBuf>, force: bool) -> Result<()> {
    download_all(model_dir.clone(), force).await?;

    // 若使用默认目录，自动写入 config
    if model_dir.is_none() {
        let default_dir = default_model_dir()?;
        save_model_to_config(&default_dir)?;
        println!("✅ 模型路径已自动写入配置，可直接运行:");
        println!("   open-flow start");
    }

    Ok(())
}

/// 执行实际下载流程
async fn download_all(model_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let dest_dir = match model_dir {
        Some(p) => p,
        None => default_model_dir()?,
    };

    println!("📦 Open Flow 模型下载");
    println!("   目标目录: {}", dest_dir.display());
    println!();

    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("无法创建目录: {}", dest_dir.display()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()?;

    for (remote_name, local_name, size_hint) in MODEL_FILES {
        let dest_path = dest_dir.join(local_name);

        if dest_path.exists() && !force {
            println!("✅ 已存在，跳过: {} ({})", local_name, size_hint);
            continue;
        }

        println!("⬇️  正在下载: {} ({})", local_name, size_hint);

        let url = format!("{}/{}", MODEL_BASE, remote_name);
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
