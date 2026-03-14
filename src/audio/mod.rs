use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info};

/// 音频采集器
pub struct AudioCapture {
    device: cpal::Device,
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
        info!("正在初始化音频采集器...");

        let host = cpal::default_host();

        // 优先查找 MacBook Pro 麦克风
        let device = host
            .input_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find(|d| {
                    d.name()
                        .map(|name| name.contains("MacBook Pro"))
                        .unwrap_or(false)
                })
            })
            // 如果没找到 MacBook Pro 麦克风，使用默认设备
            .or_else(|| host.default_input_device())
            .context("未找到输入设备，请检查麦克风是否连接并已在系统设置中启用")?;

        // 获取设备名称
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        info!("使用音频设备: {}", device_name);

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
        info!("💾 保存录音到: {:?}", output_path);

        // 确保目录存在
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // WAV 文件规格
        let spec = WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let file = File::create(output_path)
            .with_context(|| format!("无法创建文件: {:?}", output_path))?;
        let writer = BufWriter::new(file);
        let mut wav_writer = WavWriter::new(writer, spec).context("创建 WAV 写入器失败")?;

        // 写入样本
        for &sample in buffer {
            wav_writer.write_sample(sample)?;
        }

        wav_writer.finalize().context("完成 WAV 文件写入失败")?;

        let duration_secs = buffer.len() as f32 / self.sample_rate as f32 / self.channels as f32;
        info!(
            "✓ 录音已保存: {} 样本, {:.2} 秒",
            buffer.len(),
            duration_secs
        );

        Ok(())
    }

    /// 创建并启动一个实时录音流，音频数据写入共享缓冲区。
    /// 调用者负责 drop 返回的 Stream 来停止录音。
    pub fn build_live_stream(
        &self,
        buffer: Arc<Mutex<Vec<f32>>>,
    ) -> Result<cpal::Stream> {
        let err_fn = |err: cpal::StreamError| {
            error!("❌ 音频采集错误: {}", err);
        };

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                let buf = buffer.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        buf.lock().unwrap().extend_from_slice(data);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let buf = buffer.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut b = buf.lock().unwrap();
                        for &s in data {
                            b.push(s as f32 / 32768.0);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let buf = buffer.clone();
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let mut b = buf.lock().unwrap();
                        for &s in data {
                            b.push((s as f32 - 32768.0) / 32768.0);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            _ => anyhow::bail!("不支持的采样格式: {:?}", self.sample_format),
        };

        stream.play().context("启动音频流失败")?;
        Ok(stream)
    }

    /// 获取音频配置信息
    pub fn get_info(&self) -> AudioInfo {
        AudioInfo {
            sample_rate: self.sample_rate,
            channels: self.channels,
            sample_format: format!("{:?}", self.sample_format),
        }
    }
}

/// 音频信息
#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
}
