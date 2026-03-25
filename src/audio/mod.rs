use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use serde::Serialize;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info, warn};

/// 音频采集器
pub struct AudioCapture {
    device: cpal::Device,
    device_name: String,
    config: StreamConfig,
    sample_format: SampleFormat,
    sample_rate: u32,
    channels: u16,
}

/// 录音数据
pub struct RecordingData {
    pub buffer: Vec<f32>,
    pub is_recording: bool,
}

impl Default for RecordingData {
    fn default() -> Self {
        Self {
            buffer: Vec::with_capacity(44100 * 10),
            is_recording: false,
        }
    }
}

impl AudioCapture {
    /// 创建新的音频采集器
    pub fn new() -> Result<Self> {
        Self::new_with_device_name(None)
    }

    /// 使用指定输入设备名称创建新的音频采集器；空值时回落到系统默认设备。
    pub fn new_with_device_name(device_name: Option<&str>) -> Result<Self> {
        info!("正在初始化音频采集器...");

        let host = cpal::default_host();
        let requested_device_name = device_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let device = select_input_device(&host, requested_device_name.as_deref())?;
        let actual_device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        match requested_device_name.as_deref() {
            Some(requested) if requested == actual_device_name => {
                info!("使用指定输入设备: {}", actual_device_name);
            }
            Some(requested) => {
                warn!(
                    "指定输入设备未命中，回退到可用设备 requested={} actual={}",
                    requested, actual_device_name
                );
                info!("使用回退输入设备: {}", actual_device_name);
            }
            None => {
                info!("使用系统默认输入设备: {}", actual_device_name);
            }
        }
        info!("使用音频设备: {}", actual_device_name);

        // 获取默认配置
        let supported_config = device
            .default_input_config()
            .context("无法获取默认输入配置")?;

        info!("默认配置: {:?}", supported_config);

        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.config().into();
        let sample_rate = config.sample_rate.0;
        let channels = config.channels as u16;

        info!(
            "🎙️  音频采集器已初始化: {:?} @ {}Hz, {} 通道",
            sample_format, sample_rate, channels
        );

        Ok(Self {
            device,
            device_name: actual_device_name,
            config,
            sample_format,
            sample_rate,
            channels,
        })
    }

    /// 录制音频并直接保存（简化版）
    pub fn record_to_file(&self, duration: Duration, output_path: &Path) -> Result<()> {
        info!("🔴 开始录音 {} 秒...", duration.as_secs());

        // 创建共享缓冲区
        let recording_data = Arc::new(Mutex::new(RecordingData {
            buffer: Vec::with_capacity(
                self.sample_rate as usize * duration.as_secs() as usize * self.channels as usize,
            ),
            is_recording: true,
        }));

        let recording_data_clone = recording_data.clone();

        // 错误回调
        let err_fn = move |err| {
            error!("❌ 音频采集错误: {}", err);
        };

        info!("创建音频流...");

        // 根据采样格式创建对应的流
        let stream = match self.sample_format {
            SampleFormat::F32 => self.device.build_input_stream(
                &self.config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut data_lock) = recording_data_clone.lock() {
                        if data_lock.is_recording {
                            data_lock.buffer.extend_from_slice(data);
                        }
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => self.device.build_input_stream(
                &self.config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut data_lock) = recording_data_clone.lock() {
                        if data_lock.is_recording {
                            for &sample in data.iter() {
                                data_lock.buffer.push(sample as f32 / 32768.0);
                            }
                        }
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::U16 => self.device.build_input_stream(
                &self.config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut data_lock) = recording_data_clone.lock() {
                        if data_lock.is_recording {
                            for &sample in data.iter() {
                                data_lock.buffer.push((sample as f32 - 32768.0) / 32768.0);
                            }
                        }
                    }
                },
                err_fn,
                None,
            )?,
            _ => anyhow::bail!("不支持的采样格式: {:?}", self.sample_format),
        };

        info!("启动音频流...");

        // 开始录音
        stream.play().context("启动音频流失败")?;
        info!("✓ 录音已开始，请说话...");

        // 等待录音完成
        std::thread::sleep(duration);

        info!("停止音频流...");

        // 停止录音
        drop(stream);

        // 标记录音结束
        let mut data_lock = recording_data.lock().unwrap();
        data_lock.is_recording = false;

        info!("⏹️  录音停止，收集到 {} 个样本", data_lock.buffer.len());

        // 保存录音
        if data_lock.buffer.is_empty() {
            anyhow::bail!("音频缓冲区为空，可能没有录到声音。\n请检查：\n1. 麦克风权限（系统设置 > 隐私与安全性 > 麦克风）\n2. 麦克风是否正常工作\n3. 选中的音频设备是否正确");
        }

        self.save_buffer_to_wav(&data_lock.buffer, output_path)?;

        Ok(())
    }

    /// 将缓冲区保存为 WAV 文件
    pub fn save_buffer_to_wav(&self, buffer: &[f32], output_path: &Path) -> Result<()> {
        save_buffer_to_wav_with_spec(buffer, self.sample_rate, self.channels, output_path)
    }

    /// 创建并启动一个实时录音流，音频数据写入共享缓冲区。
    /// 调用者负责 drop 返回的 Stream 来停止录音。
    pub fn build_live_stream(&self, buffer: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream> {
        println!(
            "[Audio] build_live_stream begin sample_format={:?} sample_rate={} channels={}",
            self.sample_format, self.sample_rate, self.channels
        );

        let err_fn = |err: cpal::StreamError| {
            eprintln!("[Audio] stream_error error={}", err);
            error!("❌ 音频采集错误: {}", err);
        };

        let channels = self.channels as usize;

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                println!("[Audio] build_input_stream format=f32");
                let buf = buffer.clone();
                let callback_logged = Arc::new(AtomicBool::new(false));
                let callback_logged_clone = callback_logged.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !callback_logged_clone.swap(true, Ordering::SeqCst) {
                            println!("[Audio] first_callback format=f32 frames={}", data.len());
                        }
                        if let Ok(mut b) = buf.lock() {
                            if channels > 1 {
                                for chunk in data.chunks(channels) {
                                    b.push(chunk.iter().sum::<f32>() / channels as f32);
                                }
                            } else {
                                b.extend_from_slice(data);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                println!("[Audio] build_input_stream format=i16");
                let buf = buffer.clone();
                let callback_logged = Arc::new(AtomicBool::new(false));
                let callback_logged_clone = callback_logged.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !callback_logged_clone.swap(true, Ordering::SeqCst) {
                            println!("[Audio] first_callback format=i16 frames={}", data.len());
                        }
                        if let Ok(mut b) = buf.lock() {
                            if channels > 1 {
                                for chunk in data.chunks(channels) {
                                    let mixed =
                                        chunk.iter().map(|&s| s as f32 / 32768.0).sum::<f32>()
                                            / channels as f32;
                                    b.push(mixed);
                                }
                            } else {
                                b.extend(data.iter().map(|&s| s as f32 / 32768.0));
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                println!("[Audio] build_input_stream format=u16");
                let buf = buffer.clone();
                let callback_logged = Arc::new(AtomicBool::new(false));
                let callback_logged_clone = callback_logged.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if !callback_logged_clone.swap(true, Ordering::SeqCst) {
                            println!("[Audio] first_callback format=u16 frames={}", data.len());
                        }
                        if let Ok(mut b) = buf.lock() {
                            if channels > 1 {
                                for chunk in data.chunks(channels) {
                                    let mixed = chunk
                                        .iter()
                                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                        .sum::<f32>()
                                        / channels as f32;
                                    b.push(mixed);
                                }
                            } else {
                                b.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            _ => anyhow::bail!("不支持的采样格式: {:?}", self.sample_format),
        };

        println!("[Audio] build_input_stream success");
        println!("[Audio] stream_play begin");
        stream.play().context("启动音频流失败")?;
        println!("[Audio] stream_play success");
        Ok(stream)
    }

    /// 获取音频配置信息
    pub fn get_info(&self) -> AudioInfo {
        AudioInfo {
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            sample_format: format!("{:?}", self.sample_format),
        }
    }
}

pub fn save_buffer_to_wav_with_spec(
    buffer: &[f32],
    sample_rate: u32,
    channels: u16,
    output_path: &Path,
) -> Result<()> {
    info!("💾 保存录音到: {:?}", output_path);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let file = File::create(output_path)
        .with_context(|| format!("无法创建文件: {:?}", output_path))?;
    let writer = BufWriter::new(file);
    let mut wav_writer = WavWriter::new(writer, spec).context("创建 WAV 写入器失败")?;

    for &sample in buffer {
        wav_writer.write_sample(sample)?;
    }

    wav_writer.finalize().context("完成 WAV 文件写入失败")?;

    let duration_secs = buffer.len() as f32 / sample_rate as f32 / channels as f32;
    info!(
        "✓ 录音已保存: {} 样本, {:.2} 秒",
        buffer.len(),
        duration_secs
    );

    Ok(())
}

pub fn list_input_devices() -> Result<AudioDeviceSnapshot> {
    let host = cpal::default_host();
    let default_device_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let mut devices = Vec::new();

    let input_devices = host
        .input_devices()
        .context("无法枚举输入设备，请检查麦克风权限和音频设备状态")?;

    for device in input_devices {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = default_device_name
            .as_ref()
            .map(|default_name| default_name == &name)
            .unwrap_or(false);
        devices.push(AudioDeviceEntry { name, is_default });
    }

    Ok(AudioDeviceSnapshot {
        default_device_name,
        devices,
    })
}

fn select_input_device(host: &cpal::Host, preferred_name: Option<&str>) -> Result<cpal::Device> {
    if let Some(target_name) = preferred_name {
        let devices = host
            .input_devices()
            .context("无法枚举输入设备，请检查麦克风权限和音频设备状态")?;
        for device in devices {
            if device.name().ok().as_deref() == Some(target_name) {
                return Ok(device);
            }
        }
    }

    if let Some(device) = host.default_input_device() {
        return Ok(device);
    }

    host.input_devices()
        .context("无法枚举输入设备，请检查麦克风权限和音频设备状态")?
        .next()
        .context("未找到输入设备，请检查麦克风是否连接并已在系统设置中启用")
}

/// 音频信息
#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceEntry {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceSnapshot {
    pub default_device_name: Option<String>,
    pub devices: Vec<AudioDeviceEntry>,
}
