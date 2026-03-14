#![allow(unexpected_cfgs)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

mod asr;
mod audio;
mod cli;
mod common;
mod daemon;
mod hotkey;
mod text_injection;
mod tray;

use cli::commands;
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

    tracing_subscriber::fmt::init();

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
        Commands::Transcribe { file, duration, model } => {
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
        Commands::Setup { model_dir, force } => {
            commands::setup::run(model_dir, force).await?;
        }
    }

    Ok(())
}
