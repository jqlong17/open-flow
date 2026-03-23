//! 模型预设切换：quantized（默认）| fp16

use crate::common::config::{Config, ModelPreset};
use anyhow::Result;
use open_flow::model_store;

/// 切换到指定预设并可选触发下载
pub async fn use_preset(preset: ModelPreset, download: bool) -> Result<()> {
    let mut config = Config::load()?;
    config.set_model_preset(preset)?;
    println!("✅ 当前模型预设: {}", preset.as_str());

    let default_dir = model_store::default_model_dir(preset)?;
    if model_store::model_is_ready(&default_dir) {
        println!("   路径: {}（已就绪）", default_dir.display());
        model_store::save_model_to_config(&default_dir)?;
        if !download {
            return Ok(());
        }
        println!("   正在按 --download 重新检查/下载...");
    } else {
        println!("   路径: {}（未就绪，将自动下载）", default_dir.display());
    }

    model_store::download_all(None, preset, false).await?;
    model_store::save_model_to_config(&default_dir)?;
    println!("✅ 模型已就绪，可运行: open-flow start");
    Ok(())
}

/// 列出当前预设与可用预设
pub fn list() -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let current = config.effective_preset();
    let path = config
        .model_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(默认目录)".to_string());

    println!("当前模型预设: {}", current.as_str());
    println!("   model_path: {}", path);
    println!();
    println!("可用预设:");
    println!("  quantized   量化版（~200MB），默认");
    println!("  fp16        高精度 FP16（~450MB），手动切换: open-flow model use fp16");
    Ok(())
}
