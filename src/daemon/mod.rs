use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::info;

use crate::asr::AsrEngine;
use crate::audio::AudioCapture;
use crate::common::config::Config;
use crate::common::types::{HotkeyEvent, RecordingState};
use crate::hotkey::{
    check_accessibility_permission, request_accessibility_permission, HotkeyListener,
};
use crate::text_injection::TextInjector;
use crate::tray::{TrayHandle, TrayIconState};

/// Daemon 事件类型
#[derive(Debug)]
pub enum DaemonEvent {
    Hotkey(HotkeyEvent),
    TranscriptionComplete(String),
}

pub struct Daemon {
    #[allow(dead_code)]
    config: Config,
    state: Arc<Mutex<RecordingState>>,
    /// 转写/粘贴进行中时忽略新热键，避免竞态
    is_processing: AtomicBool,
    /// 已收到的热键事件次数（用于日志：第 N 次按键）
    hotkey_recv_count: std::sync::atomic::AtomicU64,
    model_path: PathBuf,
    audio_capture: AudioCapture,
    asr_engine: Arc<Mutex<AsrEngine>>,
    text_injector: TextInjector,
    /// 当前录音流（Some = 正在录音，drop 即停止）
    active_stream: Mutex<Option<cpal::Stream>>,
    /// 当前录音 session 的缓冲区（每次录音创建新 Arc，防止旧 stream 的 stale 回调污染新 session）
    recording_buffer: Mutex<Arc<Mutex<Vec<f32>>>>,
    /// 托盘句柄（Send+Sync，状态更新发回主线程）
    tray: Option<Arc<TrayHandle>>,
}

impl Daemon {
    pub fn new(
        config: Config,
        model_path: PathBuf,
        tray: Option<Arc<TrayHandle>>,
    ) -> Result<Self> {
        let audio_capture = AudioCapture::new().context("初始化音频采集器失败")?;
        let asr_engine = Arc::new(Mutex::new(AsrEngine::new(model_path.clone())));
        let text_injector = TextInjector::new();

        Ok(Self {
            config,
            state: Arc::new(Mutex::new(RecordingState::default())),
            is_processing: AtomicBool::new(false),
            hotkey_recv_count: std::sync::atomic::AtomicU64::new(0),
            model_path,
            audio_capture,
            asr_engine,
            text_injector,
            active_stream: Mutex::new(None),
            recording_buffer: Mutex::new(Arc::new(Mutex::new(Vec::new()))),
            tray,
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

        // ── 模型预热（消除首次推理 JIT 开销）──────────────────────────
        {
            let warmup_start = std::time::Instant::now();
            self.asr_engine.lock().unwrap().warmup();
            let warmup_ms = warmup_start.elapsed().as_millis();
            info!("模型预热耗时: {}ms", warmup_ms);
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
        println!(
            "   音频设备: {}Hz / {} 通道",
            audio_info.sample_rate, audio_info.channels
        );
        println!("   模型路径: {:?}", self.model_path);
        println!();
        println!("🎙️  按右侧 Command 键开始录音，再按一次停止并转写");
        println!("   托盘图标可查看状态（灰=待机 红=录音 黄=转写）");
        println!();

        // ── 主事件循环 ────────────────────────────────────────────────
        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    match event {
                        DaemonEvent::Hotkey(ev) => {
                            self.handle_hotkey(ev, &event_tx).await;
                        }
                        DaemonEvent::TranscriptionComplete(text) => {
                            self.set_tray(TrayIconState::Idle);
                            println!("📝 转写完成: {}", text);
                            // 延迟 250ms 再粘贴，避免与用户紧接着的「第三次按键」重叠：
                            // inject 模拟左 Command+V 会干扰 CGEventTap，导致右 Command 的 KeyPress 丢失、只收到 Release
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            info!("[Hotkey] 开始粘贴");
                            if let Err(e) = self.text_injector.inject(&text) {
                                eprintln!("⚠️  文字注入失败: {e}");
                            }
                            info!("[Hotkey] 粘贴结束");
                        }
                    }
                }
                        // daemon 每 200ms 检查托盘退出标志
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                    if self.tray.as_ref().map_or(false, |t| t.exit_requested()) {
                        println!("👋 托盘退出信号已收到，daemon 即将停止...");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn set_tray(&self, state: TrayIconState) {
        if let Some(ref t) = self.tray {
            t.set_state(state);
        }
    }

    async fn handle_hotkey(&self, _event: HotkeyEvent, tx: &mpsc::Sender<DaemonEvent>) {
        let n = self.hotkey_recv_count.fetch_add(1, Ordering::SeqCst) + 1;
        let is_processing = self.is_processing.load(Ordering::SeqCst);
        let is_recording = self.state.lock().unwrap().is_recording;
        info!(
            "[Hotkey] 收到第 {} 次按键 is_recording={} is_processing={}",
            n, is_recording, is_processing
        );
        if is_processing {
            info!("[Hotkey] 第 {} 次 -> 忽略（转写中）", n);
            return;
        }
        if is_recording {
            info!("[Hotkey] 第 {} 次 -> 动作: 停止录音并转写", n);
            self.set_tray(TrayIconState::Transcribing);
            let res = self.stop_and_transcribe(tx).await;
            if let Err(e) = res {
                self.set_tray(TrayIconState::Idle);
                eprintln!("⚠️  转写失败: {e}");
            }
        } else {
            info!("[Hotkey] 第 {} 次 -> 动作: 开始录音", n);
            if let Err(e) = self.start_recording() {
                eprintln!("⚠️  录音启动失败: {e}");
            } else {
                self.set_tray(TrayIconState::Recording);
            }
        }
    }

    fn start_recording(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.is_recording {
            return Ok(());
        }

        // 每次录音创建全新 Arc，旧 stream 的 stale 回调只写入旧 Arc，不影响本次 session
        let session_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let stream = self
            .audio_capture
            .build_live_stream(session_buf.clone())
            .context("创建录音流失败")?;
        *self.recording_buffer.lock().unwrap() = session_buf;
        *self.active_stream.lock().unwrap() = Some(stream);

        state.is_recording = true;
        state.start_time = Some(std::time::Instant::now());

        info!("[Hotkey] 录音已启动");
        println!("🔴 录音中... 再按右侧 Command 键停止");
        Ok(())
    }

    async fn stop_and_transcribe(&self, tx: &mpsc::Sender<DaemonEvent>) -> Result<()> {
        self.is_processing.store(true, Ordering::SeqCst);

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

        // 先拿走本次 session 的 Arc，再 drop stream
        // 这样即使旧 stream 有 stale 回调继续写入，也写入旧 Arc，不会影响下次 session
        let session_buf = self.recording_buffer.lock().unwrap().clone();
        drop(self.active_stream.lock().unwrap().take());

        let buffer: Vec<f32> = session_buf.lock().unwrap().clone();
        info!(
            "[Hotkey] 录音已停止，开始转写 (时长 {:.1}s, {} 样本)",
            duration.as_secs_f32(),
            buffer.len()
        );
        println!(
            "⏹️  录音停止 ({:.1}s / {} 样本)，正在转写...",
            duration.as_secs_f32(),
            buffer.len()
        );

        if buffer.is_empty() {
            self.is_processing.store(false, Ordering::SeqCst);
            eprintln!("⚠️  录音为空，请检查麦克风权限（系统设置 > 隐私 > 麦克风）");
            return Ok(());
        }

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

        let asr_engine = self.asr_engine.clone();
        let audio_path_clone = audio_path.clone();
        let result = match tokio::task::spawn_blocking(move || {
            asr_engine
                .lock()
                .unwrap()
                .transcribe(&audio_path_clone, Some("auto"))
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                self.is_processing.store(false, Ordering::SeqCst);
                return Err(anyhow::anyhow!("spawn_blocking failed: {}", e));
            }
        };
        let result = match result {
            Ok(r) => r,
            Err(e) => {
                self.is_processing.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };

        let _ = std::fs::remove_file(&audio_path);

        // 在发送前清除，避免：Hotkey(P3) 先于 TranscriptionComplete 被处理时被误忽略
        self.is_processing.store(false, Ordering::SeqCst);
        tx.send(DaemonEvent::TranscriptionComplete(result.text))
            .await
            .ok();

        Ok(())
    }
}

pub async fn run_daemon(
    config: Config,
    model_path: PathBuf,
    tray: Option<Arc<TrayHandle>>,
) -> Result<()> {
    let daemon = Daemon::new(config, model_path, tray)?;
    daemon.run().await
}
