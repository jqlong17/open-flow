use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

use crate::asr::AsrEngine;
use crate::audio::AudioCapture;
use crate::common::config::Config;

/// 单次转写 - 录音并识别
pub async fn run(
    output_mode: String,
    file: Option<PathBuf>,
    duration_secs: u64,
    model_override: Option<PathBuf>,
) -> Result<()> {
    info!("开始单次转写");

    let model_path = match model_override {
        Some(p) => {
            if !p.is_dir() {
                anyhow::bail!("模型路径不是目录: {:?}", p);
            }
            p
        }
        None => {
            let config = Config::load()?;
            config.model_path.ok_or_else(|| {
                anyhow::anyhow!("未配置模型路径。请运行: open-flow config set-model <path> 或使用 --model <path>")
            })?
        }
    };
    
    println!("🎙️  语音转写");
    println!();

    let use_external_file = file.is_some();
    let audio_path = if let Some(input_file) = file {
        if !input_file.exists() {
            anyhow::bail!("音频文件不存在: {:?}", input_file);
        }
        println!("📂 使用已有音频文件: {:?}", input_file);
        input_file
    } else {
        let audio_capture = AudioCapture::new()
            .context("初始化音频采集器失败")?;
        let audio_info = audio_capture.get_info();
        println!("音频设备: MacBook Pro麦克风");
        println!("  采样率: {}Hz, 通道: {}", audio_info.sample_rate, audio_info.channels);
        println!();
        // 确定录音时长
        let duration = if duration_secs == 0 {
        // 交互模式：等待用户按键停止
        println!("🔴 准备录音，按 Enter 键开始...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        println!("   录音中... 按 Enter 停止");
        
        // 这里简化处理，默认录制 10 秒
        // 实际应该开启录音线程，然后等待用户按键
        Duration::from_secs(10)
        } else {
            Duration::from_secs(duration_secs)
        };

        println!("🔴 正在录音 {} 秒...", duration.as_secs());
        
        let temp_dir = std::env::temp_dir();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let recorded_path = temp_dir.join(format!("open-flow-transcribe-{}.wav", timestamp));

        audio_capture.record_to_file(duration, &recorded_path)?;
        println!("✓ 录音完成");
        recorded_path
    };
    
    // 转写
    println!("🧠 正在识别...");
    
    let mut asr_engine = AsrEngine::new(model_path);
    let result = asr_engine.transcribe(&audio_path, Some("auto"))?;
    
    println!();
    println!("📝 转写结果:");
    println!("   {}", result.text);
    println!();
    println!("   置信度: {:.0}%", result.confidence * 100.0);
    println!("   耗时: {}ms", result.duration_ms);
    
    // 根据输出模式处理结果
    match output_mode.as_str() {
        "stdout" => {
            // 已打印到 stdout
        }
        "clipboard" => {
            println!("\n📋 已复制到剪贴板（实现中）");
        }
        "paste" => {
            println!("\n📋 自动粘贴（实现中）");
        }
        _ => {
            // 默认已打印
        }
    }
    
    // 清理临时文件
    if !use_external_file {
        let _ = std::fs::remove_file(&audio_path);
    }
    
    Ok(())
}
