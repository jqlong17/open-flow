use anyhow::{Context, Result};
use std::time::Duration;

/// 文本注入器：剪贴板 + 模拟 Cmd+V
pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    /// 将文本注入当前焦点窗口，并保留在剪贴板：
    /// 1. 把 text 写入剪贴板
    /// 2. 模拟 Cmd+V（MetaLeft + V）粘贴到当前光标
    pub fn inject(&self, text: &str) -> Result<()> {
        use arboard::Clipboard;
        use rdev::{simulate, EventType, Key};

        // ── 1. 写入转写文本 ───────────────────────────────────────────
        let mut clipboard = Clipboard::new().context("无法访问剪贴板")?;
        clipboard.set_text(text).context("写入剪贴板失败")?;

        // 给剪贴板一点时间同步
        std::thread::sleep(Duration::from_millis(60));

        // ── 2. 模拟 Cmd+V ─────────────────────────────────────────────
        let press = |key: Key| {
            simulate(&EventType::KeyPress(key))
                .map_err(|e| anyhow::anyhow!("模拟按键失败: {:?}", e))
        };
        let release = |key: Key| {
            simulate(&EventType::KeyRelease(key))
                .map_err(|e| anyhow::anyhow!("模拟按键释放失败: {:?}", e))
        };

        press(Key::MetaLeft)?;
        std::thread::sleep(Duration::from_millis(20));
        press(Key::KeyV)?;
        std::thread::sleep(Duration::from_millis(20));
        release(Key::KeyV)?;
        std::thread::sleep(Duration::from_millis(20));
        release(Key::MetaLeft)?;

        Ok(())
    }
}

impl Default for TextInjector {
    fn default() -> Self {
        Self::new()
    }
}
