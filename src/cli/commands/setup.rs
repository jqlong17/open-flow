use anyhow::Result;
use open_flow::model_store;
use std::path::PathBuf;

/// `open-flow setup` 命令入口（手动触发，保留供高级用途）
/// 若未指定目录，按当前 config 的 model_preset 下载到默认目录。
pub async fn run(model_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let preset = crate::common::config::Config::load()
        .unwrap_or_default()
        .effective_preset();
    model_store::download_all(model_dir.clone(), preset, force).await?;

    if model_dir.is_none() {
        let default_dir = model_store::default_model_dir(preset)?;
        model_store::save_model_to_config(&default_dir)?;
        println!("✅ 模型路径已自动写入配置，可直接运行:");
        println!("   open-flow start");
    }

    Ok(())
}
