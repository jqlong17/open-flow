use anyhow::{Context, Result};
use std::time::Duration;

/// 文本注入器：剪贴板 + osascript Cmd+V
pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    /// 将文本注入当前焦点窗口：
    /// 1. 把 text 写入剪贴板
    /// 2. 通过 osascript（Accessibility API）发送 Cmd+V 粘贴
    ///
    /// 不使用 rdev::simulate，因为 simulate 通过 CGEventPost 投递，
    /// 会被 CGEventTap 捕获，与用户同时按下的右 Command 产生修饰键
    /// 状态竞争，导致真实 KeyPress(MetaRight) 丢失。
    ///
    /// 使用 async fn + tokio::time::sleep，避免阻塞 tokio worker 线程。
    pub async fn inject(&self, text: &str) -> Result<()> {
        use arboard::Clipboard;

        // ── 1. 写入转写文本 ───────────────────────────────────────────
        let mut clipboard = Clipboard::new().context("无法访问剪贴板")?;
        clipboard.set_text(text).context("写入剪贴板失败")?;

        // 给剪贴板一点时间同步（非阻塞，不占用 tokio worker）
        tokio::time::sleep(Duration::from_millis(60)).await;

        // ── 2. 用 osascript 发送 Cmd+V（走 AX API，不经过 CGEventTap）──
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
            .output()
            .context("osascript 执行失败")?;

        Ok(())
    }
}

impl Default for TextInjector {
    fn default() -> Self {
        Self::new()
    }
}
