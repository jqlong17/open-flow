use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use open_flow::asr::AsrEngine;
use open_flow::common::config::ModelPreset;
use open_flow::model_store;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ofasr")]
#[command(about = "Standalone speech-to-text CLI built from Open Flow ASR")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Transcribe a local audio file
    Transcribe {
        /// Path to wav audio file
        #[arg(short, long)]
        file: PathBuf,

        /// Override model directory
        #[arg(short, long)]
        model: Option<PathBuf>,

        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },

    /// Download or refresh model files
    Setup {
        /// Model preset to download
        #[arg(long, value_enum, default_value_t = PresetArg::Quantized)]
        preset: PresetArg,

        /// Custom installation directory
        #[arg(long)]
        model_dir: Option<PathBuf>,

        /// Re-download existing files
        #[arg(long)]
        force: bool,
    },

    /// Validate whether the model directory is usable
    Check {
        /// Override model directory
        #[arg(short, long)]
        model: Option<PathBuf>,

        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum PresetArg {
    Quantized,
    Fp16,
}

impl From<PresetArg> for ModelPreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Quantized => ModelPreset::Quantized,
            PresetArg::Fp16 => ModelPreset::Fp16,
        }
    }
}

#[derive(Serialize)]
struct CheckOutput {
    model_path: String,
    model_exists: bool,
    onnx_exists: bool,
    tokens_exists: bool,
    ready: bool,
    load_error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    match Cli::parse().command {
        Commands::Transcribe { file, model, json } => transcribe(file, model, json).await,
        Commands::Setup {
            preset,
            model_dir,
            force,
        } => setup(preset.into(), model_dir, force).await,
        Commands::Check { model, json } => check(model, json).await,
    }
}

async fn transcribe(file: PathBuf, model: Option<PathBuf>, json: bool) -> Result<()> {
    if !file.exists() {
        anyhow::bail!("音频文件不存在: {}", file.display());
    }

    let model_path = model_store::ensure_model_ready(model).await?;
    let mut engine = AsrEngine::new(model_path);
    let result = engine.transcribe(&file, Some("auto"))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", result.text);
    }

    Ok(())
}

async fn setup(preset: ModelPreset, model_dir: Option<PathBuf>, force: bool) -> Result<()> {
    let dest_dir = model_store::download_all(model_dir.clone(), preset, force).await?;
    if model_dir.is_none() {
        model_store::save_model_to_config(&dest_dir)?;
        println!("✅ 默认模型路径已更新");
    }
    Ok(())
}

async fn check(model: Option<PathBuf>, json: bool) -> Result<()> {
    let model_path = model_store::ensure_model_ready(model).await?;
    let engine = AsrEngine::new(model_path);
    let status = engine.check_status();

    if json {
        let output = CheckOutput {
            model_path: status.model_path.display().to_string(),
            model_exists: status.model_exists,
            onnx_exists: status.onnx_exists,
            tokens_exists: status.tokens_exists,
            ready: status.ready,
            load_error: status.load_error,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("model_path: {}", status.model_path.display());
        println!("model_exists: {}", status.model_exists);
        println!("onnx_exists: {}", status.onnx_exists);
        println!("tokens_exists: {}", status.tokens_exists);
        println!("ready: {}", status.ready);
        if let Some(err) = status.load_error {
            println!("load_error: {}", err);
        }
    }

    Ok(())
}
