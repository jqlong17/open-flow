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
        #[cfg(target_os = "macos")]
        {
            return Self::run_listen_loop_macos(sender);
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Self::run_listen_loop_rdev(sender);
        }
    }

    #[cfg(target_os = "macos")]
    fn run_listen_loop_macos(sender: Sender<HotkeyEvent>) -> Result<()> {
        use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
        use core_graphics::event::{
            CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventType, EventField,
            KeyCode,
        };
        use std::sync::atomic::{AtomicBool, AtomicU64};

        let pressed = Arc::new(AtomicBool::new(false));
        let pressed_clone = pressed.clone();
        let press_count = Arc::new(AtomicU64::new(0));
        let release_count = Arc::new(AtomicU64::new(0));
        let pc = press_count.clone();
        let rc = release_count.clone();

        println!("⌨️  热键监听器已启动（CGEventTap）");

        let current = CFRunLoop::get_current();
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            core_graphics::event::CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::FlagsChanged],
            move |_proxy, event_type, event| {
                if event_type as u32 != CGEventType::FlagsChanged as u32 {
                    return None;
                }

                let keycode =
                    event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                if keycode != KeyCode::RIGHT_COMMAND {
                    return None;
                }

                let is_pressed = event.get_flags().contains(CGEventFlags::CGEventFlagCommand);
                if is_pressed {
                    let was = pressed_clone.swap(true, Ordering::SeqCst);
                    if !was {
                        let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                        info!(
                            "[Hotkey] 事件 #press={} 按下（右侧 Command）was_pressed={} -> 发送",
                            n, was
                        );
                        if let Err(e) = sender.send(HotkeyEvent) {
                            error!("发送热键事件失败: {}", e);
                        }
                    }
                } else {
                    let was = pressed_clone.swap(false, Ordering::SeqCst);
                    if was {
                        let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                        info!(
                            "[Hotkey] 事件 #release={} 松开（右侧 Command）was_pressed={}",
                            n, was
                        );
                    }
                }

                None
            },
        )
        .map_err(|_| anyhow::anyhow!("CGEventTap 创建失败，请确认已授予辅助功能和输入监控权限"))?;

        let loop_source = tap
            .mach_port
            .create_runloop_source(0)
            .map_err(|_| anyhow::anyhow!("无法创建 CGEventTap RunLoopSource"))?;
        unsafe {
            current.add_source(&loop_source, kCFRunLoopCommonModes);
        }
        tap.enable();
        CFRunLoop::run_current();
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn run_listen_loop_rdev(sender: Sender<HotkeyEvent>) -> Result<()> {
        use rdev::{listen, Event, EventType, Key};
        use std::sync::atomic::{AtomicBool, AtomicU64};

        let pressed = Arc::new(AtomicBool::new(false));
        let pressed_clone = pressed.clone();
        let press_count = Arc::new(AtomicU64::new(0));
        let release_count = Arc::new(AtomicU64::new(0));
        let pc = press_count.clone();
        let rc = release_count.clone();

        println!("⌨️  热键监听器已启动（rdev）");

        // Windows / Linux: 右侧 Alt（AltGr）；与 macOS 右 Command 区分
        let result = listen(move |event: Event| {
            let hotkey_press = matches!(event.event_type, EventType::KeyPress(Key::AltGr));
            let hotkey_release = matches!(event.event_type, EventType::KeyRelease(Key::AltGr));
            if hotkey_press {
                let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                let was = pressed_clone.swap(true, Ordering::SeqCst);
                info!(
                    "[Hotkey] 事件 #press={} 按下（右侧 Alt）was_pressed={} -> 发送",
                    n, was
                );
                if let Err(e) = sender.send(HotkeyEvent) {
                    error!("发送热键事件失败: {}", e);
                }
            } else if hotkey_release {
                let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                let was = pressed_clone.swap(false, Ordering::SeqCst);
                info!(
                    "[Hotkey] 事件 #release={} 松开（右侧 Alt）was_pressed={}",
                    n, was
                );
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

/// 检查是否已授予 Input Monitoring 权限（macOS 监听全局键盘事件需要）
pub fn check_input_monitoring_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        unsafe extern "C" {
            fn CGPreflightListenEventAccess() -> bool;
        }

        unsafe { CGPreflightListenEventAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// 提示用户手动授予 Accessibility 权限（不主动弹系统框）
pub fn request_accessibility_permission() {
    warn!("需要 Accessibility 权限才能监听全局热键");
    println!("⚠️  需要 Accessibility 权限");
    println!("请前往：系统设置 > 隐私与安全性 > 辅助功能");
    println!("将 Open Flow.app 添加到列表并启用，然后完全退出后重新打开应用。");
}

/// 提示用户手动授予 Input Monitoring 权限（不主动弹系统框）
pub fn request_input_monitoring_permission() {
    warn!("需要 Input Monitoring 权限才能监听全局热键");
    println!("⚠️  需要“输入监控”权限");
    println!("请前往：系统设置 > 隐私与安全性 > 输入监控");
    println!("将 Open Flow.app 添加到列表并启用，然后完全退出后重新打开应用。");
}

/// 检查麦克风权限状态。
/// 返回 true 表示已授权（AVAuthorizationStatusAuthorized）。
/// 未确定（0）时返回 false——首次运行需 NSMicrophoneUsageDescription 触发系统弹框。
pub fn check_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc::{class, msg_send, sel, sel_impl};
        use objc::runtime::Object;

        // 链接 AVFoundation 框架（仅需声明，不需要 extern fn）
        #[link(name = "AVFoundation", kind = "framework")]
        extern "C" {}

        unsafe {
            // AVMediaTypeAudio = @"soun"
            let ns_string_cls = class!(NSString);
            let audio_type: *mut Object =
                msg_send![ns_string_cls, stringWithUTF8String: b"soun\0".as_ptr() as *const i8];

            // [AVCaptureDevice authorizationStatusForMediaType:] → i64
            // 0=NotDetermined 1=Restricted 2=Denied 3=Authorized
            let status: i64 =
                msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: audio_type];

            info!("Microphone TCC status: {}", status);
            status == 3
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
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
