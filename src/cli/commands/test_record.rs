use anyhow::Result;
use std::time::Duration;

use crate::audio::{save_buffer_to_wav_with_spec, AudioCapture};
use open_flow::system_audio::SystemAudioCapture;
use std::sync::{Arc, Mutex};

/// 测试录音功能
pub async fn test_record(duration_secs: u64) -> Result<()> {
    let config = crate::common::config::Config::load().unwrap_or_default();
    let configured_input_source = config.resolved_input_source();

    println!("🎙️  测试录音功能");
    println!();

    // 显示可用设备
    println!("可用的音频输入设备:");
    println!("{}", "=".repeat(50));
    if let Ok(snapshot) = crate::audio::list_input_devices() {
        for (idx, device) in snapshot.devices.iter().enumerate() {
            println!(
                "{} [{}] {}",
                if device.is_default { "*" } else { " " },
                idx,
                device.name
            );
        }
    }
    println!("{}", "=".repeat(50));
    println!("* = 默认设备");
    if let Some(source) = configured_input_source.as_deref() {
        println!("当前配置输入源: {}", source);
    } else {
        println!("当前配置输入源: 系统默认");
    }
    println!();

    // 初始化音频采集器
    let capture_mode = config.resolved_capture_mode();
    let info = if capture_mode == "microphone" {
        let audio_capture = AudioCapture::new_with_device_name(configured_input_source.as_deref())?;
        let info = audio_capture.get_info();
        println!("使用音频设备配置:");
        println!("  设备名: {}", info.device_name);
        println!("  采样率: {}Hz", info.sample_rate);
        println!("  通道数: {}", info.channels);
        println!("  格式: {}", info.sample_format);
        println!();

        let temp_dir = std::env::temp_dir();
        let output_path = temp_dir.join("open-flow-test-recording.wav");

        println!("🔴 准备开始录音 {} 秒...", duration_secs);
        println!("   请准备好说话");
        println!();

        for i in (1..=3).rev() {
            print!("\r   开始录音倒计时: {}...", i);
            std::io::Write::flush(&mut std::io::stdout())?;
            std::thread::sleep(Duration::from_secs(1));
        }
        println!("\r   开始！                    ");

        match audio_capture.record_to_file(Duration::from_secs(duration_secs), &output_path) {
            Ok(_) => {
                println!();
                println!("✅ 测试完成！");
                println!("   录音文件: {:?}", output_path);

                if output_path.exists() {
                    let metadata = std::fs::metadata(&output_path)?;
                    println!(
                        "   文件大小: {} bytes ({:.2} MB)",
                        metadata.len(),
                        metadata.len() as f64 / 1024.0 / 1024.0
                    );
                    println!();
                    println!("📢 播放录音:");
                    println!("   open {:?}", output_path);
                }
            }
            Err(e) => {
                println!();
                println!("❌ 录音失败: {}", e);
                return Err(e);
            }
        }

        return Ok(());
    } else {
        let info = SystemAudioCapture::info_from_config(&config);
        println!("使用系统音频配置:");
        println!("  模式: {}", capture_mode);
        println!("  目标: {}", info.device_name);
        println!("  采样率: {}Hz", info.sample_rate);
        println!("  通道数: {}", info.channels);
        println!("  格式: {}", info.sample_format);
        info
    };
    println!();

    let temp_dir = std::env::temp_dir();
    let output_path = temp_dir.join("open-flow-test-recording.wav");

    println!("🔴 准备开始录音 {} 秒...", duration_secs);
    println!("   请准备好说话");
    println!();

    for i in (1..=3).rev() {
        print!("\r   开始录音倒计时: {}...", i);
        std::io::Write::flush(&mut std::io::stdout())?;
        std::thread::sleep(Duration::from_secs(1));
    }
    println!("\r   开始！                    ");

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let capture = SystemAudioCapture::spawn_from_config(&config, buffer.clone())?;
    std::thread::sleep(Duration::from_secs(duration_secs));
    capture.stop();

    let samples = {
        let mut guard = buffer.lock().unwrap();
        std::mem::take(&mut *guard)
    };

    if samples.is_empty() {
        anyhow::bail!("系统音频缓冲区为空，当前目标可能没有产生声音");
    }

    save_buffer_to_wav_with_spec(&samples, info.sample_rate, info.channels, &output_path)?;

    println!();
    println!("✅ 测试完成！");
    println!("   录音文件: {:?}", output_path);

    if output_path.exists() {
        let metadata = std::fs::metadata(&output_path)?;
        println!(
            "   文件大小: {} bytes ({:.2} MB)",
            metadata.len(),
            metadata.len() as f64 / 1024.0 / 1024.0
        );
        println!();
        println!("📢 播放录音:");
        println!("   open {:?}", output_path);
    }

    Ok(())
}
