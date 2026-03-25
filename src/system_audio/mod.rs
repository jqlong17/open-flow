use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread::JoinHandle;

use crate::audio::AudioInfo;
use crate::common::config::Config;

const SYSTEM_AUDIO_SAMPLE_RATE: u32 = 48_000;
const SYSTEM_AUDIO_CHANNELS: u16 = 1;

pub struct SystemAudioCapture {
    child: Child,
    reader_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl SystemAudioCapture {
    pub fn spawn_from_config(config: &Config, buffer: Arc<Mutex<Vec<f32>>>) -> Result<Self> {
        let helper = helper_binary_path()?;
        let capture_mode = config.resolved_capture_mode();

        let mut command = Command::new(&helper);
        match capture_mode.as_str() {
            "system_audio_desktop" => {
                command.arg("stream-desktop");
            }
            "system_audio_application" => {
                let pid = config.system_audio_target_pid.trim();
                if pid.is_empty() {
                    anyhow::bail!("系统音频模式为应用音频，但未配置目标应用 PID");
                }
                command.arg("stream-application").arg("--pid").arg(pid);
            }
            other => {
                anyhow::bail!("不支持的系统音频模式: {}", other);
            }
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("启动系统音频 helper 失败: {}", helper.display()))?;

        let stdout = child
            .stdout
            .take()
            .context("系统音频 helper 未提供 stdout 管道")?;
        let stderr = child
            .stderr
            .take()
            .context("系统音频 helper 未提供 stderr 管道")?;

        let reader_thread = Some(spawn_stdout_reader(stdout, buffer));
        let stderr_thread = Some(spawn_stderr_reader(stderr));

        std::thread::sleep(Duration::from_millis(350));
        if let Some(status) = child
            .try_wait()
            .context("检查系统音频 helper 启动状态失败")?
        {
            anyhow::bail!("系统音频 helper 启动后立即退出，状态码: {}", status);
        }

        Ok(Self {
            child,
            reader_thread,
            stderr_thread,
        })
    }

    pub fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn helper_available() -> bool {
        helper_binary_path().is_ok()
    }

    pub fn info_from_config(config: &Config) -> AudioInfo {
        let device_name = match config.resolved_capture_mode().as_str() {
            "system_audio_application" => {
                let target_name = config.system_audio_target_name.trim();
                if target_name.is_empty() {
                    "System Audio (Application)".to_string()
                } else {
                    format!("System Audio ({})", target_name)
                }
            }
            _ => "System Audio (Desktop)".to_string(),
        };

        AudioInfo {
            device_name,
            sample_rate: SYSTEM_AUDIO_SAMPLE_RATE,
            channels: SYSTEM_AUDIO_CHANNELS,
            sample_format: "F32".to_string(),
        }
    }
}

fn spawn_stdout_reader(stdout: ChildStdout, buffer: Arc<Mutex<Vec<f32>>>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut read_buf = [0u8; 16 * 1024];
        let mut pending = Vec::<u8>::new();

        loop {
            match reader.read(&mut read_buf) {
                Ok(0) => break,
                Ok(n) => {
                    pending.extend_from_slice(&read_buf[..n]);
                    let complete_len = pending.len() / 4 * 4;
                    if complete_len == 0 {
                        continue;
                    }

                    let mut samples = Vec::with_capacity(complete_len / 4);
                    for chunk in pending[..complete_len].chunks_exact(4) {
                        samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                    }

                    if let Ok(mut guard) = buffer.lock() {
                        guard.extend_from_slice(&samples);
                    }

                    pending.drain(..complete_len);
                }
                Err(err) => {
                    eprintln!("[SystemAudio] stdout read error: {}", err);
                    break;
                }
            }
        }
    })
}

fn spawn_stderr_reader(stderr: ChildStderr) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = buf.trim();
                    if !trimmed.is_empty() {
                        eprintln!("[SystemAudioHelper] {}", trimmed);
                    }
                }
                Err(err) => {
                    eprintln!("[SystemAudio] stderr read error: {}", err);
                    break;
                }
            }
        }
    })
}

fn helper_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("无法获取当前可执行文件路径")?;
    if let Some(parent) = current_exe.parent() {
        let bundled = parent.join("OpenFlowSystemAudioHelper");
        if is_executable(&bundled) {
            return Ok(bundled);
        }
    }

    let mut candidates = Vec::new();
    if let Some(repo_root) = detect_repo_root(&current_exe) {
        candidates.push(
            repo_root.join("settings-app/.build/arm64-apple-macosx/release/OpenFlowSystemAudioHelper"),
        );
        candidates.push(
            repo_root.join("settings-app/.build/x86_64-apple-macosx/release/OpenFlowSystemAudioHelper"),
        );
        candidates.push(repo_root.join("settings-app/.build/release/OpenFlowSystemAudioHelper"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("settings-app/.build/arm64-apple-macosx/release/OpenFlowSystemAudioHelper"));
        candidates.push(cwd.join("settings-app/.build/x86_64-apple-macosx/release/OpenFlowSystemAudioHelper"));
        candidates.push(cwd.join("settings-app/.build/release/OpenFlowSystemAudioHelper"));
    }

    for candidate in candidates {
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    anyhow::bail!("未找到 OpenFlowSystemAudioHelper，可先运行 settings-app 构建或 app 打包")
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

fn detect_repo_root(current_exe: &Path) -> Option<PathBuf> {
    current_exe
        .ancestors()
        .find(|path| path.join("Cargo.toml").exists() && path.join("settings-app").is_dir())
        .map(|path| path.to_path_buf())
}
