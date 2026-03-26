use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn log_path() -> Result<PathBuf> {
    Ok(crate::common::config::Config::data_dir()?.join("daemon.log"))
}

pub fn init_tracing(app_name: &str) -> Result<PathBuf> {
    let path = log_path()?;
    let _ = LOG_PATH.set(path.clone());

    let file_for_layer = path.clone();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(move || {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_for_layer)
                .expect("open tracing log file")
        });

    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()
        .ok();

    append_raw_line(&format!(
        "[Support] {} logging initialized path={}",
        app_name,
        path.display()
    ));
    install_panic_hook(app_name.to_string());

    Ok(path)
}

pub fn append_raw_line(message: &str) {
    let Some(path) = LOG_PATH.get().cloned().or_else(|| log_path().ok()) else {
        return;
    };

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", message);
    }
}

fn install_panic_hook(app_name: String) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        append_raw_line(&format!("[Panic] {} {}", app_name, panic_info));
        default_hook(panic_info);
    }));
}
