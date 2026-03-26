use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub async fn run(tail_lines: usize) -> Result<()> {
    let config_path = crate::common::config::Config::config_path()?;
    let data_dir = crate::common::config::Config::data_dir()?;
    let log_path = crate::common::logging::log_path()?;
    let current_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let config = crate::common::config::Config::load().unwrap_or_default();

    println!("Open Flow Support Bundle");
    println!();
    println!("Platform: {}", std::env::consts::OS);
    println!("Arch: {}", std::env::consts::ARCH);
    println!("Current exe: {}", current_exe);
    println!("Config path: {}", config_path.display());
    println!("Data dir: {}", data_dir.display());
    println!("Log path: {}", log_path.display());
    println!("Hotkey: {}", config.hotkey);
    println!("Trigger mode: {}", config.trigger_mode);
    println!("Capture mode: {}", config.capture_mode);
    println!(
        "Input source: {}",
        config
            .resolved_input_source()
            .unwrap_or_else(|| "<system_default>".to_string())
    );
    println!(
        "Model path: {}",
        config
            .model_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unset>".to_string())
    );
    println!();

    println!("Audio Devices");
    match crate::audio::list_input_devices() {
        Ok(snapshot) => {
            println!(
                "Default: {}",
                snapshot.default_device_name.unwrap_or_else(|| "<none>".to_string())
            );
            for device in snapshot.devices {
                println!(
                    "{} {}",
                    if device.is_default { "*" } else { "-" },
                    device.name
                );
            }
        }
        Err(err) => {
            println!("Failed to enumerate audio devices: {}", err);
        }
    }
    println!();

    println!("Recent Log Tail");
    print_log_tail(&log_path, tail_lines);
    println!();
    println!("Share the full output above with the maintainer when reporting Windows/Linux issues.");

    Ok(())
}

fn print_log_tail(log_path: &PathBuf, tail_lines: usize) {
    let line_count = tail_lines.max(20);
    match fs::read_to_string(log_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(line_count);
            for line in &lines[start..] {
                println!("{}", line);
            }
            if lines.is_empty() {
                println!("<log file is empty>");
            }
        }
        Err(err) => {
            println!("<failed to read log file: {}>", err);
        }
    }
}
