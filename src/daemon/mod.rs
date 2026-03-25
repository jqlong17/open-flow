use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::info;

use crate::asr::AsrProvider;
use crate::audio::AudioCapture;
use crate::common::memory;
use crate::common::perf::{
    empty_resource_snapshot, now_unix_ms, sample_process_resources, PerformanceLogEntry,
    PerformanceLogWriter, ProcessResourceSnapshot, PERF_SCHEMA_VERSION,
};
use crate::common::types::{HotkeyEvent, RecordingState, TranscriptionResult};
use crate::draft_panel::DraftPanelEvent;
use crate::hotkey::{
    check_accessibility_permission, request_accessibility_permission, HotkeyListener,
};
use crate::text_injection::TextInjector;
use crate::tray::{TrayHandle, TrayIconState};
use open_flow::correction::TextCorrector;
use open_flow::meeting_notes::{MeetingSessionWriter, MeetingTranscriptEntry};
use open_flow::system_audio::SystemAudioCapture;

/// Daemon 事件类型
#[derive(Debug)]
pub enum DaemonEvent {
    Hotkey(HotkeyEvent),
    TranscriptionComplete(CompletedTranscription),
    HotkeyListenerDead,
}

#[derive(Debug)]
pub(crate) struct CompletedTranscription {
    text: String,
    perf_entry: Option<PerformanceLogEntry>,
}

struct CorrectionOutcome {
    text: String,
    attempted: bool,
    changed: bool,
    duration_ms: u64,
    status: &'static str,
}

#[derive(Debug)]
struct AsrExecutionSummary {
    result: TranscriptionResult,
    segmented: bool,
    segment_count: usize,
}

#[derive(Clone, Copy)]
struct ActivePerformanceSession {
    session_id: u64,
    started_at_ms: u64,
    resource_at_record_start: ProcessResourceSnapshot,
}

#[derive(Clone)]
struct DualMeetingLiveState {
    session_writer: MeetingSessionWriter,
    entries: Arc<Mutex<Vec<MeetingTranscriptEntry>>>,
    microphone_cursor: Arc<AtomicUsize>,
    system_audio_cursor: Arc<AtomicUsize>,
    microphone_inflight: Arc<AtomicBool>,
    system_audio_inflight: Arc<AtomicBool>,
    microphone_processed_samples: Arc<AtomicUsize>,
    system_audio_processed_samples: Arc<AtomicUsize>,
    microphone_segment_index: Arc<AtomicU64>,
    system_audio_segment_index: Arc<AtomicU64>,
}

pub struct Daemon {
    state: Arc<Mutex<RecordingState>>,
    is_processing: AtomicBool,
    hotkey_recv_count: std::sync::atomic::AtomicU64,
    recording_session_count: std::sync::atomic::AtomicU64,
    transcription_count: std::sync::atomic::AtomicU64,
    recording_warning_issued: AtomicBool,
    audio_capture: Option<AudioCapture>,
    provider: Arc<dyn AsrProvider>,
    text_injector: TextInjector,
    active_stream: Mutex<Option<cpal::Stream>>,
    active_system_audio: Mutex<Option<SystemAudioCapture>>,
    recording_buffer: Mutex<Arc<Mutex<Vec<f32>>>>,
    microphone_recording_buffer: Mutex<Option<Arc<Mutex<Vec<f32>>>>>,
    system_audio_recording_buffer: Mutex<Option<Arc<Mutex<Vec<f32>>>>>,
    tray: Option<Arc<TrayHandle>>,
    hotkey: String,
    trigger_mode: String,
    capture_mode: String,
    draft_mode_active: Arc<AtomicBool>,
    draft_event_tx: Option<std::sync::mpsc::SyncSender<DraftPanelEvent>>,
    draft_live_cursor: Arc<AtomicUsize>,
    draft_live_inflight: Arc<AtomicBool>,
    draft_live_last_tick: Mutex<std::time::Instant>,
    draft_live_text: Arc<Mutex<String>>,
    text_corrector: Option<TextCorrector>,
    correction_config_enabled: bool,
    correction_api_key_configured: bool,
    correction_model_name: String,
    correction_vocab_count: usize,
    system_audio_target_pid: String,
    system_audio_target_name: String,
    performance_logger: PerformanceLogWriter,
    performance_log_enabled: bool,
    current_recording_session: Mutex<Option<ActivePerformanceSession>>,
    dual_meeting_live_state: Mutex<Option<DualMeetingLiveState>>,
}

const MAX_RECORDING_DURATION_SECS: u64 = 2 * 60 * 60;
const RECORDING_WARNING_DURATION_SECS: u64 = MAX_RECORDING_DURATION_SECS - 5 * 60;
const TRANSCRIBE_SEGMENT_SECS: u64 = 60;
const TRANSCRIBE_TIMEOUT_SECS: u64 = 120;
const DUAL_MEETING_SEGMENT_SECS: u64 = 10;

impl Daemon {
    fn personal_vocabulary_count() -> usize {
        crate::common::config::Config::personal_vocabulary_path()
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|content| {
                content
                    .lines()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .count()
            })
            .unwrap_or(0)
    }

    fn uses_microphone_capture(&self) -> bool {
        matches!(
            self.capture_mode.as_str(),
            "microphone" | "system_audio_microphone"
        )
    }

    fn uses_system_audio_capture(&self) -> bool {
        matches!(
            self.capture_mode.as_str(),
            "system_audio_desktop" | "system_audio_application" | "system_audio_microphone"
        )
    }

    fn is_dual_capture_mode(&self) -> bool {
        self.capture_mode == "system_audio_microphone"
    }

    pub fn new(
        provider: Arc<dyn AsrProvider>,
        tray: Option<Arc<TrayHandle>>,
        draft_mode_active: Arc<AtomicBool>,
        draft_event_tx: Option<std::sync::mpsc::SyncSender<DraftPanelEvent>>,
    ) -> Result<Self> {
        let config = crate::common::config::Config::load().unwrap_or_default();
        let capture_mode = config.resolved_capture_mode();
        let uses_microphone = matches!(
            capture_mode.as_str(),
            "microphone" | "system_audio_microphone"
        );
        let uses_system_audio = matches!(
            capture_mode.as_str(),
            "system_audio_desktop" | "system_audio_application" | "system_audio_microphone"
        );
        let audio_capture = if uses_microphone {
            Some(
                AudioCapture::new_with_device_name(config.resolved_input_source().as_deref())
                    .context("初始化音频采集器失败")?,
            )
        } else {
            None
        };
        if uses_system_audio && !SystemAudioCapture::helper_available() {
            anyhow::bail!("系统音频 helper 不可用，请先构建或重新打包 Open Flow.app");
        }
        let text_injector = TextInjector::new();
        let text_corrector = TextCorrector::from_config(&config);
        let correction_config_enabled = config.correction_enabled();
        let correction_api_key_configured = !config.resolved_correction_api_key().trim().is_empty();
        let correction_model_name = config.resolved_correction_model();
        let correction_vocab_count = Self::personal_vocabulary_count();
        let performance_logger = PerformanceLogWriter::from_config(&config)?;
        let performance_log_enabled = performance_logger.enabled();

        println!(
            "[Pipeline] startup provider={} capture_mode={} input_source={} system_audio_target_pid={} correction_config_enabled={} correction_runtime_enabled={} correction_api_key_configured={} correction_model={} vocabulary_terms={} performance_log_enabled={}",
            provider.name(),
            config.resolved_capture_mode(),
            config
                .resolved_input_source()
                .unwrap_or_else(|| "system_default".to_string()),
            if config.system_audio_target_pid.trim().is_empty() {
                "none".to_string()
            } else {
                config.system_audio_target_pid.clone()
            },
            correction_config_enabled,
            text_corrector.is_some(),
            correction_api_key_configured,
            correction_model_name,
            correction_vocab_count,
            performance_log_enabled
        );

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
            active_system_audio: Mutex::new(None),
            recording_buffer: Mutex::new(Arc::new(Mutex::new(Vec::new()))),
            microphone_recording_buffer: Mutex::new(None),
            system_audio_recording_buffer: Mutex::new(None),
            tray,
            hotkey: config.hotkey.clone(),
            trigger_mode: config.trigger_mode,
            capture_mode,
            draft_mode_active,
            draft_event_tx,
            draft_live_cursor: Arc::new(AtomicUsize::new(0)),
            draft_live_inflight: Arc::new(AtomicBool::new(false)),
            draft_live_last_tick: Mutex::new(std::time::Instant::now()),
            draft_live_text: Arc::new(Mutex::new(String::new())),
            text_corrector,
            correction_config_enabled,
            correction_api_key_configured,
            correction_model_name,
            correction_vocab_count,
            system_audio_target_pid: config.system_audio_target_pid.clone(),
            system_audio_target_name: config.system_audio_target_name.clone(),
            performance_logger,
            performance_log_enabled,
            current_recording_session: Mutex::new(None),
            dual_meeting_live_state: Mutex::new(None),
        })
    }

    pub async fn run(self) -> Result<()> {
        let ui = crate::common::config::Config::load()
            .map(|config| crate::common::ui::UiLanguage::from_config(&config))
            .unwrap_or_default();
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
        let microphone_required = self.uses_microphone_capture();
        let microphone_ok = if microphone_required {
            crate::hotkey::check_microphone_permission()
        } else {
            true
        };
        println!("{}", ui.pick("🔎 权限诊断", "🔎 Permission Diagnostics"));
        println!(
            "   {} {}",
            ui.pick("可执行文件:", "Executable:"),
            current_exe
        );
        println!("   Accessibility: {}", accessibility_ok);
        println!("   Input Monitoring: {}", input_monitoring_ok);
        println!("   Microphone: {}", microphone_ok);

        // 请求缺失的权限（触发系统对话框）
        if !accessibility_ok {
            println!();
            println!(
                "{}",
                ui.pick(
                    "⚠️  Accessibility 权限未授权——正在请求...",
                    "⚠️  Accessibility permission not granted. Requesting now..."
                )
            );
            request_accessibility_permission();
        }
        if !input_monitoring_ok {
            println!();
            println!(
                "{}",
                ui.pick(
                    "⚠️  Input Monitoring 权限未授权——正在请求...",
                    "⚠️  Input Monitoring permission not granted. Requesting now..."
                )
            );
            crate::hotkey::request_input_monitoring_permission();
        }
        if microphone_required && !microphone_ok {
            println!();
            println!(
                "{}",
                ui.pick(
                    "⚠️  麦克风权限尚未授权。",
                    "⚠️  Microphone permission is not granted yet."
                )
            );
            crate::hotkey::request_microphone_permission();
        }

        if !accessibility_ok || !input_monitoring_ok {
            println!();
            println!("{}", ui.pick("⏳ 等待权限授权... 请在系统设置中授权，然后应用将重试。", "⏳ Waiting for permissions... Grant access in System Settings, then the app will retry."));
            println!("{}", ui.pick("   （如果授权后热键不工作，请重启应用）", "   (If the hotkey still does not work after granting access, restart the app.)"));
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
        let audio_info = self.current_audio_info();

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
        println!(
            "{}",
            ui.pick("✅ Open Flow 已就绪", "✅ Open Flow is ready")
        );
        println!(
            "   {} {}",
            ui.pick("捕获模式:", "Capture Mode:"),
            match self.capture_mode.as_str() {
                "system_audio_microphone" => {
                    ui.pick("桌面音频 + 麦克风（会议）", "Desktop Audio + Microphone (Meeting)")
                }
                "system_audio_desktop" => {
                    ui.pick("桌面音频（实验）", "Desktop Audio (Experimental)")
                }
                "system_audio_application" => {
                    ui.pick("应用音频（实验）", "Application Audio (Experimental)")
                }
                _ => ui.pick("麦克风", "Microphone"),
            }
        );
        println!(
            "   {} {}",
            ui.pick("输入设备:", "Input Device:"),
            audio_info.device_name
        );
        println!(
            "{}",
            ui.pick(
                format!(
                    "   音频设备: {}Hz / {} 通道",
                    audio_info.sample_rate, audio_info.channels
                ),
                format!(
                    "   Audio: {}Hz / {} channels",
                    audio_info.sample_rate, audio_info.channels
                ),
            )
        );
        if self.is_dual_capture_mode() {
            if let Some(mic_info) = self.microphone_audio_info() {
                println!(
                    "{}",
                    ui.pick(
                        format!(
                            "   麦克风: {} / {}Hz / {} 通道",
                            mic_info.device_name, mic_info.sample_rate, mic_info.channels
                        ),
                        format!(
                            "   Microphone: {} / {}Hz / {} channels",
                            mic_info.device_name, mic_info.sample_rate, mic_info.channels
                        ),
                    )
                );
            }
            if let Some(system_info) = self.system_audio_info() {
                println!(
                    "{}",
                    ui.pick(
                        format!(
                            "   系统音频: {} / {}Hz / {} 通道",
                            system_info.device_name, system_info.sample_rate, system_info.channels
                        ),
                        format!(
                            "   System Audio: {} / {}Hz / {} channels",
                            system_info.device_name, system_info.sample_rate, system_info.channels
                        ),
                    )
                );
            }
        }
        println!("   Provider: {}", self.provider.name());
        println!();
        println!("{}", ui.pick("🎙️  按热键开始录音，再按一次停止并转写", "🎙️  Press the hotkey to start recording, then press it again to stop and transcribe"));
        println!("{}", ui.pick("   托盘图标可查看状态（灰=待机 红=录音 黄=转写）", "   Check the tray icon for status (gray = idle, red = recording, yellow = transcribing)"));
        println!();

        // ── 主事件循环 ────────────────────────────────────────────────
        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    match event {
                        DaemonEvent::Hotkey(ev) => {
                            self.handle_hotkey(ev, &event_tx).await;
                        }
                        DaemonEvent::TranscriptionComplete(mut completed) => {
                            self.is_processing.store(false, Ordering::SeqCst);
                            self.set_tray(TrayIconState::Idle);
                            println!(
                                "{}",
                                ui.pick(
                                    format!("📝 转写完成: {}", completed.text),
                                    format!("📝 Transcription complete: {}", completed.text),
                                )
                            );
                            let output_started_at = std::time::Instant::now();
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            if self.draft_mode_active.load(Ordering::SeqCst) {
                                if let Some(ref tx) = self.draft_event_tx {
                                    let _ = tx.try_send(DraftPanelEvent::SetText(completed.text.clone()));
                                }
                            } else {
                                info!("[Hotkey] 开始粘贴");
                                if let Err(e) = self.text_injector.inject(&completed.text).await {
                                    eprintln!("⚠️  文字注入失败: {e}");
                                }
                                info!("[Hotkey] 粘贴结束");
                            }

                            if let Some(ref mut perf_entry) = completed.perf_entry {
                                perf_entry.output_duration_ms = output_started_at.elapsed().as_millis() as u64;
                                perf_entry.total_e2e_ms =
                                    perf_entry.total_e2e_ms.saturating_add(perf_entry.output_duration_ms);
                                perf_entry.resource_after_pipeline = self.sample_resource_snapshot();
                                self.write_performance_entry(perf_entry);
                            }
                        }
                        DaemonEvent::HotkeyListenerDead => {
                            eprintln!(
                                "{}",
                                ui.pick(
                                    "❌ 热键监听线程已退出，daemon 停止。请运行 open-flow start 重启。",
                                    "❌ The hotkey listener thread exited, so the daemon stopped. Run `open-flow start` to restart it.",
                                )
                            );
                            break;
                        }
                    }
                }
                        // daemon 每 200ms 检查托盘退出标志
                _ = tray_poll.tick() => {
                    if self.tray.as_ref().map_or(false, |t| t.exit_requested()) {
                        println!(
                            "{}",
                            ui.pick(
                                "👋 托盘退出信号已收到，daemon 即将停止...",
                                "👋 Quit signal received from the tray. The daemon will stop now...",
                            )
                        );
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
                        if self.is_dual_capture_mode() {
                            self.maybe_schedule_dual_live_transcription();
                        }
                        self.maybe_schedule_live_draft_transcription();

                        if elapsed.as_secs() >= MAX_RECORDING_DURATION_SECS {
                            eprintln!(
                                "{}",
                                ui.pick(
                                    "⚠️  录音已达到最大时长（2 小时），已自动停止并开始转写。",
                                    "⚠️  Recording reached the maximum duration (2 hours). It was stopped automatically and transcription has started.",
                                )
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
                                "{}",
                                ui.pick(
                                    format!("⚠️  录音时长接近上限，还可继续录制约 {} 秒。", remain),
                                    format!("⚠️  Recording is close to the limit. About {} seconds remain.", remain),
                                )
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

        if self.is_dual_capture_mode() {
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

        let sample_rate = self.current_audio_info().sample_rate as usize;
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

    fn maybe_schedule_dual_live_transcription(&self) {
        if !self.is_dual_capture_mode() {
            return;
        }

        let is_recording = self.state.lock().unwrap().is_recording;
        if !is_recording {
            return;
        }

        let Some(live_state) = self.dual_meeting_live_state.lock().unwrap().clone() else {
            return;
        };

        if let (Some(buffer_arc), Some(audio_info)) = (
            self.system_audio_recording_buffer.lock().unwrap().clone(),
            self.system_audio_info(),
        ) {
            self.maybe_schedule_dual_source_segment(
                live_state.clone(),
                buffer_arc,
                live_state.system_audio_cursor.clone(),
                live_state.system_audio_inflight.clone(),
                live_state.system_audio_processed_samples.clone(),
                live_state.system_audio_segment_index.clone(),
                "system_audio",
                "对方",
                audio_info.sample_rate,
            );
        }

        if let (Some(buffer_arc), Some(audio_info)) = (
            self.microphone_recording_buffer.lock().unwrap().clone(),
            self.microphone_audio_info(),
        ) {
            self.maybe_schedule_dual_source_segment(
                live_state.clone(),
                buffer_arc,
                live_state.microphone_cursor.clone(),
                live_state.microphone_inflight.clone(),
                live_state.microphone_processed_samples.clone(),
                live_state.microphone_segment_index.clone(),
                "microphone",
                "我",
                audio_info.sample_rate,
            );
        }
    }

    fn maybe_schedule_dual_source_segment(
        &self,
        live_state: DualMeetingLiveState,
        buffer_arc: Arc<Mutex<Vec<f32>>>,
        cursor: Arc<AtomicUsize>,
        inflight: Arc<AtomicBool>,
        processed_samples: Arc<AtomicUsize>,
        segment_index: Arc<AtomicU64>,
        source: &'static str,
        role_label: &'static str,
        sample_rate: u32,
    ) {
        if inflight.load(Ordering::SeqCst) {
            return;
        }

        let chunk_samples = (sample_rate as usize)
            .saturating_mul(DUAL_MEETING_SEGMENT_SECS as usize)
            .max(1);
        let (start_cursor, start_sample_offset, chunk) = {
            let guard = buffer_arc.lock().unwrap();
            let cursor_value = cursor.load(Ordering::SeqCst);
            if cursor_value >= guard.len() || guard.len().saturating_sub(cursor_value) < chunk_samples {
                return;
            }
            let end = cursor_value.saturating_add(chunk_samples);
            let start_offset = processed_samples.load(Ordering::SeqCst).saturating_add(cursor_value);
            (cursor_value, start_offset, guard[cursor_value..end].to_vec())
        };

        let consumed_samples = chunk.len();
        let started_at_ms = start_sample_offset as u64 * 1000 / sample_rate as u64;
        let ended_at_ms =
            (start_sample_offset.saturating_add(consumed_samples)) as u64 * 1000 / sample_rate as u64;
        cursor.store(start_cursor.saturating_add(consumed_samples), Ordering::SeqCst);
        inflight.store(true, Ordering::SeqCst);

        let provider = self.provider.clone();
        let draft_event_tx = self.draft_event_tx.clone();
        let draft_mode_active = self.draft_mode_active.clone();

        tokio::spawn(async move {
            let entry_result = Self::process_dual_source_segment(
                live_state.session_writer.clone(),
                live_state.entries.clone(),
                provider,
                chunk,
                source,
                role_label,
                sample_rate,
                segment_index.fetch_add(1, Ordering::SeqCst) + 1,
                started_at_ms,
                ended_at_ms,
            )
            .await;

            if let Ok(Some(entry)) = entry_result {
                if draft_mode_active.load(Ordering::SeqCst) {
                    if let Some(tx) = draft_event_tx {
                        let _ = tx.try_send(DraftPanelEvent::AppendText(format!(
                            "{}：{}\n",
                            entry.role_label, entry.text
                        )));
                    }
                }
            } else if let Err(err) = entry_result {
                eprintln!(
                    "[Meeting] failed_to_process_segment source={} error={}",
                    source, err
                );
            }

            if let Ok(mut guard) = buffer_arc.lock() {
                let drain_to = consumed_samples.min(guard.len());
                if drain_to > 0 {
                    guard.drain(0..drain_to);
                }
            }
            processed_samples.fetch_add(consumed_samples, Ordering::SeqCst);
            cursor.fetch_sub(consumed_samples, Ordering::SeqCst);
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
        let microphone_buf = if self.uses_microphone_capture() {
            Some(Arc::new(Mutex::new(Vec::new())))
        } else {
            None
        };
        let system_audio_buf = if self.uses_system_audio_capture() {
            Some(Arc::new(Mutex::new(Vec::new())))
        } else {
            None
        };
        println!("[Daemon] start_recording session_buffer_created");

        match self.capture_mode.as_str() {
            "microphone" => {
                let stream = self
                    .audio_capture
                    .as_ref()
                    .context("麦克风模式下音频采集器未初始化")?
                    .build_live_stream(session_buf.clone())
                    .context("创建录音流失败")?;
                println!("[Daemon] start_recording build_live_stream_returned");
                *self.active_stream.lock().unwrap() = Some(stream);
                println!("[Daemon] start_recording active_stream_stored");
                *self.microphone_recording_buffer.lock().unwrap() = Some(session_buf.clone());
                *self.system_audio_recording_buffer.lock().unwrap() = None;
            }
            "system_audio_desktop" | "system_audio_application" => {
                let mut config = crate::common::config::Config::load().unwrap_or_default();
                config.capture_mode = self.capture_mode.clone();
                config.system_audio_target_pid = self.system_audio_target_pid.clone();
                config.system_audio_target_name = self.system_audio_target_name.clone();
                let capture = SystemAudioCapture::spawn_from_config(&config, session_buf.clone())
                    .context("启动系统音频流失败")?;
                println!("[Daemon] start_recording system_audio_capture_started");
                *self.active_system_audio.lock().unwrap() = Some(capture);
                *self.microphone_recording_buffer.lock().unwrap() = None;
                *self.system_audio_recording_buffer.lock().unwrap() = Some(session_buf.clone());
            }
            "system_audio_microphone" => {
                let microphone_buf = microphone_buf
                    .clone()
                    .context("会议双路模式下麦克风缓冲区未初始化")?;
                let system_audio_buf = system_audio_buf
                    .clone()
                    .context("会议双路模式下系统音频缓冲区未初始化")?;

                let stream = self
                    .audio_capture
                    .as_ref()
                    .context("会议双路模式下麦克风采集器未初始化")?
                    .build_live_stream(microphone_buf.clone())
                    .context("创建麦克风录音流失败")?;
                println!("[Daemon] start_recording build_live_stream_returned");

                let mut config = crate::common::config::Config::load().unwrap_or_default();
                config.capture_mode = "system_audio_desktop".to_string();
                config.system_audio_target_pid.clear();
                config.system_audio_target_name.clear();
                let capture =
                    SystemAudioCapture::spawn_from_config(&config, system_audio_buf.clone())
                        .context("启动桌面系统音频流失败")?;
                println!("[Daemon] start_recording system_audio_capture_started");
                *self.active_stream.lock().unwrap() = Some(stream);
                println!("[Daemon] start_recording active_stream_stored");
                *self.active_system_audio.lock().unwrap() = Some(capture);

                *self.microphone_recording_buffer.lock().unwrap() = Some(microphone_buf);
                *self.system_audio_recording_buffer.lock().unwrap() = Some(system_audio_buf);
            }
            other => anyhow::bail!("未知录音模式: {}", other),
        }

        *self.recording_buffer.lock().unwrap() = session_buf;
        println!("[Daemon] start_recording recording_buffer_stored");

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
        if self.is_dual_capture_mode() {
            let session_writer = MeetingSessionWriter::create(session_id, &self.capture_mode)
                .context("创建会议会话落盘目录失败")?;
            println!(
                "[Meeting] session_started session_id={} dir={}",
                session_id,
                session_writer.directory().display()
            );
            *self.dual_meeting_live_state.lock().unwrap() = Some(DualMeetingLiveState {
                session_writer,
                entries: Arc::new(Mutex::new(Vec::new())),
                microphone_cursor: Arc::new(AtomicUsize::new(0)),
                system_audio_cursor: Arc::new(AtomicUsize::new(0)),
                microphone_inflight: Arc::new(AtomicBool::new(false)),
                system_audio_inflight: Arc::new(AtomicBool::new(false)),
                microphone_processed_samples: Arc::new(AtomicUsize::new(0)),
                system_audio_processed_samples: Arc::new(AtomicUsize::new(0)),
                microphone_segment_index: Arc::new(AtomicU64::new(0)),
                system_audio_segment_index: Arc::new(AtomicU64::new(0)),
            });
        } else {
            *self.dual_meeting_live_state.lock().unwrap() = None;
        }
        let performance_session = ActivePerformanceSession {
            session_id,
            started_at_ms: now_unix_ms(),
            resource_at_record_start: self.sample_resource_snapshot(),
        };
        *self.current_recording_session.lock().unwrap() = Some(performance_session);
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
        let ui = crate::common::config::Config::load()
            .map(|config| crate::common::ui::UiLanguage::from_config(&config))
            .unwrap_or_default();
        println!(
            "{}",
            ui.pick(
                "🔴 录音中... 再按热键停止",
                "🔴 Recording... press the hotkey again to stop",
            )
        );
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
        let perf_session = self.current_recording_session.lock().unwrap().take();
        let session_buf = self.recording_buffer.lock().unwrap().clone();
        let microphone_buf = self.microphone_recording_buffer.lock().unwrap().clone();
        let system_audio_buf = self.system_audio_recording_buffer.lock().unwrap().clone();
        let dual_live_state = self.dual_meeting_live_state.lock().unwrap().clone();
        drop(self.active_stream.lock().unwrap().take());
        if let Some(capture) = self.active_system_audio.lock().unwrap().take() {
            capture.stop();
        }

        if self.is_dual_capture_mode() {
            return self
                .stop_and_transcribe_dual(
                    tx,
                    duration,
                    perf_session,
                    dual_live_state,
                    microphone_buf,
                    system_audio_buf,
                )
                .await;
        }

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
        let ui = crate::common::config::Config::load()
            .map(|config| crate::common::ui::UiLanguage::from_config(&config))
            .unwrap_or_default();
        println!(
            "{}",
            ui.pick(
                format!(
                    "⏹️  录音停止 ({:.1}s / {} 样本)，正在转写...",
                    duration.as_secs_f32(),
                    buffer.len()
                ),
                format!(
                    "⏹️  Recording stopped ({:.1}s / {} samples), transcribing...",
                    duration.as_secs_f32(),
                    buffer.len()
                ),
            )
        );

        let resource_after_record_stop = self.sample_resource_snapshot();

        if buffer.is_empty() {
            self.is_processing.store(false, Ordering::SeqCst);
            self.emit_performance_log(PerformanceLogEntry {
                schema_version: PERF_SCHEMA_VERSION,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                created_at_ms: now_unix_ms(),
                session_id: perf_session.map(|s| s.session_id).unwrap_or(0),
                status: "empty_buffer".to_string(),
                provider: self.provider.name().to_string(),
                trigger_mode: self.trigger_mode.clone(),
                hotkey: self.hotkey.clone(),
                segmented: false,
                segment_count: 0,
                sample_rate: self.current_audio_info().sample_rate,
                sample_count: 0,
                recording_duration_ms: duration.as_millis() as u64,
                asr_duration_ms: 0,
                llm_duration_ms: 0,
                total_pipeline_ms: 0,
                output_duration_ms: 0,
                total_e2e_ms: perf_session
                    .map(|s| now_unix_ms().saturating_sub(s.started_at_ms))
                    .unwrap_or(0),
                llm_attempted: false,
                llm_changed: false,
                llm_status: "skipped".to_string(),
                text_chars: 0,
                confidence: 0.0,
                language: None,
                max_amplitude: 0.0,
                rms: 0.0,
                error: Some("recording buffer was empty".to_string()),
                resource_at_record_start: perf_session
                    .map(|s| s.resource_at_record_start)
                    .unwrap_or_else(empty_resource_snapshot),
                resource_after_record_stop,
                resource_after_asr: empty_resource_snapshot(),
                resource_after_pipeline: empty_resource_snapshot(),
            });
            eprintln!(
                "{}",
                if self.capture_mode == "microphone" {
                    ui.pick(
                        "⚠️  录音为空，请检查麦克风权限（系统设置 > 隐私与安全性 > 麦克风）",
                        "⚠️  The recording is empty. Please check microphone permission in System Settings > Privacy & Security > Microphone.",
                    )
                } else {
                    ui.pick(
                        "⚠️  系统音频缓冲区为空，请确认目标正在发声，并且屏幕录制权限已授权。",
                        "⚠️  The system-audio buffer is empty. Make sure the target is producing sound and screen recording permission is granted.",
                    )
                }
            );
            return Ok(());
        }

        let sample_rate = self.current_audio_info().sample_rate;

        // 振幅诊断：如果音量过低说明麦克风权限缺失或静音
        let max_amp = buffer.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let rms = (buffer.iter().map(|x| x * x).sum::<f32>() / buffer.len() as f32).sqrt();
        info!("[Audio] max_amp={:.6} rms={:.6}", max_amp, rms);
        if max_amp < 0.001 {
            self.is_processing.store(false, Ordering::SeqCst);
            self.emit_performance_log(PerformanceLogEntry {
                schema_version: PERF_SCHEMA_VERSION,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                created_at_ms: now_unix_ms(),
                session_id: perf_session.map(|s| s.session_id).unwrap_or(0),
                status: "low_amplitude".to_string(),
                provider: self.provider.name().to_string(),
                trigger_mode: self.trigger_mode.clone(),
                hotkey: self.hotkey.clone(),
                segmented: false,
                segment_count: 0,
                sample_rate,
                sample_count: buffer.len(),
                recording_duration_ms: duration.as_millis() as u64,
                asr_duration_ms: 0,
                llm_duration_ms: 0,
                total_pipeline_ms: 0,
                output_duration_ms: 0,
                total_e2e_ms: perf_session
                    .map(|s| now_unix_ms().saturating_sub(s.started_at_ms))
                    .unwrap_or(0),
                llm_attempted: false,
                llm_changed: false,
                llm_status: "skipped".to_string(),
                text_chars: 0,
                confidence: 0.0,
                language: None,
                max_amplitude: max_amp,
                rms,
                error: Some(format!("audio amplitude too low: max_amp={max_amp:.6}")),
                resource_at_record_start: perf_session
                    .map(|s| s.resource_at_record_start)
                    .unwrap_or_else(empty_resource_snapshot),
                resource_after_record_stop,
                resource_after_asr: empty_resource_snapshot(),
                resource_after_pipeline: empty_resource_snapshot(),
            });
            eprintln!(
                "{}",
                if self.capture_mode == "microphone" {
                    ui.pick(
                        format!("⚠️  录音振幅极低（max={:.6}），麦克风可能未授权。", max_amp),
                        format!("⚠️  Recording amplitude is very low (max={:.6}). Microphone permission may be missing.", max_amp),
                    )
                } else {
                    ui.pick(
                        format!("⚠️  系统音频振幅极低（max={:.6}），当前目标可能没有有效声音输出。", max_amp),
                        format!("⚠️  System-audio amplitude is very low (max={:.6}). The current target may not be producing audible output.", max_amp),
                    )
                }
            );
            if self.capture_mode == "microphone" {
                eprintln!(
                    "{}",
                    ui.pick(
                        "   请前往：系统设置 > 隐私与安全性 > 麦克风，将 Open Flow.app 添加到列表并启用。",
                        "   Open System Settings > Privacy & Security > Microphone, then add and enable Open Flow.app.",
                    )
                );
                eprintln!(
                    "{}",
                    ui.pick(
                        "   然后完全退出并重新打开 Open Flow。",
                        "   Then fully quit and reopen Open Flow."
                    )
                );
            } else {
                eprintln!(
                    "{}",
                    ui.pick(
                        "   你可以先在诊断页运行桌面音频 / 应用音频探测，确认 ScreenCaptureKit 回调是否正常。",
                        "   Try the desktop/application audio probes in Diagnostics first to confirm ScreenCaptureKit callbacks are working.",
                    )
                );
            }
            return Ok(());
        }

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
            let mut asr_summary = AsrExecutionSummary {
                result: TranscriptionResult {
                    text: String::new(),
                    confidence: 0.0,
                    language: None,
                    duration_ms: 0,
                },
                segmented: false,
                segment_count: 0,
            };
            if !tail.is_empty() {
                asr_summary = self
                    .transcribe_with_segments(provider.clone(), tail, sample_rate)
                    .await?;
                let tail_text = asr_summary.result.text.trim().to_string();

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
            let correction = self.maybe_correct_text(final_text).await;
            let resource_after_pipeline = self.sample_resource_snapshot();
            let transcription_id = self.transcription_count.fetch_add(1, Ordering::SeqCst) + 1;
            self.log_memory_checkpoint(
                "after_transcribe",
                Some(format!(
                    "transcription_id={} duration_ms={} sample_count={} tail_sample_count={} text_chars={}",
                    transcription_id,
                    transcribe_started_at.elapsed().as_millis(),
                    buffer.len(),
                    tail.len(),
                    correction.text.chars().count(),
                )),
            );
            println!(
                "[Pipeline] transcription_complete provider={} asr_duration_ms={} llm_duration_ms={} total_pipeline_ms={} llm_attempted={} llm_changed={} llm_status={} final_chars={}",
                self.provider.name(),
                asr_summary.result.duration_ms,
                correction.duration_ms,
                transcribe_started_at.elapsed().as_millis(),
                correction.attempted,
                correction.changed,
                correction.status,
                correction.text.chars().count()
            );

            let perf_entry = self.build_performance_entry(
                perf_session,
                "completed",
                duration.as_millis() as u64,
                sample_rate,
                buffer.len(),
                max_amp,
                rms,
                &asr_summary,
                &correction,
                resource_after_record_stop,
                self.sample_resource_snapshot(),
                resource_after_pipeline,
                correction.text.chars().count(),
                None,
            );

            tx.send(DaemonEvent::TranscriptionComplete(CompletedTranscription {
                text: correction.text,
                perf_entry,
            }))
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
                let error_string = e.to_string();
                if let Some(entry) = self.build_performance_entry(
                    perf_session,
                    "asr_failed",
                    duration.as_millis() as u64,
                    sample_rate,
                    buffer.len(),
                    max_amp,
                    rms,
                    &AsrExecutionSummary {
                        result: TranscriptionResult {
                            text: String::new(),
                            confidence: 0.0,
                            language: None,
                            duration_ms: transcribe_started_at.elapsed().as_millis() as u64,
                        },
                        segmented: false,
                        segment_count: 0,
                    },
                    &CorrectionOutcome {
                        text: String::new(),
                        attempted: false,
                        changed: false,
                        duration_ms: 0,
                        status: "skipped",
                    },
                    resource_after_record_stop,
                    self.sample_resource_snapshot(),
                    self.sample_resource_snapshot(),
                    0,
                    Some(error_string),
                ) {
                    self.emit_performance_log(entry);
                }
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
                result.result.text.chars().count(),
                result.result.duration_ms,
            )),
        );

        let resource_after_asr = self.sample_resource_snapshot();
        let asr_duration_ms = result.result.duration_ms;
        let raw_text = result.result.text.clone();
        let correction = self.maybe_correct_text(raw_text).await;
        let resource_after_pipeline = self.sample_resource_snapshot();
        println!(
            "[Pipeline] transcription_complete provider={} asr_duration_ms={} llm_duration_ms={} total_pipeline_ms={} llm_attempted={} llm_changed={} llm_status={} final_chars={}",
            self.provider.name(),
            asr_duration_ms,
            correction.duration_ms,
            transcribe_started_at.elapsed().as_millis(),
            correction.attempted,
            correction.changed,
            correction.status,
            correction.text.chars().count()
        );

        let perf_entry = self.build_performance_entry(
            perf_session,
            "completed",
            duration.as_millis() as u64,
            sample_rate,
            buffer.len(),
            max_amp,
            rms,
            &result,
            &correction,
            resource_after_record_stop,
            resource_after_asr,
            resource_after_pipeline,
            correction.text.chars().count(),
            None,
        );

        tx.send(DaemonEvent::TranscriptionComplete(CompletedTranscription {
            text: correction.text,
            perf_entry,
        }))
        .await
        .ok();

        Ok(())
    }

    async fn stop_and_transcribe_dual(
        &self,
        tx: &mpsc::Sender<DaemonEvent>,
        duration: std::time::Duration,
        perf_session: Option<ActivePerformanceSession>,
        dual_live_state: Option<DualMeetingLiveState>,
        microphone_buf: Option<Arc<Mutex<Vec<f32>>>>,
        system_audio_buf: Option<Arc<Mutex<Vec<f32>>>>,
    ) -> Result<()> {
        let ui = crate::common::config::Config::load()
            .map(|config| crate::common::ui::UiLanguage::from_config(&config))
            .unwrap_or_default();

        let live_state = dual_live_state.context("会议双路模式缺少实时会话状态")?;
        let microphone_buf = microphone_buf.context("会议双路模式缺少麦克风缓冲区")?;
        let system_audio_buf = system_audio_buf.context("会议双路模式缺少系统音频缓冲区")?;
        *self.dual_meeting_live_state.lock().unwrap() = None;

        for _ in 0..50 {
            if !live_state.microphone_inflight.load(Ordering::SeqCst)
                && !live_state.system_audio_inflight.load(Ordering::SeqCst)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let microphone_info = self
            .microphone_audio_info()
            .context("会议双路模式未找到麦克风音频信息")?;
        let system_audio_info = self
            .system_audio_info()
            .context("会议双路模式未找到系统音频信息")?;

        let microphone_tail = Self::collect_dual_source_tail(
            &microphone_buf,
            live_state.microphone_cursor.clone(),
            live_state.microphone_processed_samples.clone(),
            microphone_info.sample_rate,
        );
        let system_audio_tail = Self::collect_dual_source_tail(
            &system_audio_buf,
            live_state.system_audio_cursor.clone(),
            live_state.system_audio_processed_samples.clone(),
            system_audio_info.sample_rate,
        );
        let total_sample_count = live_state
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.sample_count)
            .sum::<usize>()
            + microphone_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0)
            + system_audio_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0);

        info!(
            "[Hotkey] 双路录音已停止，开始转写 (时长 {:.1}s, mic_samples={}, system_samples={})",
            duration.as_secs_f32(),
            microphone_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0),
            system_audio_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0)
        );
        self.log_memory_checkpoint(
            "after_recording_buffer_clone",
            Some(format!(
                "duration_ms={} sample_count={} microphone_tail_len={} system_audio_tail_len={}",
                duration.as_millis(),
                total_sample_count,
                microphone_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0),
                system_audio_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0),
            )),
        );
        println!(
            "{}",
            ui.pick(
                format!(
                    "⏹️  录音停止（双路会议模式，{:.1}s / 麦克风 {} 样本 / 系统音频 {} 样本），正在转写...",
                    duration.as_secs_f32(),
                    microphone_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0),
                    system_audio_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0)
                ),
                format!(
                    "⏹️  Recording stopped (dual meeting mode, {:.1}s / mic {} samples / system {} samples), transcribing...",
                    duration.as_secs_f32(),
                    microphone_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0),
                    system_audio_tail.as_ref().map(|tail| tail.0.len()).unwrap_or(0)
                ),
            )
        );

        let resource_after_record_stop = self.sample_resource_snapshot();
        let mut flushed_entries = Vec::<MeetingTranscriptEntry>::new();
        if let Some((buffer, started_at_ms, ended_at_ms)) = system_audio_tail {
            if let Some(entry) = Self::process_dual_source_segment(
                live_state.session_writer.clone(),
                live_state.entries.clone(),
                self.provider.clone(),
                buffer,
                "system_audio",
                "对方",
                system_audio_info.sample_rate,
                live_state.system_audio_segment_index.fetch_add(1, Ordering::SeqCst) + 1,
                started_at_ms,
                ended_at_ms,
            )
            .await?
            {
                flushed_entries.push(entry);
            }
        }
        if let Some((buffer, started_at_ms, ended_at_ms)) = microphone_tail {
            if let Some(entry) = Self::process_dual_source_segment(
                live_state.session_writer.clone(),
                live_state.entries.clone(),
                self.provider.clone(),
                buffer,
                "microphone",
                "我",
                microphone_info.sample_rate,
                live_state.microphone_segment_index.fetch_add(1, Ordering::SeqCst) + 1,
                started_at_ms,
                ended_at_ms,
            )
            .await?
            {
                flushed_entries.push(entry);
            }
        }

        let mut entries = live_state.entries.lock().unwrap().clone();
        entries.sort_by_key(|entry| (entry.started_at_ms, entry.segment_index));

        if entries.is_empty() {
            self.is_processing.store(false, Ordering::SeqCst);
            self.emit_performance_log(PerformanceLogEntry {
                schema_version: PERF_SCHEMA_VERSION,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                created_at_ms: now_unix_ms(),
                session_id: perf_session.map(|s| s.session_id).unwrap_or(0),
                status: "empty_dual_buffer".to_string(),
                provider: self.provider.name().to_string(),
                trigger_mode: self.trigger_mode.clone(),
                hotkey: self.hotkey.clone(),
                segmented: false,
                segment_count: 0,
                sample_rate: system_audio_info.sample_rate.max(microphone_info.sample_rate),
                sample_count: total_sample_count,
                recording_duration_ms: duration.as_millis() as u64,
                asr_duration_ms: 0,
                llm_duration_ms: 0,
                total_pipeline_ms: 0,
                output_duration_ms: 0,
                total_e2e_ms: perf_session
                    .map(|s| now_unix_ms().saturating_sub(s.started_at_ms))
                    .unwrap_or(0),
                llm_attempted: false,
                llm_changed: false,
                llm_status: "skipped".to_string(),
                text_chars: 0,
                confidence: 0.0,
                language: None,
                max_amplitude: 0.0,
                rms: 0.0,
                error: Some("both dual-source buffers were empty or too quiet".to_string()),
                resource_at_record_start: perf_session
                    .map(|s| s.resource_at_record_start)
                    .unwrap_or_else(empty_resource_snapshot),
                resource_after_record_stop,
                resource_after_asr: empty_resource_snapshot(),
                resource_after_pipeline: empty_resource_snapshot(),
            });
            eprintln!(
                "{}",
                ui.pick(
                    "⚠️  双路会议模式没有采到有效声音，请确认麦克风正常，并且桌面系统音频正在发声。",
                    "⚠️  Dual meeting mode did not capture usable audio. Check your microphone and make sure desktop system audio is actually playing.",
                )
            );
            return Ok(());
        }

        let merged_lines = entries
            .iter()
            .filter_map(|summary| {
                let text = summary.text.trim();
                if text.is_empty() {
                    None
                } else {
                    Some(format!("{}：{}", summary.role_label, text))
                }
            })
            .collect::<Vec<_>>();

        let merged_text = merged_lines.join("\n");
        let correction = CorrectionOutcome {
            text: merged_text.clone(),
            attempted: false,
            changed: false,
            duration_ms: 0,
            status: "skipped_dual_source",
        };
        let transcription_id = self.transcription_count.fetch_add(1, Ordering::SeqCst) + 1;
        let resource_after_asr = self.sample_resource_snapshot();
        let resource_after_pipeline = resource_after_asr;
        let asr_duration_ms = entries
            .iter()
            .map(|entry| entry.provider_duration_ms)
            .sum::<u64>();
        let segment_count = entries.len();
        let confidence_sum = entries
            .iter()
            .map(|entry| entry.confidence)
            .sum::<f32>();
        let confidence = confidence_sum / entries.len() as f32;
        let language = entries
            .iter()
            .find_map(|entry| entry.language.clone());
        let aggregated_summary = AsrExecutionSummary {
            result: TranscriptionResult {
                text: merged_text.clone(),
                confidence,
                language,
                duration_ms: asr_duration_ms,
            },
            segmented: true,
            segment_count,
        };

        self.log_memory_checkpoint(
            "after_transcribe",
            Some(format!(
                "transcription_id={} duration_ms={} sample_count={} text_chars={} mic_chars={} system_chars={}",
                transcription_id,
                asr_duration_ms,
                total_sample_count,
                correction.text.chars().count(),
                entries
                    .iter()
                    .filter(|entry| entry.source == "microphone")
                    .map(|entry| entry.text.chars().count())
                    .sum::<usize>(),
                entries
                    .iter()
                    .filter(|entry| entry.source == "system_audio")
                    .map(|entry| entry.text.chars().count())
                    .sum::<usize>(),
            )),
        );
        println!(
            "[Pipeline] transcription_complete provider={} mode=dual_source asr_duration_ms={} llm_duration_ms=0 total_pipeline_ms={} llm_attempted=false llm_changed=false llm_status=skipped_dual_source final_chars={}",
            self.provider.name(),
            asr_duration_ms,
            asr_duration_ms,
            correction.text.chars().count()
        );

        let perf_entry = self.build_performance_entry(
            perf_session,
            "completed",
            duration.as_millis() as u64,
            system_audio_info.sample_rate.max(microphone_info.sample_rate),
            total_sample_count,
            0.0,
            0.0,
            &aggregated_summary,
            &correction,
            resource_after_record_stop,
            resource_after_asr,
            resource_after_pipeline,
            correction.text.chars().count(),
            None,
        );

        tx.send(DaemonEvent::TranscriptionComplete(CompletedTranscription {
            text: correction.text,
            perf_entry,
        }))
        .await
        .ok();

        Ok(())
    }

    async fn transcribe_with_segments(
        &self,
        provider: Arc<dyn AsrProvider>,
        audio: &[f32],
        sample_rate: u32,
    ) -> Result<AsrExecutionSummary> {
        Self::transcribe_with_segments_static(provider, audio, sample_rate).await
    }

    async fn transcribe_with_segments_static(
        provider: Arc<dyn AsrProvider>,
        audio: &[f32],
        sample_rate: u32,
    ) -> Result<AsrExecutionSummary> {
        let provider_name = provider.name().to_string();
        let chunk_samples = (sample_rate as usize)
            .saturating_mul(TRANSCRIBE_SEGMENT_SECS as usize)
            .max(1);

        if audio.len() <= chunk_samples {
            println!(
                "[Pipeline] asr_start provider={} segmented=false sample_count={} sample_rate={} timeout_secs={}",
                provider_name,
                audio.len(),
                sample_rate,
                TRANSCRIBE_TIMEOUT_SECS
            );
            return match tokio::time::timeout(
                std::time::Duration::from_secs(TRANSCRIBE_TIMEOUT_SECS),
                provider.transcribe(audio, sample_rate),
            )
            .await
            {
                Ok(Ok(r)) => {
                    println!(
                        "[Pipeline] asr_complete provider={} segmented=false text_chars={} confidence={:.3} language={} provider_duration_ms={}",
                        provider_name,
                        r.text.chars().count(),
                        r.confidence,
                        r.language.as_deref().unwrap_or("unknown"),
                        r.duration_ms
                    );
                    Ok(AsrExecutionSummary {
                        result: r,
                        segmented: false,
                        segment_count: 1,
                    })
                }
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
            "[Pipeline] asr_start provider={} segmented=true total_segments={} sample_count={} sample_rate={} timeout_secs={}",
            provider_name,
            total_segments,
            audio.len(),
            sample_rate,
            TRANSCRIBE_TIMEOUT_SECS
        );
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
            println!(
                "[Pipeline] asr_segment_start provider={} segment={}/{} segment_sample_count={}",
                provider_name,
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

            println!(
                "[Pipeline] asr_segment_complete provider={} segment={}/{} text_chars={} confidence={:.3} language={} provider_duration_ms={}",
                provider_name,
                segment_no,
                total_segments,
                segment.text.chars().count(),
                segment.confidence,
                segment.language.as_deref().unwrap_or("unknown"),
                segment.duration_ms
            );

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

        let merged = TranscriptionResult {
            text: merged_texts.join(" "),
            confidence: if confidence_count > 0 {
                confidence_sum / confidence_count as f32
            } else {
                0.0
            },
            language,
            duration_ms: started_at.elapsed().as_millis() as u64,
        };

        println!(
            "[Pipeline] asr_complete provider={} segmented=true total_segments={} text_chars={} confidence={:.3} language={} provider_duration_ms={}",
            provider_name,
            total_segments,
            merged.text.chars().count(),
            merged.confidence,
            merged.language.as_deref().unwrap_or("unknown"),
            merged.duration_ms
        );

        Ok(AsrExecutionSummary {
            result: merged,
            segmented: true,
            segment_count: total_segments,
        })
    }

    fn collect_dual_source_tail(
        buffer_arc: &Arc<Mutex<Vec<f32>>>,
        cursor: Arc<AtomicUsize>,
        processed_samples: Arc<AtomicUsize>,
        sample_rate: u32,
    ) -> Option<(Vec<f32>, u64, u64)> {
        let cursor_value = cursor.load(Ordering::SeqCst);
        let processed = processed_samples.load(Ordering::SeqCst);
        let mut guard = buffer_arc.lock().unwrap();
        if cursor_value >= guard.len() {
            guard.clear();
            cursor.store(0, Ordering::SeqCst);
            return None;
        }

        let tail = guard[cursor_value..].to_vec();
        guard.clear();
        cursor.store(0, Ordering::SeqCst);
        if tail.is_empty() {
            return None;
        }

        let started_sample = processed.saturating_add(cursor_value);
        let ended_sample = started_sample.saturating_add(tail.len());
        Some((
            tail,
            started_sample as u64 * 1000 / sample_rate as u64,
            ended_sample as u64 * 1000 / sample_rate as u64,
        ))
    }

    async fn process_dual_source_segment(
        session_writer: MeetingSessionWriter,
        entries: Arc<Mutex<Vec<MeetingTranscriptEntry>>>,
        provider: Arc<dyn AsrProvider>,
        buffer: Vec<f32>,
        source: &'static str,
        role_label: &'static str,
        sample_rate: u32,
        segment_index: u64,
        started_at_ms: u64,
        ended_at_ms: u64,
    ) -> Result<Option<MeetingTranscriptEntry>> {
        if buffer.is_empty() {
            println!("[Pipeline] dual_source_skip source={} reason=empty_buffer", source);
            return Ok(None);
        }

        let max_amplitude = buffer.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let rms = (buffer.iter().map(|x| x * x).sum::<f32>() / buffer.len() as f32).sqrt();
        println!(
            "[Pipeline] dual_source_prepare source={} segment_index={} sample_count={} sample_rate={} max_amp={:.6} rms={:.6}",
            source,
            segment_index,
            buffer.len(),
            sample_rate,
            max_amplitude,
            rms
        );

        if max_amplitude < 0.001 {
            println!(
                "[Pipeline] dual_source_skip source={} reason=low_amplitude max_amp={:.6}",
                source, max_amplitude
            );
            return Ok(None);
        }

        let asr = Self::transcribe_with_segments_static(provider, &buffer, sample_rate).await?;
        let trimmed_text = asr.result.text.trim().to_string();
        if trimmed_text.is_empty() {
            println!(
                "[Pipeline] dual_source_skip source={} reason=empty_transcript segment_index={}",
                source, segment_index
            );
            return Ok(None);
        }

        let wav_path = session_writer
            .save_segment_wav(source, segment_index, sample_rate, &buffer)?
            .display()
            .to_string();
        let entry = MeetingTranscriptEntry {
            session_id: session_writer.session_id(),
            segment_index,
            source: source.to_string(),
            role_label: role_label.to_string(),
            started_at_ms,
            ended_at_ms,
            sample_rate,
            sample_count: buffer.len(),
            text: trimmed_text,
            confidence: asr.result.confidence,
            language: asr.result.language.clone(),
            provider_duration_ms: asr.result.duration_ms,
            wav_path,
            created_at_ms: now_unix_ms(),
        };
        let mut guard = entries.lock().unwrap();
        session_writer.append_entry(&entry)?;
        guard.push(entry.clone());
        Ok(Some(entry))
    }

    async fn maybe_correct_text(&self, text: String) -> CorrectionOutcome {
        let Some(corrector) = &self.text_corrector else {
            let reason = if !self.correction_config_enabled {
                "disabled_in_config"
            } else if !self.correction_api_key_configured {
                "missing_api_key"
            } else {
                "runtime_unavailable"
            };
            println!(
                "[Pipeline] llm_correction_skip reason={} correction_config_enabled={} correction_api_key_configured={} correction_model={} vocabulary_terms={} raw_chars={}",
                reason,
                self.correction_config_enabled,
                self.correction_api_key_configured,
                self.correction_model_name,
                self.correction_vocab_count,
                text.chars().count()
            );
            return CorrectionOutcome {
                text,
                attempted: false,
                changed: false,
                duration_ms: 0,
                status: reason,
            };
        };

        let started_at = std::time::Instant::now();
        match corrector.correct(&text).await {
            Ok(corrected) if !corrected.trim().is_empty() => CorrectionOutcome {
                changed: corrected != text,
                text: corrected,
                attempted: true,
                duration_ms: started_at.elapsed().as_millis() as u64,
                status: "completed",
            },
            Ok(_) => {
                println!(
                    "[Pipeline] llm_correction_complete model={} changed=false raw_chars={} corrected_chars={} vocab_count={} duration_ms={}",
                    corrector.model_name(),
                    text.chars().count(),
                    text.chars().count(),
                    corrector.vocabulary_count(),
                    started_at.elapsed().as_millis()
                );
                CorrectionOutcome {
                    text,
                    attempted: true,
                    changed: false,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    status: "empty_result_fallback",
                }
            }
            Err(err) => {
                eprintln!(
                    "[Pipeline] llm_correction_failed model={} raw_chars={} vocab_count={} duration_ms={} error={}",
                    corrector.model_name(),
                    text.chars().count(),
                    corrector.vocabulary_count(),
                    started_at.elapsed().as_millis(),
                    err
                );
                eprintln!("⚠️  文本纠错失败，已回退原始转写: {}", err);
                CorrectionOutcome {
                    text,
                    attempted: true,
                    changed: false,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    status: "failed_fallback",
                }
            }
        }
    }

    fn sample_resource_snapshot(&self) -> ProcessResourceSnapshot {
        sample_process_resources().unwrap_or_else(empty_resource_snapshot)
    }

    fn build_performance_entry(
        &self,
        perf_session: Option<ActivePerformanceSession>,
        status: &str,
        recording_duration_ms: u64,
        sample_rate: u32,
        sample_count: usize,
        max_amplitude: f32,
        rms: f32,
        asr_summary: &AsrExecutionSummary,
        correction: &CorrectionOutcome,
        resource_after_record_stop: ProcessResourceSnapshot,
        resource_after_asr: ProcessResourceSnapshot,
        resource_after_pipeline: ProcessResourceSnapshot,
        text_chars: usize,
        error: Option<String>,
    ) -> Option<PerformanceLogEntry> {
        if !self.performance_log_enabled {
            return None;
        }

        let now_ms = now_unix_ms();
        let started_at_ms = perf_session.map(|s| s.started_at_ms).unwrap_or(now_ms);
        Some(PerformanceLogEntry {
            schema_version: PERF_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at_ms: now_ms,
            session_id: perf_session.map(|s| s.session_id).unwrap_or(0),
            status: status.to_string(),
            provider: self.provider.name().to_string(),
            trigger_mode: self.trigger_mode.clone(),
            hotkey: self.hotkey.clone(),
            segmented: asr_summary.segmented,
            segment_count: asr_summary.segment_count,
            sample_rate,
            sample_count,
            recording_duration_ms,
            asr_duration_ms: asr_summary.result.duration_ms,
            llm_duration_ms: correction.duration_ms,
            total_pipeline_ms: asr_summary
                .result
                .duration_ms
                .saturating_add(correction.duration_ms),
            output_duration_ms: 0,
            total_e2e_ms: now_ms.saturating_sub(started_at_ms),
            llm_attempted: correction.attempted,
            llm_changed: correction.changed,
            llm_status: correction.status.to_string(),
            text_chars,
            confidence: asr_summary.result.confidence,
            language: asr_summary.result.language.clone(),
            max_amplitude,
            rms,
            error,
            resource_at_record_start: perf_session
                .map(|s| s.resource_at_record_start)
                .unwrap_or_else(empty_resource_snapshot),
            resource_after_record_stop,
            resource_after_asr,
            resource_after_pipeline,
        })
    }

    fn emit_performance_log(&self, entry: PerformanceLogEntry) {
        self.write_performance_entry(&entry);
    }

    fn write_performance_entry(&self, entry: &PerformanceLogEntry) {
        if let Err(err) = self.performance_logger.write_entry(entry) {
            eprintln!(
                "[Performance] failed_to_persist session_id={} error={}",
                entry.session_id, err
            );
        } else {
            println!(
                "[Performance] persisted session_id={} status={} total_e2e_ms={} file_dir={}",
                entry.session_id,
                entry.status,
                entry.total_e2e_ms,
                self.performance_logger.directory().display()
            );
        }
    }

    fn microphone_audio_info(&self) -> Option<crate::audio::AudioInfo> {
        self.audio_capture.as_ref().map(|audio_capture| audio_capture.get_info())
    }

    fn system_audio_info(&self) -> Option<crate::audio::AudioInfo> {
        if !self.uses_system_audio_capture() {
            return None;
        }

        let mut config = crate::common::config::Config::load().unwrap_or_default();
        config.capture_mode = match self.capture_mode.as_str() {
            "system_audio_application" => "system_audio_application".to_string(),
            _ => "system_audio_desktop".to_string(),
        };
        config.system_audio_target_pid = self.system_audio_target_pid.clone();
        config.system_audio_target_name = self.system_audio_target_name.clone();
        Some(SystemAudioCapture::info_from_config(&config))
    }

    fn current_audio_info(&self) -> crate::audio::AudioInfo {
        if self.is_dual_capture_mode() {
            return crate::audio::AudioInfo {
                device_name: "Desktop Audio + Microphone".to_string(),
                sample_rate: self
                    .system_audio_info()
                    .map(|info| info.sample_rate)
                    .or_else(|| self.microphone_audio_info().map(|info| info.sample_rate))
                    .unwrap_or(48_000),
                channels: 1,
                sample_format: "F32".to_string(),
            };
        }

        if let Some(audio_capture) = &self.audio_capture {
            audio_capture.get_info()
        } else {
            self.system_audio_info().unwrap_or(crate::audio::AudioInfo {
                device_name: "Unknown Audio".to_string(),
                sample_rate: 48_000,
                channels: 1,
                sample_format: "F32".to_string(),
            })
        }
    }

    fn log_memory_checkpoint(&self, checkpoint: &str, extra: Option<String>) {
        let state = self.state.lock().unwrap();
        let buffer_info = if self.is_dual_capture_mode() {
            let microphone_info = self
                .microphone_recording_buffer
                .lock()
                .unwrap()
                .clone()
                .map(|buffer_arc| {
                    let buffer = buffer_arc.lock().unwrap();
                    (buffer.len(), buffer.capacity())
                })
                .unwrap_or((0, 0));
            let system_audio_info = self
                .system_audio_recording_buffer
                .lock()
                .unwrap()
                .clone()
                .map(|buffer_arc| {
                    let buffer = buffer_arc.lock().unwrap();
                    (buffer.len(), buffer.capacity())
                })
                .unwrap_or((0, 0));
            (
                microphone_info.0 + system_audio_info.0,
                microphone_info.1 + system_audio_info.1,
            )
        } else {
            let buffer_arc = self.recording_buffer.lock().unwrap().clone();
            let buffer = buffer_arc.lock().unwrap();
            (buffer.len(), buffer.capacity())
        };
        let active_stream = self.active_stream.lock().unwrap().is_some()
            || self.active_system_audio.lock().unwrap().is_some();
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
