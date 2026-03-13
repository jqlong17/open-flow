use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct RecordingState {
    pub is_recording: bool,
    pub start_time: Option<std::time::Instant>,
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
}
