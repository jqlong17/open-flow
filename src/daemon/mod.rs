use anyhow::{Context, Result};
use cpal::traits::StreamTrait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::asr::AsrEngine;
use crate::audio::AudioCapture;
use crate::common::config::Config;
use crate::common::types::{HotkeyEvent, RecordingState};
use crate::hotkey::{
    check_accessibility_permission, request_accessibility_permission, HotkeyListener,
};
use crate::text_injection::TextInjector;

/// Daemon 事件类型
#[derive(Debug)]
pub enum DaemonEvent {
    Hotkey(HotkeyEvent),
    TranscriptionComplete(String),
}

pub struct Daemon {
    config: Config,
    state: Arc<Mutex<RecordingState>>,
    model_path: PathBuf,
    audio_capture: AudioCapture,
    asr_engine: Mutex<AsrEngine>,
    text_injector: TextInjector,
    /// 当前录音流（Some = 正在录音，drop 即停止）
    active_stream: Mutex<Option<cpal::Stream>>,
    /// 当前录音缓冲区（与 stream callback 共享）
    recording_buffer: Arc<Mutex<Vec<f32>>>,
}

impl Daemon {
    pub fn new(config: Config, model_path: PathBuf) -> Result<Self> {
        let audio_capture =
            AudioCapture::new().context("初始化音频采集器失败")?;
        let asr_engine = Mutex::new(AsrEngine::new(model_path.clone()));
        let text_injector = TextInjector::new();

        Ok(Self {
            config,
            state: Arc::new(Mutex::new(RecordingState::default())),
            model_path,
            audio_capture,
            asr_engine,
            text_injector,
            active_stream: Mutex::new(None),
            recording_buffer: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub async fn run(self) -> Result<()> {
        // ── Accessibility 权限检查 ───────────────────────────────────
        if !check_accessibility_permission() {
            request_accessibility_permission();
            println!();
            println!("授权后请重新运行 open-flow start。");
            anyhow::bail!("缺少 Accessibility 权限");
        }

        // ── ASR 状态 ─────────────────────────────────────────────────
        let asr_status = self.asr_engine.lock().unwrap().check_status();
        if !asr_status.ready {
            anyhow::bail!(
                "模型未就绪：{:?}\n  onnx={} mvn={}",
                asr_status.model_path,
                asr_status.onnx_exists,
                asr_status.model_exists,
            );
        }

        // ── 音频设备信息 ──────────────────────────────────────────────
        let audio_info = self.audio_capture.get_info();

        // ── 启动热键监听器 ─────────────────────────────────────────────
        let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
        let listener = HotkeyListener::new(hotkey_tx);
        listener.start().context("启动热键监听器失败")?;

        // ── 把同步 mpsc 桥接到 tokio mpsc ─────────────────────────────
        let (event_tx, mut event_rx) = mpsc::channel::<DaemonEvent>(32);
        let event_tx_clone = event_tx.clone();
        tokio::task::spawn_blocking(move || loop {
            match hotkey_rx.recv() {
                Ok(ev) => {
                    if event_tx_clone
                        .blocking_send(DaemonEvent::Hotkey(ev))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        });

        // ── 就绪提示 ──────────────────────────────────────────────────
        println!();
        println!("✅ Open Flow 已就绪");
        println!("   音频设备: {}Hz / {} 通道", audio_info.sample_rate, audio_info.channels);
        println!("   模型路径: {:?}", self.model_path);
        println!();
        println!("🎙️  按右侧 Command 键开始录音，再按一次停止并转写");
        println!("   Ctrl+C 退出");
        println!();

        // ── 主事件循环 ────────────────────────────────────────────────
        while let Some(event) = event_rx.recv().await {
            match event {
                DaemonEvent::Hotkey(ev) => {
                    self.handle_hotkey(ev, &event_tx).await;
                }
                DaemonEvent::TranscriptionComplete(text) => {
                    println!("📝 转写完成: {}", text);
                    if let Err(e) = self.text_injector.inject(&text) {
                        eprintln!("⚠️  文字注入失败: {e}");
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_hotkey(
        &self,
        event: HotkeyEvent,
        tx: &mpsc::Sender<DaemonEvent>,
    ) {
        if let HotkeyEvent::Pressed = event {
            let is_recording = self.state.lock().unwrap().is_recording;
            if is_recording {
                if let Err(e) = self.stop_and_transcribe(tx).await {
                    eprintln!("⚠️  转写失败: {e}");
                }
            } else if let Err(e) = self.start_recording() {
                eprintln!("⚠️  录音启动失败: {e}");
            }
        }
    }

    fn start_recording(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.is_recording {
            return Ok(());
        }

        // 清空上次的缓冲区
        self.recording_buffer.lock().unwrap().clear();

        // 创建并启动 cpal 录音流
        let stream = self
            .audio_capture
            .build_live_stream(self.recording_buffer.clone())
            .context("创建录音流失败")?;
        *self.active_stream.lock().unwrap() = Some(stream);

        state.is_recording = true;
        state.start_time = Some(std::time::Instant::now());

        println!("🔴 录音中... 再按右侧 Command 键停止");
        Ok(())
    }

    async fn stop_and_transcribe(&self, tx: &mpsc::Sender<DaemonEvent>) -> Result<()> {
        let duration = {
            let mut state = self.state.lock().unwrap();
            if !state.is_recording {
                return Ok(());
            }
            state.is_recording = false;
            state
                .start_time
                .map(|t| t.elapsed())
                .unwrap_or_default()
        };

        // drop stream → cpal 停止向 buffer 写数据
        drop(self.active_stream.lock().unwrap().take());

        let buffer: Vec<f32> = self.recording_buffer.lock().unwrap().clone();
        println!(
            "⏹️  录音停止 ({:.1}s / {} 样本)，正在转写...",
            duration.as_secs_f32(),
            buffer.len()
        );

        if buffer.is_empty() {
            eprintln!("⚠️  录音为空，请检查麦克风权限（系统设置 > 隐私 > 麦克风）");
            return Ok(());
        }

        // 保存临时 WAV
        let audio_path = std::env::temp_dir().join(format!(
            "open-flow-{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        self.audio_capture
            .save_buffer_to_wav(&buffer, &audio_path)
            .context("保存录音失败")?;

        // 转写（在 blocking 线程里跑，避免阻塞 tokio executor）
        let asr_engine = &self.asr_engine;
        let result = tokio::task::block_in_place(|| {
            asr_engine.lock().unwrap().transcribe(&audio_path, Some("auto"))
        })?;

        // 清理临时文件
        let _ = std::fs::remove_file(&audio_path);

        tx.send(DaemonEvent::TranscriptionComplete(result.text))
            .await
            .ok();

        Ok(())
    }
}

pub async fn run_daemon(config: Config, model_path: PathBuf) -> Result<()> {
    let daemon = Daemon::new(config, model_path)?;
    daemon.run().await
}
