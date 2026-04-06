use anyhow::{Context, Result};
use open_flow::model_store;
#[cfg(not(feature = "mas"))]
use open_flow::system_audio::SystemAudioCapture;
use std::path::PathBuf;
#[cfg(not(feature = "mas"))]
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

use crate::asr::AsrEngine;
#[cfg(not(feature = "mas"))]
use crate::audio::save_buffer_to_wav_with_spec;
use crate::audio::AudioCapture;

/// 单次转写 - 录音并识别
pub async fn run(
    file: Option<PathBuf>,
    duration_secs: u64,
    model_override: Option<PathBuf>,
) -> Result<()> {
    info!("开始单次转写");

    let model_path = model_store::ensure_model_ready(model_override).await?;
    let config = crate::common::config::Config::load().unwrap_or_default();

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

        #[cfg(feature = "mas")]
        let capture_mode = "microphone".to_string();
        #[cfg(not(feature = "mas"))]
        let capture_mode = config.resolved_capture_mode();

        #[cfg(feature = "mas")]
        if config.resolved_capture_mode() != "microphone" {
            println!("ℹ️  Mac App Store 构建仅支持麦克风录音，已忽略当前 capture_mode 配置。");
            println!();
        }

        if capture_mode == "microphone" {
            let audio_capture =
                AudioCapture::new_with_device_name(config.resolved_input_source().as_deref())
                    .context("初始化音频采集器失败")?;
            let audio_info = audio_capture.get_info();
            println!("音频设备: {}", audio_info.device_name);
            println!(
                "  采样率: {}Hz, 通道: {}",
                audio_info.sample_rate, audio_info.channels
            );
            println!();

            audio_capture.record_to_file(duration, &recorded_path)?;
        } else {
            #[cfg(feature = "mas")]
            {
                anyhow::bail!("Mac App Store 构建不支持系统音频转写");
            }

            #[cfg(not(feature = "mas"))]
            {
            let audio_info = SystemAudioCapture::info_from_config(&config);
            println!("音频设备: {}", audio_info.device_name);
            println!(
                "  采样率: {}Hz, 通道: {}",
                audio_info.sample_rate, audio_info.channels
            );
            println!();

            let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
            let capture = SystemAudioCapture::spawn_from_config(&config, buffer.clone())
                .context("启动系统音频采集失败")?;
            std::thread::sleep(duration);
            capture.stop();

            let samples = {
                let mut guard = buffer.lock().unwrap();
                std::mem::take(&mut *guard)
            };

            if samples.is_empty() {
                anyhow::bail!("系统音频缓冲区为空，当前目标可能没有产生声音");
            }

            save_buffer_to_wav_with_spec(
                &samples,
                audio_info.sample_rate,
                audio_info.channels,
                &recorded_path,
            )?;
            }
        }

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

    if !use_external_file {
        let _ = std::fs::remove_file(&audio_path);
    }

    Ok(())
}
