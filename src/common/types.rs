use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RecordingState {
    pub is_recording: bool,
    pub start_time: Option<std::time::Instant>,
    pub audio_buffer: Vec<f32>,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            is_recording: false,
            start_time: None,
            audio_buffer: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub confidence: f32,
    pub language: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    /// Toggle recording on/off with hotkey
    Toggle,
    /// Hold to record, release to stop
    Hold,
}
