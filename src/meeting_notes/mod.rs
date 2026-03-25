use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::audio::save_buffer_to_wav_with_spec;
use crate::common::config::Config;
use crate::common::perf::now_unix_ms;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSessionManifest {
    pub session_id: u64,
    pub created_at_ms: u64,
    pub app_version: String,
    pub capture_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTranscriptEntry {
    pub session_id: u64,
    pub segment_index: u64,
    pub source: String,
    pub role_label: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub sample_rate: u32,
    pub sample_count: usize,
    pub text: String,
    pub confidence: f32,
    pub language: Option<String>,
    pub provider_duration_ms: u64,
    pub wav_path: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct MeetingSessionWriter {
    session_id: u64,
    dir: PathBuf,
    segments_dir: PathBuf,
    transcripts_path: PathBuf,
    merged_markdown_path: PathBuf,
}

impl MeetingSessionWriter {
    pub fn create(session_id: u64, capture_mode: &str) -> Result<Self> {
        let created_at_ms = now_unix_ms();
        let dir = Config::data_dir()?.join("meeting-sessions").join(format!(
            "session-{}-{}",
            created_at_ms, session_id
        ));
        let segments_dir = dir.join("segments");
        fs::create_dir_all(&segments_dir)?;

        let writer = Self {
            session_id,
            transcripts_path: dir.join("transcripts.jsonl"),
            merged_markdown_path: dir.join("merged_transcript.md"),
            dir,
            segments_dir,
        };

        let manifest = MeetingSessionManifest {
            session_id,
            created_at_ms,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: capture_mode.to_string(),
        };
        fs::write(
            writer.dir.join("session.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        fs::write(
            &writer.merged_markdown_path,
            format!(
                "# Meeting Session {}\n\n- Created At: {}\n- Capture Mode: {}\n\n",
                session_id, created_at_ms, capture_mode
            ),
        )?;

        Ok(writer)
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    pub fn save_segment_wav(
        &self,
        source: &str,
        segment_index: u64,
        sample_rate: u32,
        buffer: &[f32],
    ) -> Result<PathBuf> {
        let filename = format!("{source}-{segment_index:04}.wav");
        let path = self.segments_dir.join(filename);
        save_buffer_to_wav_with_spec(buffer, sample_rate, 1, &path)?;
        Ok(path)
    }

    pub fn append_entry(&self, entry: &MeetingTranscriptEntry) -> Result<()> {
        let mut transcripts = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.transcripts_path)?;
        serde_json::to_writer(&mut transcripts, entry)?;
        transcripts.write_all(b"\n")?;
        transcripts.flush()?;

        let started = format_offset_ms(entry.started_at_ms);
        let ended = format_offset_ms(entry.ended_at_ms);
        let markdown_line = format!(
            "[{} - {}] {}：{}\n",
            started, ended, entry.role_label, entry.text
        );
        let mut merged = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.merged_markdown_path)?;
        merged.write_all(markdown_line.as_bytes())?;
        merged.flush()?;
        Ok(())
    }
}

fn format_offset_ms(offset_ms: u64) -> String {
    let total_seconds = offset_ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}
