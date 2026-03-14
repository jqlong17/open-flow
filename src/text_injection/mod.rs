pub mod chinese_convert;

use anyhow::Result;
use std::time::Duration;

/// 文本注入器：可选的中文转换 + 通过 CGEvent 模拟打字（macOS）
/// 或剪贴板粘贴回退（其他平台）
pub struct TextInjector {
    chinese_conversion: String,
}

impl TextInjector {
    pub fn new() -> Self {
        let config = crate::common::config::Config::load().unwrap_or_default();
        Self {
            chinese_conversion: config.chinese_conversion,
        }
    }

    pub async fn inject(&self, text: &str) -> Result<()> {
        // 1. 如果配置了中文转换则应用
        let text = chinese_convert::convert_chinese(text, &self.chinese_conversion);

        // 2. 同时写入剪贴板作为备份
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }

        // 3. 输入文本（平台特定）
        Self::type_text(&text).await
    }

    /// macOS: 使用 CGEvent 键盘事件模拟真实打字。
    /// 比剪贴板粘贴 (Cmd+V) 在不同应用中更一致。
    #[cfg(target_os = "macos")]
    async fn type_text(text: &str) -> Result<()> {
        use std::ffi::c_void;

        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn CGEventCreateKeyboardEvent(
                source: *const c_void,
                virtual_key: u16,
                key_down: bool,
            ) -> *mut c_void;
            fn CGEventKeyboardSetUnicodeString(
                event: *mut c_void,
                string_length: usize,
                unicode_string: *const u16,
            );
            fn CGEventPost(tap: u32, event: *mut c_void);
            fn CFRelease(cf: *const c_void);
        }

        fn post_unicode_chunk(chunk: &[u16]) {
            unsafe {
                // Key down
                let down = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
                if down.is_null() {
                    return;
                }
                CGEventKeyboardSetUnicodeString(down, chunk.len(), chunk.as_ptr());
                CGEventPost(0, down); // 0 = kCGHIDEventTap
                CFRelease(down);

                // Key up
                let up = CGEventCreateKeyboardEvent(std::ptr::null(), 0, false);
                if up.is_null() {
                    return;
                }
                CGEventKeyboardSetUnicodeString(up, chunk.len(), chunk.as_ptr());
                CGEventPost(0, up);
                CFRelease(up);
            }
        }

        if text.is_empty() {
            return Ok(());
        }

        // Normalize newlines for terminal compatibility
        let normalized = text.replace('\n', "\r");
        let utf16: Vec<u16> = normalized.encode_utf16().collect();

        // Post in small chunks with a short delay between each for consistency
        const CHUNK_SIZE: usize = 20;
        const CHUNK_DELAY_MS: u64 = 5;

        for chunk in utf16.chunks(CHUNK_SIZE) {
            post_unicode_chunk(chunk);
            if CHUNK_DELAY_MS > 0 {
                tokio::time::sleep(Duration::from_millis(CHUNK_DELAY_MS)).await;
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn type_text(_text: &str) -> Result<()> {
        // 优先尝试 xdotool（X11 / XWayland），其次 wl-paste+wtype（纯 Wayland）
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
    async fn type_text(_text: &str) -> Result<()> {
        tracing::warn!("当前平台不支持自动粘贴，转写结果已写入剪贴板，请手动粘贴（Ctrl+V / Cmd+V）。");
        Ok(())
    }
}

impl Default for TextInjector {
    fn default() -> Self {
        Self::new()
    }
}
