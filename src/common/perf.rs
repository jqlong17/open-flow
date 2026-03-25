use crate::common::config::Config;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PERF_SCHEMA_VERSION: u32 = 1;
const PERF_RETENTION_DAYS: u64 = 14;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProcessResourceSnapshot {
    pub cpu_percent: Option<f32>,
    pub rss_bytes: Option<u64>,
    pub vsz_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceLogEntry {
    pub schema_version: u32,
    pub app_version: String,
    pub created_at_ms: u64,
    pub session_id: u64,
    pub status: String,
    pub provider: String,
    pub trigger_mode: String,
    pub hotkey: String,
    pub segmented: bool,
    pub segment_count: usize,
    pub sample_rate: u32,
    pub sample_count: usize,
    pub recording_duration_ms: u64,
    pub asr_duration_ms: u64,
    pub llm_duration_ms: u64,
    pub total_pipeline_ms: u64,
    pub output_duration_ms: u64,
    pub total_e2e_ms: u64,
    pub llm_attempted: bool,
    pub llm_changed: bool,
    pub llm_status: String,
    pub text_chars: usize,
    pub confidence: f32,
    pub language: Option<String>,
    pub max_amplitude: f32,
    pub rms: f32,
    pub error: Option<String>,
    pub resource_at_record_start: ProcessResourceSnapshot,
    pub resource_after_record_stop: ProcessResourceSnapshot,
    pub resource_after_asr: ProcessResourceSnapshot,
    pub resource_after_pipeline: ProcessResourceSnapshot,
}

#[derive(Debug, Clone)]
pub struct PerformanceLogWriter {
    enabled: bool,
    dir: PathBuf,
}

impl PerformanceLogWriter {
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let dir = Config::data_dir()?.join("performance");
        if config.performance_log_enabled() {
            fs::create_dir_all(&dir)?;
        }
        Ok(Self {
            enabled: config.performance_log_enabled(),
            dir,
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn write_entry(&self, entry: &PerformanceLogEntry) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        fs::create_dir_all(&self.dir)?;
        self.cleanup_old_logs()?;

        let log_path = self.log_file_path();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        serde_json::to_writer(&mut file, entry)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    fn log_file_path(&self) -> PathBuf {
        let day_index = now_unix_ms() / 86_400_000;
        self.dir.join(format!("perf-{}.jsonl", day_index))
    }

    fn cleanup_old_logs(&self) -> anyhow::Result<()> {
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(PERF_RETENTION_DAYS * 24 * 60 * 60))
            .unwrap_or(UNIX_EPOCH);

        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::now());
            if modified < cutoff {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}

pub fn sample_process_resources() -> Option<ProcessResourceSnapshot> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "%cpu=", "-o", "rss=", "-o", "vsz=", "-p", &pid])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut parts = stdout.split_whitespace();
    let cpu_percent = parts.next().and_then(|v| v.parse::<f32>().ok());
    let rss_bytes = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    let vsz_bytes = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));

    Some(ProcessResourceSnapshot {
        cpu_percent,
        rss_bytes,
        vsz_bytes,
    })
}

pub fn empty_resource_snapshot() -> ProcessResourceSnapshot {
    ProcessResourceSnapshot {
        cpu_percent: None,
        rss_bytes: None,
        vsz_bytes: None,
    }
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
