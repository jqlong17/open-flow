use anyhow::Result;
use std::sync::atomic::{Ordering};
use std::sync::{mpsc::Sender, Arc};
use std::thread;
use tracing::{error, info, warn};

use crate::common::types::HotkeyEvent;

/// 热键监听器，通过 rdev（底层 CGEventTap）监听全局按键事件
pub struct HotkeyListener {
    sender: Sender<HotkeyEvent>,
}

impl HotkeyListener {
    pub fn new(sender: Sender<HotkeyEvent>) -> Self {
        Self { sender }
    }

    /// 在独立线程启动热键监听
    pub fn start(self) -> Result<()> {
        info!("正在启动热键监听器（右侧 Command 键，基于 CGEventTap）...");

        thread::spawn(move || {
            if let Err(e) = Self::run_listen_loop(self.sender) {
                error!("热键监听错误: {}", e);
            }
        });

        Ok(())
    }

    fn run_listen_loop(sender: Sender<HotkeyEvent>) -> Result<()> {
        use rdev::{listen, Event, EventType, Key};
        use std::sync::atomic::AtomicBool;

        // 防止连续触发（按住不放时只发一次 Pressed）
        let pressed = Arc::new(AtomicBool::new(false));
        let pressed_clone = pressed.clone();

        println!("⌨️  热键监听器已启动（CGEventTap）");

        let result = listen(move |event: Event| {
            match event.event_type {
                EventType::KeyPress(Key::MetaRight) => {
                    // 只在从未按下状态第一次按下时触发
                    if !pressed_clone.swap(true, Ordering::SeqCst) {
                        if let Err(e) = sender.send(HotkeyEvent::Pressed) {
                            error!("发送热键事件失败: {}", e);
                        }
                    }
                }
                EventType::KeyRelease(Key::MetaRight) => {
                    pressed_clone.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        });

        if let Err(e) = result {
            // rdev 在没有 Accessibility 权限时返回错误
            anyhow::bail!(
                "CGEventTap 启动失败: {:?}\n\
                 请授权辅助功能权限：系统设置 > 隐私与安全性 > 辅助功能",
                e
            );
        }
        Ok(())
    }
}

/// 检查是否已授予 Accessibility 权限（使用 macOS AXIsProcessTrusted）
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use core_foundation::base::TCFType;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        use core_foundation::boolean::CFBoolean;

        extern "C" {
            fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
        }
        // kAXTrustedCheckOptionPrompt = "AXTrustedCheckOptionPrompt"
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let val = CFBoolean::false_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// 请求 Accessibility 权限（弹出系统提示框）
pub fn request_accessibility_permission() {
    #[cfg(target_os = "macos")]
    {
        use core_foundation::base::TCFType;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        use core_foundation::boolean::CFBoolean;

        extern "C" {
            fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
        }
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let val = CFBoolean::true_value(); // true → 触发系统弹窗
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()); }
    }
    warn!("需要 Accessibility 权限才能监听全局热键");
    println!("⚠️  需要 Accessibility 权限");
    println!("请前往：系统设置 > 隐私与安全性 > 辅助功能");
    println!("将终端应用（Terminal / iTerm）添加到列表并启用，然后重新运行。");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permission() {
        let has_permission = check_accessibility_permission();
        println!("Accessibility 权限状态: {}", has_permission);
    }
}
