use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use crate::common::config::Config;

pub async fn set_model(path: PathBuf) -> Result<()> {
    info!("Setting model path to: {:?}", path);
    
    // 验证路径存在
    if !path.exists() {
        anyhow::bail!("路径不存在: {:?}", path);
    }
    
    // 加载现有配置或创建默认配置
    let mut config = Config::load()?;
    config.model_path = Some(path.clone());
    config.save()?;
    
    println!("✅ Model path set to: {:?}", path);
    Ok(())
}

pub async fn set_hotkey(key: String) -> Result<()> {
    info!("Setting hotkey to: {}", key);
    
    // 加载现有配置或创建默认配置
    let mut config = Config::load()?;
    config.hotkey = key.clone();
    config.save()?;
    
    println!("✅ Hotkey set to: {}", key);
    Ok(())
}

pub async fn show() -> Result<()> {
    let config = Config::load()?;
    
    println!("Open Flow Configuration:");
    println!("  Model path: {:?}", config.model_path);
    println!("  Hotkey: {}", config.hotkey);
    println!("  Output mode: {:?}", config.output_mode);
    println!("  Language: {}", config.language);
    println!("  Auto paste: {}", config.auto_paste);
    println!("  Clipboard restore: {}", config.clipboard_restore);
    
    Ok(())
}
