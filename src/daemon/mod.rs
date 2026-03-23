use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::info;

use crate::asr::AsrProvider;
use crate::audio::AudioCapture;
use crate::common::memory;
use crate::common::types::{HotkeyEvent, RecordingState, TranscriptionResult};
use crate::draft_panel::DraftPanelEvent;
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
    HotkeyListenerDead,
}

pub struct Daemon {
    state: Arc<Mutex<RecordingState>>,
    is_processing: AtomicBool,
    hotkey_recv_count: std::sync::atomic::AtomicU64,
    recording_session_count: std::sync::atomic::AtomicU64,
    transcription_count: std::sync::atomic::AtomicU64,
    recording_warning_issued: AtomicBool,
    audio_capture: AudioCapture,
    provider: Arc<dyn AsrProvider>,
    text_injector: TextInjector,
    active_stream: Mutex<Option<cpal::Stream>>,
    recording_buffer: Mutex<Arc<Mutex<Vec<f32>>>>,
    tray: Option<Arc<TrayHandle>>,
    trigger_mode: String,
    draft_mode_active: Arc<AtomicBool>,
    draft_event_tx: Option<std::sync::mpsc::SyncSender<DraftPanelEvent>>,
    draft_live_cursor: Arc<AtomicUsize>,
    draft_live_inflight: Arc<AtomicBool>,
    draft_live_last_tick: Mutex<std::time::Instant>,
    draft_live_text: Arc<Mutex<String>>,
}

const MAX_RECORDING_DURATION_SECS: u64 = 2 * 60 * 60;
const RECORDING_WARNING_DURATION_SECS: u64 = MAX_RECORDING_DURATION_SECS - 5 * 60;
const TRANSCRIBE_SEGMENT_SECS: u64 = 60;
const TRANSCRIBE_TIMEOUT_SECS: u64 = 120;

impl Daemon {
    pub fn new(
        provider: Arc<dyn AsrProvider>,
        tray: Option<Arc<TrayHandle>>,
        draft_mode_active: Arc<AtomicBool>,
        draft_event_tx: Option<std::sync::mpsc::SyncSender<DraftPanelEvent>>,
    ) -> Result<Self> {
        let audio_capture = AudioCapture::new().context("初始化音频采集器失败")?;
        let text_injector = TextInjector::new();
        let config = crate::common::config::Config::load().unwrap_or_default();

        Ok(Self {
            state: Arc::new(Mutex::new(RecordingState::default())),
            is_processing: AtomicBool::new(false),
            hotkey_recv_count: std::sync::atomic::AtomicU64::new(0),
            recording_session_count: std::sync::atomic::AtomicU64::new(0),
            transcription_count: std::sync::atomic::AtomicU64::new(0),
            recording_warning_issued: AtomicBool::new(false),
            audio_capture,
            provider,
            text_injector,
            active_stream: Mutex::new(None),
            recording_buffer: Mutex::new(Arc::new(Mutex::new(Vec::new()))),
            tray,
            trigger_mode: config.trigger_mode,
            draft_mode_active,
            draft_event_tx,
            draft_live_cursor: Arc::new(AtomicUsize::new(0)),
            draft_live_inflight: Arc::new(AtomicBool::new(false)),
            draft_live_last_tick: Mutex::new(std::time::Instant::now()),
            draft_live_text: Arc::new(Mutex::new(String::new())),
        })
    }

    pub async fn run(self) -> Result<()> {
        self.log_memory_checkpoint("daemon_start", None);

        let mut tray_poll = tokio::time::interval(std::time::Duration::from_millis(200));
        let mut memory_heartbeat = tokio::time::interval(std::time::Duration::from_secs(60));
        tray_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        memory_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tray_poll.tick().await;
        memory_heartbeat.tick().await;

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
        println!("   输入设备: {}", audio_info.device_name);
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
                            if self.draft_mode_active.load(Ordering::SeqCst) {
                                if let Some(ref tx) = self.draft_event_tx {
                                    let _ = tx.try_send(DraftPanelEvent::SetText(text));
                                }
                            } else {
                                info!("[Hotkey] 开始粘贴");
                                if let Err(e) = self.text_injector.inject(&text).await {
                                    eprintln!("⚠️  文字注入失败: {e}");
                                }
                                info!("[Hotkey] 粘贴结束");
                            }
                        }
                        DaemonEvent::HotkeyListenerDead => {
                            eprintln!("❌ 热键监听线程已退出，daemon 停止。请运行 open-flow start 重启。");
                            break;
                        }
                    }
                }
                        // daemon 每 200ms 检查托盘退出标志
                _ = tray_poll.tick() => {
                    if self.tray.as_ref().map_or(false, |t| t.exit_requested()) {
                        println!("👋 托盘退出信号已收到，daemon 即将停止...");
                        break;
                    }

                    let recording_elapsed = {
                        let state = self.state.lock().unwrap();
                        if state.is_recording {
                            state.start_time.map(|t| t.elapsed())
                        } else {
                            None
                        }
                    };

                    if let Some(elapsed) = recording_elapsed {
                        self.maybe_schedule_live_draft_transcription();

                        if elapsed.as_secs() >= MAX_RECORDING_DURATION_SECS {
                            eprintln!(
                                "⚠️  录音已达到最大时长（2 小时），已自动停止并开始转写。"
                            );
                            self.set_tray(TrayIconState::Transcribing);
                            if let Err(e) = self.stop_and_transcribe(&event_tx).await {
                                self.set_tray(TrayIconState::Idle);
                                eprintln!("⚠️  转写失败: {e}");
                            }
                        } else if elapsed.as_secs() >= RECORDING_WARNING_DURATION_SECS
                            && !self.recording_warning_issued.swap(true, Ordering::SeqCst)
                        {
                            let remain = MAX_RECORDING_DURATION_SECS.saturating_sub(elapsed.as_secs());
                            eprintln!(
                                "⚠️  录音时长接近上限，还可继续录制约 {} 秒。",
                                remain
                            );
                        }
                    }
                }
                _ = memory_heartbeat.tick() => {
                    self.log_memory_checkpoint("heartbeat", None);
                }
            }
        }

        self.log_memory_checkpoint("daemon_stop", None);

        Ok(())
    }

    fn set_tray(&self, state: TrayIconState) {
        if let Some(ref t) = self.tray {
            t.set_state(state);
        }
    }

    fn maybe_schedule_live_draft_transcription(&self) {
        if !self.draft_mode_active.load(Ordering::SeqCst) {
            return;
        }

        let is_recording = self.state.lock().unwrap().is_recording;
        if !is_recording {
            return;
        }

        let now = std::time::Instant::now();
        {
            let mut last_tick = self.draft_live_last_tick.lock().unwrap();
            if now.duration_since(*last_tick) < std::time::Duration::from_secs(3) {
                return;
            }
            *last_tick = now;
        }

        if self.draft_live_inflight.swap(true, Ordering::SeqCst) {
            return;
        }

        let sample_rate = self.audio_capture.get_info().sample_rate as usize;
        let min_chunk = sample_rate.saturating_mul(2);
        let prune_threshold = sample_rate.saturating_mul(30);
        let mut cursor = self.draft_live_cursor.load(Ordering::SeqCst);

        if cursor >= prune_threshold {
            let buffer_arc = self.recording_buffer.lock().unwrap().clone();
            let mut guard = buffer_arc.lock().unwrap();
            let drain_to = cursor.min(guard.len());
            if drain_to > 0 {
                guard.drain(0..drain_to);
                cursor = cursor.saturating_sub(drain_to);
                self.draft_live_cursor.store(cursor, Ordering::SeqCst);
            }
        }

        let chunk = {
            let buffer_arc = self.recording_buffer.lock().unwrap().clone();
            let guard = buffer_arc.lock().unwrap();
            if cursor >= guard.len() {
                Vec::new()
            } else {
                guard[cursor..].to_vec()
            }
        };

        if chunk.len() < min_chunk {
            self.draft_live_inflight.store(false, Ordering::SeqCst);
            return;
        }

        let provider = self.provider.clone();
        let inflight = self.draft_live_inflight.clone();
        let cursor_state = self.draft_live_cursor.clone();
        let draft_event_tx = self.draft_event_tx.clone();
        let draft_text = self.draft_live_text.clone();
        let chunk_len = chunk.len();
        let start_cursor = cursor;

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(TRANSCRIBE_TIMEOUT_SECS),
                provider.transcribe(&chunk, sample_rate as u32),
            )
            .await;

            if let Ok(Ok(r)) = result {
                cursor_state.store(start_cursor.saturating_add(chunk_len), Ordering::SeqCst);
                let text = r.text.trim().to_string();
                if !text.is_empty() {
                    {
                        let mut merged = draft_text.lock().unwrap();
                        if !merged.is_empty() {
                            merged.push(' ');
                        }
                        merged.push_str(&text);
                    }

                    if let Some(tx) = draft_event_tx {
                        let mut chunk_text = text;
                        chunk_text.push(' ');
                        let _ = tx.try_send(DraftPanelEvent::AppendText(chunk_text));
                    }
                }
            }

            inflight.store(false, Ordering::SeqCst);
        });
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
                        info!(
                            "[Hotkey] 第 {} 次 -> 动作: 停止录音并转写（toggle 模式）",
                            n
                        );
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
        self.log_memory_checkpoint("before_start_recording", None);

        {
            let state = self.state.lock().unwrap();
            if state.is_recording {
                println!("[Daemon] start_recording skipped already_recording=true");
                return Ok(());
            }
        }

        println!("[Daemon] start_recording state_checked");

        let session_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        println!("[Daemon] start_recording session_buffer_created");

        let stream = self
            .audio_capture
            .build_live_stream(session_buf.clone())
            .context("创建录音流失败")?;
        println!("[Daemon] start_recording build_live_stream_returned");

        *self.recording_buffer.lock().unwrap() = session_buf;
        println!("[Daemon] start_recording recording_buffer_stored");

        *self.active_stream.lock().unwrap() = Some(stream);
        println!("[Daemon] start_recording active_stream_stored");

        {
            let mut state = self.state.lock().unwrap();
            state.is_recording = true;
            state.start_time = Some(std::time::Instant::now());
        }
        self.recording_warning_issued.store(false, Ordering::SeqCst);
        self.draft_live_cursor.store(0, Ordering::SeqCst);
        self.draft_live_inflight.store(false, Ordering::SeqCst);
        *self.draft_live_last_tick.lock().unwrap() = std::time::Instant::now();
        if self.draft_mode_active.load(Ordering::SeqCst) {
            if let Some(ref tx) = self.draft_event_tx {
                let _ = tx.try_send(DraftPanelEvent::Show);
            }
        }
        println!("[Daemon] start_recording state_updated");

        let session_id = self.recording_session_count.fetch_add(1, Ordering::SeqCst) + 1;
        println!(
            "[Daemon] start_recording session_id_assigned={}",
            session_id
        );

        info!("[Hotkey] 录音已启动");
        self.log_memory_checkpoint(
            "after_start_recording",
            Some(format!("session_id={} session_buffer_len=0", session_id)),
        );
        println!("[Daemon] start_recording after_checkpoint");
        println!("🔴 录音中... 再按热键停止");
        Ok(())
    }

    async fn stop_and_transcribe(&self, tx: &mpsc::Sender<DaemonEvent>) -> Result<()> {
        self.is_processing.store(true, Ordering::SeqCst);
        self.recording_warning_issued.store(false, Ordering::SeqCst);
        self.log_memory_checkpoint("before_stop_and_transcribe", None);

        let duration = {
            let mut state = self.state.lock().unwrap();
            if !state.is_recording {
                return Ok(());
            }
            state.is_recording = false;
            state.start_time.map(|t| t.elapsed()).unwrap_or_default()
        };

        // 先拿走本次 session 的 Arc，再 drop stream
        // 这样即使旧 stream 有 stale 回调继续写入，也写入旧 Arc，不会影响下次 session
        let session_buf = self.recording_buffer.lock().unwrap().clone();
        drop(self.active_stream.lock().unwrap().take());

        let (buffer, session_buffer_len, session_buffer_capacity) = {
            let mut guard = session_buf.lock().unwrap();
            let len = guard.len();
            let capacity = guard.capacity();
            let moved = std::mem::take(&mut *guard);
            (moved, len, capacity)
        };
        info!(
            "[Hotkey] 录音已停止，开始转写 (时长 {:.1}s, {} 样本)",
            duration.as_secs_f32(),
            buffer.len()
        );
        self.log_memory_checkpoint(
            "after_recording_buffer_clone",
            Some(format!(
                "duration_ms={} sample_count={} session_buffer_len={} session_buffer_capacity={}",
                duration.as_millis(),
                buffer.len(),
                session_buffer_len,
                session_buffer_capacity,
            )),
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
            eprintln!(
                "   请前往：系统设置 > 隐私与安全性 > 麦克风，将 Open Flow.app 添加到列表并启用。"
            );
            eprintln!("   然后完全退出并重新打开 Open Flow。");
            return Ok(());
        }

        let sample_rate = self.audio_capture.get_info().sample_rate;
        let provider = self.provider.clone();
        let transcribe_started_at = std::time::Instant::now();

        if self.draft_mode_active.load(Ordering::SeqCst) {
            for _ in 0..30 {
                if !self.draft_live_inflight.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            let cursor = self
                .draft_live_cursor
                .load(Ordering::SeqCst)
                .min(buffer.len());
            let tail = &buffer[cursor..];
            if !tail.is_empty() {
                let tail_result = self
                    .transcribe_with_segments(provider.clone(), tail, sample_rate)
                    .await?;
                let tail_text = tail_result.text.trim().to_string();

                if !tail_text.is_empty() {
                    {
                        let mut merged = self.draft_live_text.lock().unwrap();
                        if !merged.is_empty() {
                            merged.push(' ');
                        }
                        merged.push_str(&tail_text);
                    }

                    if let Some(ref tx) = self.draft_event_tx {
                        let mut chunk_text = tail_text;
                        chunk_text.push(' ');
                        let _ = tx.try_send(DraftPanelEvent::AppendText(chunk_text));
                    }
                }
            }

            let final_text = self.draft_live_text.lock().unwrap().trim().to_string();
            let transcription_id = self.transcription_count.fetch_add(1, Ordering::SeqCst) + 1;
            self.log_memory_checkpoint(
                "after_transcribe",
                Some(format!(
                    "transcription_id={} duration_ms={} sample_count={} tail_sample_count={} text_chars={}",
                    transcription_id,
                    transcribe_started_at.elapsed().as_millis(),
                    buffer.len(),
                    tail.len(),
                    final_text.chars().count(),
                )),
            );

            tx.send(DaemonEvent::TranscriptionComplete(final_text))
                .await
                .ok();
            return Ok(());
        }

        let result = match self
            .transcribe_with_segments(provider, &buffer, sample_rate)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.is_processing.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };

        let transcription_id = self.transcription_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.log_memory_checkpoint(
            "after_transcribe",
            Some(format!(
                "transcription_id={} duration_ms={} sample_count={} text_chars={} provider_duration_ms={}",
                transcription_id,
                transcribe_started_at.elapsed().as_millis(),
                buffer.len(),
                result.text.chars().count(),
                result.duration_ms,
            )),
        );

        tx.send(DaemonEvent::TranscriptionComplete(result.text))
            .await
            .ok();

        Ok(())
    }

    async fn transcribe_with_segments(
        &self,
        provider: Arc<dyn AsrProvider>,
        audio: &[f32],
        sample_rate: u32,
    ) -> Result<TranscriptionResult> {
        let chunk_samples = (sample_rate as usize)
            .saturating_mul(TRANSCRIBE_SEGMENT_SECS as usize)
            .max(1);

        if audio.len() <= chunk_samples {
            return match tokio::time::timeout(
                std::time::Duration::from_secs(TRANSCRIBE_TIMEOUT_SECS),
                provider.transcribe(audio, sample_rate),
            )
            .await
            {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(e)) => Err(e),
                Err(_elapsed) => {
                    anyhow::bail!(
                        "转写超时（>{}s），请检查模型状态或缩短录音",
                        TRANSCRIBE_TIMEOUT_SECS
                    )
                }
            };
        }

        let total_segments = (audio.len() + chunk_samples - 1) / chunk_samples;
        println!(
            "📦 检测到长录音，启用分段转写：{} 段（每段 {} 秒）",
            total_segments, TRANSCRIBE_SEGMENT_SECS
        );

        let started_at = std::time::Instant::now();
        let mut merged_texts: Vec<String> = Vec::with_capacity(total_segments);
        let mut confidence_sum = 0.0f32;
        let mut confidence_count: u32 = 0;
        let mut language: Option<String> = None;

        for (idx, chunk) in audio.chunks(chunk_samples).enumerate() {
            let segment_no = idx + 1;
            println!(
                "🧩 分段转写 {}/{}（{} 样本）",
                segment_no,
                total_segments,
                chunk.len()
            );

            let segment = match tokio::time::timeout(
                std::time::Duration::from_secs(TRANSCRIBE_TIMEOUT_SECS),
                provider.transcribe(chunk, sample_rate),
            )
            .await
            {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    return Err(e)
                        .context(format!("第 {}/{} 段转写失败", segment_no, total_segments));
                }
                Err(_elapsed) => {
                    anyhow::bail!(
                        "第 {}/{} 段转写超时（>{}s）",
                        segment_no,
                        total_segments,
                        TRANSCRIBE_TIMEOUT_SECS
                    );
                }
            };

            let trimmed = segment.text.trim();
            if !trimmed.is_empty() {
                merged_texts.push(trimmed.to_string());
            }

            confidence_sum += segment.confidence;
            confidence_count += 1;

            if language.is_none() {
                language = segment.language.clone();
            }
        }

        Ok(TranscriptionResult {
            text: merged_texts.join(" "),
            confidence: if confidence_count > 0 {
                confidence_sum / confidence_count as f32
            } else {
                0.0
            },
            language,
            duration_ms: started_at.elapsed().as_millis() as u64,
        })
    }

    fn log_memory_checkpoint(&self, checkpoint: &str, extra: Option<String>) {
        let state = self.state.lock().unwrap();
        let buffer_info = {
            let buffer_arc = self.recording_buffer.lock().unwrap().clone();
            let buffer = buffer_arc.lock().unwrap();
            (buffer.len(), buffer.capacity())
        };
        let active_stream = self.active_stream.lock().unwrap().is_some();
        let sessions = self.recording_session_count.load(Ordering::SeqCst);
        let transcriptions = self.transcription_count.load(Ordering::SeqCst);

        let suffix = extra
            .as_ref()
            .map(|v| format!(" {}", v))
            .unwrap_or_default();

        if let Some(snapshot) = memory::sample_process_memory() {
            println!(
                "[Mem] checkpoint={} rss={} vsz={} recording={} processing={} active_stream={} buffer_len={} buffer_cap={} sessions={} transcriptions={}{}",
                checkpoint,
                memory::format_bytes(snapshot.rss_bytes),
                memory::format_bytes(snapshot.vsz_bytes),
                state.is_recording,
                self.is_processing.load(Ordering::SeqCst),
                active_stream,
                buffer_info.0,
                buffer_info.1,
                sessions,
                transcriptions,
                suffix,
            );
        } else {
            println!(
                "[Mem] checkpoint={} rss=<unavailable> vsz=<unavailable> recording={} processing={} active_stream={} buffer_len={} buffer_cap={} sessions={} transcriptions={}{}",
                checkpoint,
                state.is_recording,
                self.is_processing.load(Ordering::SeqCst),
                active_stream,
                buffer_info.0,
                buffer_info.1,
                sessions,
                transcriptions,
                suffix,
            );
        }
    }
}

pub async fn run_daemon(
    provider: Arc<dyn AsrProvider>,
    tray: Option<Arc<TrayHandle>>,
    draft_mode_active: Arc<AtomicBool>,
    draft_event_tx: Option<std::sync::mpsc::SyncSender<DraftPanelEvent>>,
) -> Result<()> {
    let daemon = Daemon::new(provider, tray, draft_mode_active, draft_event_tx)?;
    daemon.run().await
}
