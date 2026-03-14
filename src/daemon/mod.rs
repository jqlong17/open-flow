use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::info;

use crate::asr::AsrProvider;
use crate::audio::AudioCapture;
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
    /// 热键监听线程已退出（崩溃或 channel 断开），daemon 应停止
    HotkeyListenerDead,
}

pub struct Daemon {
    state: Arc<Mutex<RecordingState>>,
    /// 转写/粘贴进行中时忽略新热键，避免竞态
    is_processing: AtomicBool,
    /// 已收到的热键事件次数（用于日志：第 N 次按键）
    hotkey_recv_count: std::sync::atomic::AtomicU64,
    audio_capture: AudioCapture,
    provider: Arc<dyn AsrProvider>,
    text_injector: TextInjector,
    /// 当前录音流（Some = 正在录音，drop 即停止）
    active_stream: Mutex<Option<cpal::Stream>>,
    /// 当前录音 session 的缓冲区（每次录音创建新 Arc，防止旧 stream 的 stale 回调污染新 session）
    recording_buffer: Mutex<Arc<Mutex<Vec<f32>>>>,
    /// 托盘句柄（Send+Sync，状态更新发回主线程）
    tray: Option<Arc<TrayHandle>>,
    /// 触发模式: "toggle" or "hold"
    trigger_mode: String,
}

impl Daemon {
    pub fn new(
        provider: Arc<dyn AsrProvider>,
        tray: Option<Arc<TrayHandle>>,
    ) -> Result<Self> {
        let audio_capture = AudioCapture::new().context("初始化音频采集器失败")?;
        let text_injector = TextInjector::new();
        let config = crate::common::config::Config::load().unwrap_or_default();

        Ok(Self {
            state: Arc::new(Mutex::new(RecordingState::default())),
            is_processing: AtomicBool::new(false),
            hotkey_recv_count: std::sync::atomic::AtomicU64::new(0),
            audio_capture,
            provider,
            text_injector,
            active_stream: Mutex::new(None),
            recording_buffer: Mutex::new(Arc::new(Mutex::new(Vec::new()))),
            tray,
            trigger_mode: config.trigger_mode,
        })
    }

    pub async fn run(self) -> Result<()> {
        let current_exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("<unavailable: {e}>"));
        let accessibility_ok = check_accessibility_permission();
        let input_monitoring_ok = crate::hotkey::check_input_monitoring_permission();

        info!(
            "Permission diagnostics: current_exe={} accessibility_ok={} input_monitoring_ok={}",
            current_exe, accessibility_ok, input_monitoring_ok
        );
        let microphone_ok = crate::hotkey::check_microphone_permission();
        println!("🔎 权限诊断");
        println!("   可执行文件: {}", current_exe);
        println!("   Accessibility: {}", accessibility_ok);
        println!("   Input Monitoring: {}", input_monitoring_ok);
        println!("   Microphone: {}", microphone_ok);

        // 请求缺失的权限（触发系统对话框）
        if !accessibility_ok {
            println!();
            println!("⚠️  Accessibility 权限未授权——正在请求...");
            request_accessibility_permission();
        }
        if !input_monitoring_ok {
            println!();
            println!("⚠️  Input Monitoring 权限未授权——正在请求...");
            crate::hotkey::request_input_monitoring_permission();
        }
        if !microphone_ok {
            println!();
            println!("⚠️  麦克风权限尚未授权。");
            crate::hotkey::request_microphone_permission();
        }

        if !accessibility_ok || !input_monitoring_ok {
            println!();
            println!("⏳ 等待权限授权... 请在系统设置中授权，然后应用将重试。");
            println!("   （如果授权后热键不工作，请重启应用）");
        }

        // ── Provider 状态检查 ──────────────────────────────────────────
        if let Err(e) = self.provider.check_status() {
            anyhow::bail!("Provider 未就绪: {}", e);
        }

        // ── Provider 预热（消除首次推理 JIT 开销）──────────────────────
        {
            let warmup_start = std::time::Instant::now();
            self.provider.warmup().await?;
            let warmup_ms = warmup_start.elapsed().as_millis();
            info!("Provider 预热耗时: {}ms", warmup_ms);
        }

        // ── 音频设备信息 ──────────────────────────────────────────────
        let audio_info = self.audio_capture.get_info();

        // ── 启动热键监听器 ─────────────────────────────────────────────
        let config = crate::common::config::Config::load().unwrap_or_default();
        let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
        let listener = HotkeyListener::new(hotkey_tx, config.hotkey.clone());
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
                Err(_) => {
                    // hotkey 监听线程已退出（channel sender 被 drop），通知主循环
                    let _ = event_tx_clone.blocking_send(DaemonEvent::HotkeyListenerDead);
                    break;
                }
            }
        });

        // ── 就绪提示 ──────────────────────────────────────────────────
        println!();
        println!("✅ Open Flow 已就绪");
        println!(
            "   音频设备: {}Hz / {} 通道",
            audio_info.sample_rate, audio_info.channels
        );
        println!("   Provider: {}", self.provider.name());
        println!();
        println!("🎙️  按热键开始录音，再按一次停止并转写");
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
                            self.is_processing.store(false, Ordering::SeqCst);
                            self.set_tray(TrayIconState::Idle);
                            println!("📝 转写完成: {}", text);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            info!("[Hotkey] 开始粘贴");
                            if let Err(e) = self.text_injector.inject(&text).await {
                                eprintln!("⚠️  文字注入失败: {e}");
                            }
                            info!("[Hotkey] 粘贴结束");
                        }
                        DaemonEvent::HotkeyListenerDead => {
                            eprintln!("❌ 热键监听线程已退出，daemon 停止。请运行 open-flow start 重启。");
                            break;
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

    async fn handle_hotkey(&self, event: HotkeyEvent, tx: &mpsc::Sender<DaemonEvent>) {
        let n = self.hotkey_recv_count.fetch_add(1, Ordering::SeqCst) + 1;
        let is_processing = self.is_processing.load(Ordering::SeqCst);
        let is_recording = self.state.lock().unwrap().is_recording;
        info!(
            "[Hotkey] 收到第 {} 次按键 event={:?} is_recording={} is_processing={} mode={}",
            n, event, is_recording, is_processing, self.trigger_mode
        );

        match event {
            HotkeyEvent::Pressed => {
                if is_processing {
                    info!("[Hotkey] 第 {} 次 -> 忽略（转写中）", n);
                    return;
                }
                if self.trigger_mode == "hold" {
                    // Hold 模式: Pressed 始终开始录音
                    if !is_recording {
                        info!("[Hotkey] 第 {} 次 -> 动作: 开始录音（hold 模式）", n);
                        if let Err(e) = self.start_recording() {
                            eprintln!("⚠️  录音启动失败: {e}");
                        } else {
                            self.set_tray(TrayIconState::Recording);
                        }
                    }
                } else {
                    // Toggle 模式: Pressed 切换状态
                    if is_recording {
                        info!("[Hotkey] 第 {} 次 -> 动作: 停止录音并转写（toggle 模式）", n);
                        self.set_tray(TrayIconState::Transcribing);
                        if let Err(e) = self.stop_and_transcribe(tx).await {
                            self.set_tray(TrayIconState::Idle);
                            eprintln!("⚠️  转写失败: {e}");
                        }
                    } else {
                        info!("[Hotkey] 第 {} 次 -> 动作: 开始录音（toggle 模式）", n);
                        if let Err(e) = self.start_recording() {
                            eprintln!("⚠️  录音启动失败: {e}");
                        } else {
                            self.set_tray(TrayIconState::Recording);
                        }
                    }
                }
            }
            HotkeyEvent::Released => {
                if self.trigger_mode == "hold" && is_recording {
                    // Hold 模式: Released 始终停止
                    info!("[Hotkey] 第 {} 次 -> 动作: 停止录音并转写（hold 松开）", n);
                    self.set_tray(TrayIconState::Transcribing);
                    if let Err(e) = self.stop_and_transcribe(tx).await {
                        self.set_tray(TrayIconState::Idle);
                        eprintln!("⚠️  转写失败: {e}");
                    }
                }
                // Toggle 模式: Released 忽略
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
        println!("🔴 录音中... 再按热键停止");
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
            eprintln!("⚠️  录音为空，请检查麦克风权限（系统设置 > 隐私与安全性 > 麦克风）");
            return Ok(());
        }

        // 振幅诊断：如果音量过低说明麦克风权限缺失或静音
        let max_amp = buffer.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let rms = (buffer.iter().map(|x| x * x).sum::<f32>() / buffer.len() as f32).sqrt();
        info!("[Audio] max_amp={:.6} rms={:.6}", max_amp, rms);
        if max_amp < 0.001 {
            self.is_processing.store(false, Ordering::SeqCst);
            eprintln!("⚠️  录音振幅极低（max={:.6}），麦克风可能未授权。", max_amp);
            eprintln!("   请前往：系统设置 > 隐私与安全性 > 麦克风，将 Open Flow.app 添加到列表并启用。");
            eprintln!("   然后完全退出并重新打开 Open Flow。");
            return Ok(());
        }

        // 通过 AsrProvider trait 进行转写（本地或云端）
        let sample_rate = self.audio_capture.get_info().sample_rate;
        let provider = self.provider.clone();

        let result = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            provider.transcribe(&buffer, sample_rate),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                self.is_processing.store(false, Ordering::SeqCst);
                return Err(e);
            }
            Err(_elapsed) => {
                self.is_processing.store(false, Ordering::SeqCst);
                eprintln!("⚠️  转写超时（>30s），已放弃，请检查模型或重启 daemon");
                return Ok(());
            }
        };

        tx.send(DaemonEvent::TranscriptionComplete(result.text))
            .await
            .ok();

        Ok(())
    }
}

pub async fn run_daemon(
    provider: Arc<dyn AsrProvider>,
    tray: Option<Arc<TrayHandle>>,
) -> Result<()> {
    let daemon = Daemon::new(provider, tray)?;
    daemon.run().await
}
