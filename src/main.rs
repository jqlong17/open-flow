#![allow(unexpected_cfgs)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

// 公共模块来自 lib crate（open_flow）
use open_flow::asr;
use open_flow::audio;
use open_flow::common;
use open_flow::draft_panel;
use open_flow::hotkey;
use open_flow::overlay;
use open_flow::text_injection;
use open_flow::tray;

mod cli;
mod daemon;

use cli::commands;

fn is_app_bundle_launch() -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        return false; // 仅 macOS 有 .app 包，Windows/Linux 始终走 CLI
    }

    #[cfg(target_os = "macos")]
    {
        if std::env::args_os().nth(1).is_some() {
            return false;
        }
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.to_str().map(|s| s.contains(".app/Contents/MacOS/")))
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
fn is_windows_direct_launch() -> bool {
    std::env::args_os().nth(1).is_none()
}

#[cfg(not(target_os = "windows"))]
fn is_windows_direct_launch() -> bool {
    false
}

fn log_launch_context(app_bundle_launch: bool) {
    let current_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let args: Vec<String> = std::env::args().collect();

    info!(
        "Launch context: app_bundle_launch={} current_exe={} args={:?}",
        app_bundle_launch, current_exe, args
    );
}

#[cfg(unix)]
fn redirect_app_bundle_stdio_to_log() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    let Ok(data_dir) = crate::common::config::Config::data_dir() else {
        return;
    };
    let log_path = data_dir.join("daemon.log");
    let Ok(file) = OpenOptions::new().create(true).append(true).open(log_path) else {
        return;
    };

    unsafe {
        let fd = file.as_raw_fd();
        let _ = libc::dup2(fd, libc::STDOUT_FILENO);
        let _ = libc::dup2(fd, libc::STDERR_FILENO);
    }

    // 保持文件句柄存活到进程结束，避免 stdout/stderr 指向已关闭 fd。
    std::mem::forget(file);
}

#[derive(Parser)]
#[command(name = "open-flow")]
#[command(about = "AI coding voice input for macOS")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the voice input daemon (默认后台运行；--foreground 时占用终端)
    Start {
        /// Path to SenseVoice model directory
        #[arg(short, long)]
        model: Option<PathBuf>,

        /// Run in foreground (keep terminal open for logs; default: background)
        #[arg(long)]
        foreground: bool,
    },

    /// Stop the daemon (停止守护进程)
    Stop,

    /// Check daemon status (查看状态)
    Status,

    /// One-shot transcription (单次录音转写)
    Transcribe {
        /// Use an existing audio file instead of recording
        #[arg(long)]
        file: Option<PathBuf>,

        /// Duration in seconds (0 = toggle mode)
        #[arg(short, long, default_value = "0")]
        duration: u64,

        /// Override model directory (default: from config)
        #[arg(short, long)]
        model: Option<PathBuf>,
    },

    /// Test audio recording (测试录音)
    TestRecord {
        /// Recording duration in seconds
        #[arg(short, long, default_value = "5")]
        duration: u64,
    },

    /// Simulate Command key in loop for hotkey/recording test (需另终端先运行 open-flow start)
    TestHotkey {
        /// Number of cycles: press=start, wait, press=stop, wait
        #[arg(short, long, default_value = "3")]
        cycles: u32,
        /// Seconds to "record" per cycle before simulating stop
        #[arg(short, long, default_value = "3")]
        record_secs: u64,
        /// Seconds to wait after stop for transcription to finish
        #[arg(short, long, default_value = "12")]
        transcribe_wait_secs: u64,
        /// Seconds to wait at start for daemon to be ready
        #[arg(long, default_value = "8")]
        ready_wait_secs: u64,
    },

    /// Switch or list model preset (quantized 默认 | fp16)，缺的模型会自动下载
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },

    /// Print permission status for the current Open Flow binary
    #[command(hide = true)]
    Permissions {
        /// Emit machine-readable JSON for the settings app
        #[arg(long)]
        json: bool,
    },

    /// List audio input devices for the current machine
    #[command(hide = true)]
    AudioDevices {
        /// Emit machine-readable JSON for the settings app
        #[arg(long)]
        json: bool,
    },

    /// Manually download the ASR model (手动下载模型，首次运行会自动触发无需手动执行)
    #[command(hide = true)]
    Setup {
        /// Custom model installation directory (default: app data dir)
        #[arg(short, long)]
        model_dir: Option<PathBuf>,

        /// Force re-download even if files already exist
        #[arg(short, long)]
        force: bool,
    },

    /// Print a support bundle for remote troubleshooting
    #[command(hide = true)]
    Support {
        /// Number of log lines to include from the tail of daemon.log
        #[arg(long, default_value = "200")]
        tail_lines: usize,
    },
}

#[derive(Subcommand)]
enum ModelCommand {
    /// 切换到指定预设；若该预设对应目录无模型则自动下载
    Use {
        /// 预设: quantized（默认）| fp16
        preset: String,
        /// 切换后强制检查/下载一次
        #[arg(long)]
        download: bool,
    },
    /// 列出当前预设与可用预设
    List,
}

/// `open-flow start` 默认后台；`--foreground` 时走前台路径（主线程保留给 macOS 托盘/NSRunLoop）
fn main() -> anyhow::Result<()> {
    // 若为后台子进程，先 detach 再初始化 tracing
    if std::env::var_os("OPEN_FLOW_DAEMON").is_some() {
        #[cfg(unix)]
        {
            let _ = unsafe { libc::setsid() };
        }
        std::env::remove_var("OPEN_FLOW_DAEMON");
    }

    let app_bundle_launch = is_app_bundle_launch();

    if app_bundle_launch {
        #[cfg(unix)]
        redirect_app_bundle_stdio_to_log();
    }

    let tracing_log_path = common::logging::init_tracing("open-flow")?;
    log_launch_context(app_bundle_launch);
    info!("Tracing log file: {}", tracing_log_path.display());

    // Finder / Dock 双击启动 .app 时不带子命令，直接进入前台模式。
    // 这样 app bundle 的主可执行文件就是实际运行的进程，避免权限身份漂移。
    if app_bundle_launch {
        info!("Starting Open Flow from app bundle (foreground mode)...");
        return cli::daemon::start_foreground(None);
    }

    if is_windows_direct_launch() {
        info!("Starting Open Flow from direct Windows launch (foreground mode)...");
        return cli::daemon::start_foreground(None);
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { model, foreground } => {
            if foreground {
                info!("Starting Open Flow (foreground mode)...");
                cli::daemon::start_foreground(model)
            } else {
                cli::daemon::start_background(model)
            }
        }
        other => {
            // 其他命令用 tokio 运行时
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(async_main(other))
        }
    }
}

async fn async_main(cmd: Commands) -> anyhow::Result<()> {
    match cmd {
        Commands::Start { .. } => unreachable!(),
        Commands::Stop => {
            info!("Stopping Open Flow daemon...");
            cli::daemon::stop().await?;
        }
        Commands::Status => {
            cli::daemon::status().await?;
        }
        Commands::Transcribe {
            file,
            duration,
            model,
        } => {
            commands::transcribe::run(file, duration, model).await?;
        }
        Commands::TestRecord { duration } => {
            commands::test_record::test_record(duration).await?;
        }
        Commands::TestHotkey {
            cycles,
            record_secs,
            transcribe_wait_secs,
            ready_wait_secs,
        } => {
            commands::test_hotkey::run_test_hotkey(
                cycles,
                record_secs,
                transcribe_wait_secs,
                ready_wait_secs,
            )
            .await?;
        }
        Commands::Model { command } => match command {
            ModelCommand::Use { preset, download } => {
                let p = preset
                    .parse()
                    .map_err(|e: String| anyhow::anyhow!("{}", e))?;
                commands::model::use_preset(p, download).await?;
            }
            ModelCommand::List => {
                commands::model::list()?;
            }
        },
        Commands::Permissions { json } => {
            cli::daemon::permissions(json).await?;
        }
        Commands::AudioDevices { json } => {
            commands::audio_devices::run(json).await?;
        }
        Commands::Setup { model_dir, force } => {
            commands::setup::run(model_dir, force).await?;
        }
        Commands::Support { tail_lines } => {
            commands::support::run(tail_lines).await?;
        }
    }

    Ok(())
}
