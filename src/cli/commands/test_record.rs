use anyhow::Result;
use std::time::Duration;
use tracing::info;

use crate::audio::AudioCapture;

/// 测试录音功能
pub async fn test_record(duration_secs: u64) -> Result<()> {
    println!("🎙️  测试录音功能");
    println!();

    // 显示可用设备
    AudioCapture::list_input_devices()?;
    println!();

    // 初始化音频采集器
    let audio_capture = AudioCapture::new()?;
    let info = audio_capture.get_info();
    println!("使用音频设备配置:");
    println!("  采样率: {}Hz", info.sample_rate);
    println!("  通道数: {}", info.channels);
    println!("  格式: {}", info.sample_format);
    println!();

    // 准备输出路径
    let temp_dir = std::env::temp_dir();
    let output_path = temp_dir.join("open-flow-test-recording.wav");
    
    println!("🔴 准备开始录音 {} 秒...", duration_secs);
    println!("   请准备好说话");
    println!();
    
    // 倒计时 3 秒
    for i in (1..=3).rev() {
        print!("\r   开始录音倒计时: {}...", i);
        std::io::Write::flush(&mut std::io::stdout())?;
        std::thread::sleep(Duration::from_secs(1));
    }
    println!("\r   开始！                    ");
    
    // 直接录制到文件
    match audio_capture.record_to_file(
        Duration::from_secs(duration_secs),
        &output_path,
    ) {
        Ok(_) => {
            println!();
            println!("✅ 测试完成！");
            println!("   录音文件: {:?}", output_path);
            
            // 检查文件
            if output_path.exists() {
                let metadata = std::fs::metadata(&output_path)?;
                println!("   文件大小: {} bytes ({:.2} MB)", 
                    metadata.len(),
                    metadata.len() as f64 / 1024.0 / 1024.0
                );
                
                // 显示播放命令
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
    
    Ok(())
}
