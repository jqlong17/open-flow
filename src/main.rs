use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info};

mod asr;
mod audio;
mod cli;
mod common;
mod daemon;
mod hotkey;
mod text_injection;

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
    /// Start the voice input daemon (后台守护进程)
    Start {
        /// Path to SenseVoice model directory
        #[arg(short, long)]
        model: Option<PathBuf>,
        
        /// Hotkey configuration (default: right-command)
        #[arg(short, long, default_value = "right-command")]
        hotkey: String,
    },
    
    /// Stop the daemon (停止守护进程)
    Stop,
    
    /// Check daemon status (查看状态)
    Status,
    
    /// Configure settings (配置设置)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    
    /// One-shot transcription (单次录音转写)
    Transcribe {
        /// Output mode: stdout, clipboard, paste
        #[arg(short, long, default_value = "paste")]
        output: String,
        
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
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set model path (设置模型路径)
    SetModel { path: PathBuf },
    
    /// Set hotkey (设置热键)
    SetHotkey { key: String },
    
    /// Show current configuration (显示当前配置)
    Show,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Start { model, hotkey } => {
            info!("Starting Open Flow daemon...");
            cli::daemon::start(model, hotkey).await?;
        }
        Commands::Stop => {
            info!("Stopping Open Flow daemon...");
            cli::daemon::stop().await?;
        }
        Commands::Status => {
            cli::daemon::status().await?;
        }
        Commands::Config { action } => {
            match action {
                ConfigAction::SetModel { path } => {
                    commands::config::set_model(path).await?;
                }
                ConfigAction::SetHotkey { key } => {
                    commands::config::set_hotkey(key).await?;
                }
                ConfigAction::Show => {
                    commands::config::show().await?;
                }
            }
        }
        Commands::Transcribe { output, file, duration, model } => {
            commands::transcribe::run(output, file, duration, model).await?;
        }
        Commands::TestRecord { duration } => {
            commands::test_record::test_record(duration).await?;
        }
    }
    
    Ok(())
}
