use anyhow::{Context, Result};
use std::time::Duration;

/// 文本注入器：剪贴板 + 模拟粘贴快捷键
pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    pub async fn inject(&self, text: &str) -> Result<()> {
        use arboard::Clipboard;

        // 1. 写入剪贴板
        let mut clipboard = Clipboard::new().context("无法访问剪贴板")?;
        clipboard.set_text(text).context("写入剪贴板失败")?;

        tokio::time::sleep(Duration::from_millis(60)).await;

        // 2. 模拟粘贴（平台分支）
        Self::paste_from_clipboard(text).await
    }

    #[cfg(target_os = "macos")]
    async fn paste_from_clipboard(_text: &str) -> Result<()> {
        // osascript 走 Accessibility API（不经过 CGEventTap，避免修饰键竞争）
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
            .output()
            .context("osascript 执行失败")?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn paste_from_clipboard(_text: &str) -> Result<()> {
        // 优先尝试 xdotool（X11 / XWayland），其次 wl-paste+wtype（纯 Wayland）
        // xdotool key --clearmodifiers ctrl+v
        let xdotool = std::process::Command::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .output();

        match xdotool {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::warn!("xdotool 执行失败: {}", stderr);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!("未找到 xdotool，尝试 wtype...");
            }
            Err(e) => {
                tracing::warn!("xdotool 启动失败: {}", e);
            }
        }

        // fallback: wtype（Wayland 原生）
        let wtype = std::process::Command::new("wtype")
            .args(["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"])
            .output();

        match wtype {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                anyhow::bail!(
                    "xdotool 和 wtype 均执行失败。\n\
                     X11 用户请安装 xdotool：sudo apt install xdotool\n\
                     Wayland 用户请安装 wtype：sudo apt install wtype\n\
                     wtype 错误: {}",
                    stderr
                )
            }
            Err(_) => anyhow::bail!(
                "未找到 xdotool 或 wtype，无法模拟粘贴。\n\
                 X11 用户：sudo apt install xdotool\n\
                 Wayland 用户：sudo apt install wtype"
            ),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    async fn paste_from_clipboard(_text: &str) -> Result<()> {
        tracing::warn!("当前平台不支持自动粘贴，转写结果已写入剪贴板，请手动粘贴（Ctrl+V / Cmd+V）。");
        Ok(())
    }
}

impl Default for TextInjector {
    fn default() -> Self {
        Self::new()
    }
}
