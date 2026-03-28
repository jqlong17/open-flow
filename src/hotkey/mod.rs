use anyhow::Result;
use std::sync::atomic::Ordering;
use std::sync::{mpsc::Sender, Arc};
use std::thread;
use tracing::{error, info, warn};

use crate::common::types::HotkeyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrophonePermissionStatus {
    NotDetermined,
    Restricted,
    Denied,
    Authorized,
    Unknown(i64),
    Unsupported,
}

impl MicrophonePermissionStatus {
    pub fn is_authorized(self) -> bool {
        matches!(self, Self::Authorized | Self::Unsupported)
    }

    pub fn can_prompt(self) -> bool {
        matches!(self, Self::NotDetermined)
    }

    pub fn status_code(self) -> Option<i64> {
        match self {
            Self::NotDetermined => Some(0),
            Self::Restricted => Some(1),
            Self::Denied => Some(2),
            Self::Authorized => Some(3),
            Self::Unknown(raw) => Some(raw),
            Self::Unsupported => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotDetermined => "not_determined",
            Self::Restricted => "restricted",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
            Self::Unknown(_) => "unknown",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::NotDetermined => "Not Determined",
            Self::Restricted => "Restricted",
            Self::Denied => "Denied",
            Self::Authorized => "Authorized",
            Self::Unknown(_) => "Unknown",
            Self::Unsupported => "Unsupported",
        }
    }
}

/// 热键监听器，通过 rdev（底层 CGEventTap）监听全局按键事件
pub struct HotkeyListener {
    sender: Sender<HotkeyEvent>,
    hotkey: String,
}

impl HotkeyListener {
    pub fn new(sender: Sender<HotkeyEvent>, hotkey: String) -> Self {
        Self { sender, hotkey }
    }

    /// 在独立线程启动热键监听
    pub fn start(self) -> Result<()> {
        info!("正在启动热键监听器（热键: {}）...", self.hotkey);

        let hotkey = self.hotkey.clone();
        thread::spawn(move || {
            if let Err(e) = Self::run_listen_loop(self.sender, &hotkey) {
                error!("热键监听错误: {}", e);
            }
        });

        Ok(())
    }

    fn run_listen_loop(sender: Sender<HotkeyEvent>, hotkey: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            return Self::run_listen_loop_macos(sender, hotkey);
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Self::run_listen_loop_rdev(sender, hotkey);
        }
    }

    #[cfg(target_os = "macos")]
    fn run_listen_loop_macos(sender: Sender<HotkeyEvent>, hotkey: &str) -> Result<()> {
        use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
        use core_graphics::event::{
            CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventType,
            EventField, KeyCode,
        };
        use std::sync::atomic::{AtomicBool, AtomicU64};

        #[derive(Clone, Copy)]
        enum MacHotkey {
            Fn,
            Modifier {
                label: &'static str,
                log_key: &'static str,
                keycode: u16,
                flag: CGEventFlags,
            },
            Key {
                label: &'static str,
                log_key: &'static str,
                keycode: u16,
            },
        }

        let hotkey_spec = match hotkey {
            "fn" => MacHotkey::Fn,
            "right_option" => MacHotkey::Modifier {
                label: "右侧 Option",
                log_key: "right_option",
                keycode: KeyCode::RIGHT_OPTION,
                flag: CGEventFlags::CGEventFlagAlternate,
            },
            "right_control" => MacHotkey::Modifier {
                label: "右侧 Control",
                log_key: "right_control",
                keycode: KeyCode::RIGHT_CONTROL,
                flag: CGEventFlags::CGEventFlagControl,
            },
            "right_shift" => MacHotkey::Modifier {
                label: "右侧 Shift",
                log_key: "right_shift",
                keycode: KeyCode::RIGHT_SHIFT,
                flag: CGEventFlags::CGEventFlagShift,
            },
            "f13" => MacHotkey::Key {
                label: "F13",
                log_key: "f13",
                keycode: KeyCode::F13,
            },
            _ => MacHotkey::Modifier {
                label: "右侧 Command",
                log_key: "right_cmd",
                keycode: KeyCode::RIGHT_COMMAND,
                flag: CGEventFlags::CGEventFlagCommand,
            },
        };

        let pressed = Arc::new(AtomicBool::new(false));
        let pressed_clone = pressed.clone();
        let press_count = Arc::new(AtomicU64::new(0));
        let release_count = Arc::new(AtomicU64::new(0));
        let pc = press_count.clone();
        let rc = release_count.clone();

        let key_name = match hotkey_spec {
            MacHotkey::Fn => "Fn",
            MacHotkey::Modifier { label, .. } | MacHotkey::Key { label, .. } => label,
        };
        println!("⌨️  热键监听器已启动（CGEventTap，热键: {}）", key_name);

        let current = CFRunLoop::get_current();
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            core_graphics::event::CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::FlagsChanged,
                CGEventType::KeyDown,
                CGEventType::KeyUp,
            ],
            move |_proxy, event_type, event| {
                let keycode =
                    event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

                match hotkey_spec {
                    MacHotkey::Fn => {
                        if event_type as u32 != CGEventType::FlagsChanged as u32 {
                            return Some(event.clone());
                        }

                        let flags = event.get_flags();
                        let fn_down =
                            (flags.bits() & CGEventFlags::CGEventFlagSecondaryFn.bits()) != 0;

                        if fn_down {
                            let was = pressed_clone.swap(true, Ordering::SeqCst);
                            if !was {
                                let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                                println!("[Hotkey] raw_event press={} key=fn", n);
                                if let Err(e) = sender.send(HotkeyEvent::Pressed) {
                                    eprintln!(
                                        "[Hotkey] send_failed key=fn event=Pressed error={}",
                                        e
                                    );
                                    error!("发送热键事件失败: {}", e);
                                }
                            }
                        } else {
                            let was = pressed_clone.swap(false, Ordering::SeqCst);
                            if was {
                                let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                                println!("[Hotkey] raw_event release={} key=fn", n);
                                if let Err(e) = sender.send(HotkeyEvent::Released) {
                                    eprintln!(
                                        "[Hotkey] send_failed key=fn event=Released error={}",
                                        e
                                    );
                                    error!("发送热键事件失败: {}", e);
                                }
                            }
                        }
                    }
                    MacHotkey::Modifier {
                        log_key,
                        keycode: expected_keycode,
                        flag,
                        ..
                    } => {
                        if event_type as u32 != CGEventType::FlagsChanged as u32
                            || keycode != expected_keycode
                        {
                            return Some(event.clone());
                        }

                        let is_pressed = event.get_flags().contains(flag);
                        if is_pressed {
                            let was = pressed_clone.swap(true, Ordering::SeqCst);
                            if !was {
                                let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                                println!("[Hotkey] raw_event press={} key={}", n, log_key);
                                if let Err(e) = sender.send(HotkeyEvent::Pressed) {
                                    eprintln!(
                                        "[Hotkey] send_failed key={} event=Pressed error={}",
                                        log_key, e
                                    );
                                    error!("发送热键事件失败: {}", e);
                                }
                            }
                        } else {
                            let was = pressed_clone.swap(false, Ordering::SeqCst);
                            if was {
                                let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                                println!("[Hotkey] raw_event release={} key={}", n, log_key);
                                if let Err(e) = sender.send(HotkeyEvent::Released) {
                                    eprintln!(
                                        "[Hotkey] send_failed key={} event=Released error={}",
                                        log_key, e
                                    );
                                    error!("发送热键事件失败: {}", e);
                                }
                            }
                        }
                    }
                    MacHotkey::Key {
                        log_key,
                        keycode: expected_keycode,
                        ..
                    } => {
                        if keycode != expected_keycode {
                            return Some(event.clone());
                        }

                        match event_type {
                            CGEventType::KeyDown => {
                                let is_repeat = event
                                    .get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT)
                                    != 0;
                                if is_repeat {
                                    return Some(event.clone());
                                }
                                let was = pressed_clone.swap(true, Ordering::SeqCst);
                                if !was {
                                    let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                                    println!("[Hotkey] raw_event press={} key={}", n, log_key);
                                    if let Err(e) = sender.send(HotkeyEvent::Pressed) {
                                        eprintln!(
                                            "[Hotkey] send_failed key={} event=Pressed error={}",
                                            log_key, e
                                        );
                                        error!("发送热键事件失败: {}", e);
                                    }
                                }
                            }
                            CGEventType::KeyUp => {
                                let was = pressed_clone.swap(false, Ordering::SeqCst);
                                if was {
                                    let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                                    println!("[Hotkey] raw_event release={} key={}", n, log_key);
                                    if let Err(e) = sender.send(HotkeyEvent::Released) {
                                        eprintln!(
                                            "[Hotkey] send_failed key={} event=Released error={}",
                                            log_key, e
                                        );
                                        error!("发送热键事件失败: {}", e);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                Some(event.clone())
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
    fn run_listen_loop_rdev(sender: Sender<HotkeyEvent>, hotkey: &str) -> Result<()> {
        use rdev::{listen, Event, EventType, Key};
        use std::sync::atomic::{AtomicBool, AtomicU64};

        #[derive(Clone, Copy)]
        struct RdevHotkey {
            key: Key,
            label: &'static str,
            log_key: &'static str,
        }

        let hotkey_spec = {
            #[cfg(target_os = "windows")]
            {
                match hotkey.trim().to_ascii_lowercase().as_str() {
                    "right_alt" | "altgr" => RdevHotkey {
                        key: Key::AltGr,
                        label: "Right Alt",
                        log_key: "right_alt",
                    },
                    _ => RdevHotkey {
                        key: Key::MetaRight,
                        label: "Right Win",
                        log_key: "right_win",
                    },
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                match hotkey.trim().to_ascii_lowercase().as_str() {
                    "right_win" | "right_meta" | "meta_right" => RdevHotkey {
                        key: Key::MetaRight,
                        label: "Right Meta",
                        log_key: "right_win",
                    },
                    _ => RdevHotkey {
                        key: Key::AltGr,
                        label: "Right Alt",
                        log_key: "right_alt",
                    },
                }
            }
        };

        let pressed = Arc::new(AtomicBool::new(false));
        let pressed_clone = pressed.clone();
        let press_count = Arc::new(AtomicU64::new(0));
        let release_count = Arc::new(AtomicU64::new(0));
        let pc = press_count.clone();
        let rc = release_count.clone();

        println!("⌨️  热键监听器已启动（rdev，热键: {}）", hotkey_spec.label);

        let result = listen(move |event: Event| match event.event_type {
            EventType::KeyPress(key) if key == hotkey_spec.key => {
                let was = pressed_clone.swap(true, Ordering::SeqCst);
                if !was {
                    let n = pc.fetch_add(1, Ordering::SeqCst) + 1;
                    println!("[Hotkey] raw_event press={} key={}", n, hotkey_spec.log_key);
                    if let Err(e) = sender.send(HotkeyEvent::Pressed) {
                        eprintln!(
                            "[Hotkey] send_failed key={} event=Pressed error={}",
                            hotkey_spec.log_key, e
                        );
                        error!("发送热键事件失败: {}", e);
                    }
                }
            }
            EventType::KeyRelease(key) if key == hotkey_spec.key => {
                let was = pressed_clone.swap(false, Ordering::SeqCst);
                if was {
                    let n = rc.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                        "[Hotkey] raw_event release={} key={}",
                        n, hotkey_spec.log_key
                    );
                    if let Err(e) = sender.send(HotkeyEvent::Released) {
                        eprintln!(
                            "[Hotkey] send_failed key={} event=Released error={}",
                            hotkey_spec.log_key, e
                        );
                        error!("发送热键事件失败: {}", e);
                    }
                }
            }
            _ => {}
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
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        extern "C" {
            fn AXIsProcessTrustedWithOptions(
                options: core_foundation::dictionary::CFDictionaryRef,
            ) -> bool;
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

/// 请求 Accessibility 权限——触发 macOS 系统对话框
pub fn request_accessibility_permission() {
    let ui = crate::common::config::Config::load()
        .map(|config| crate::common::ui::UiLanguage::from_config(&config))
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        extern "C" {
            fn AXIsProcessTrustedWithOptions(
                options: core_foundation::dictionary::CFDictionaryRef,
            ) -> bool;
        }

        // kAXTrustedCheckOptionPrompt = true → 触发系统权限对话框
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let val = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        unsafe {
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
        }
    }

    warn!(
        "{}",
        ui.pick(
            "需要 Accessibility 权限才能监听全局热键",
            "Accessibility permission is required to listen for global hotkeys",
        )
    );
    println!(
        "{}",
        ui.pick(
            "⚠️  需要 Accessibility 权限",
            "⚠️  Accessibility permission required"
        )
    );
    println!(
        "{}",
        ui.pick(
            "请前往：系统设置 > 隐私与安全性 > 辅助功能",
            "Please open: System Settings > Privacy & Security > Accessibility",
        )
    );
    println!(
        "{}",
        ui.pick(
            "将 Open Flow.app 添加到列表并启用，然后完全退出后重新打开应用。",
            "Add Open Flow.app to the list and enable it, then fully quit and reopen the app.",
        )
    );
}

/// 请求 Input Monitoring 权限——触发 macOS 系统对话框
pub fn request_input_monitoring_permission() {
    let ui = crate::common::config::Config::load()
        .map(|config| crate::common::ui::UiLanguage::from_config(&config))
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        unsafe extern "C" {
            fn CGRequestListenEventAccess() -> bool;
        }

        unsafe {
            CGRequestListenEventAccess();
        }
    }

    warn!(
        "{}",
        ui.pick(
            "需要 Input Monitoring 权限才能监听全局热键",
            "Input Monitoring permission is required to listen for global hotkeys",
        )
    );
    println!(
        "{}",
        ui.pick(
            "⚠️  需要“输入监控”权限",
            "⚠️  Input Monitoring permission required"
        )
    );
    println!(
        "{}",
        ui.pick(
            "请前往：系统设置 > 隐私与安全性 > 输入监控",
            "Please open: System Settings > Privacy & Security > Input Monitoring",
        )
    );
    println!(
        "{}",
        ui.pick(
            "将 Open Flow.app 添加到列表并启用，然后完全退出后重新打开应用。",
            "Add Open Flow.app to the list and enable it, then fully quit and reopen the app.",
        )
    );
}

/// 请求麦克风权限
pub fn request_microphone_permission() {
    let ui = crate::common::config::Config::load()
        .map(|config| crate::common::ui::UiLanguage::from_config(&config))
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        match request_microphone_permission_macos() {
            Some(MicrophonePermissionStatus::Authorized) => {
                println!(
                    "{}",
                    ui.pick(
                        "   已收到麦克风授权结果：允许。请重启 Open Flow 让新权限生效。",
                        "   Microphone access was granted. Please restart Open Flow so the new permission takes effect.",
                    )
                );
                return;
            }
            Some(MicrophonePermissionStatus::Denied | MicrophonePermissionStatus::Restricted) => {
                println!(
                    "{}",
                    ui.pick(
                        "   麦克风权限已被拒绝。请前往：系统设置 > 隐私与安全性 > 麦克风，手动开启 Open Flow.app。",
                        "   Microphone access was denied. Open System Settings > Privacy & Security > Microphone and enable Open Flow.app manually.",
                    )
                );
                return;
            }
            Some(MicrophonePermissionStatus::NotDetermined) => {}
            Some(MicrophonePermissionStatus::Unknown(raw)) => {
                info!("Microphone permission request completed with unknown status={}", raw);
            }
            Some(MicrophonePermissionStatus::Unsupported) => return,
            None => {
                info!("Microphone permission request did not complete synchronously");
            }
        }
    }

    info!(
        "{}",
        ui.pick(
            "麦克风权限尚未授权。首次录音时将弹出系统对话框。",
            "Microphone permission is not granted yet. A system prompt will appear the first time you record.",
        )
    );
    println!(
        "{}",
        ui.pick(
            "   首次录音时将弹出麦克风权限对话框。",
            "   A microphone permission dialog will appear the first time you record.",
        )
    );
    println!(
        "{}",
        ui.pick(
            "   如果没有弹出，请前往：系统设置 > 隐私与安全性 > 麦克风",
            "   If it does not appear, open: System Settings > Privacy & Security > Microphone",
        )
    );
}

/// 检查麦克风权限状态。
/// 返回 true 表示已授权（AVAuthorizationStatusAuthorized）。
/// 未确定（0）时返回 false——首次运行需 NSMicrophoneUsageDescription 触发系统弹框。
pub fn check_microphone_permission() -> bool {
    microphone_permission_status().is_authorized()
}

pub fn microphone_permission_status() -> MicrophonePermissionStatus {
    #[cfg(target_os = "macos")]
    {
        let status = microphone_authorization_status_macos();
        let mapped = match status {
            0 => MicrophonePermissionStatus::NotDetermined,
            1 => MicrophonePermissionStatus::Restricted,
            2 => MicrophonePermissionStatus::Denied,
            3 => MicrophonePermissionStatus::Authorized,
            raw => MicrophonePermissionStatus::Unknown(raw),
        };
        info!(
            "Microphone TCC status: {} ({})",
            status,
            mapped.as_str()
        );
        mapped
    }
    #[cfg(not(target_os = "macos"))]
    {
        MicrophonePermissionStatus::Unsupported
    }
}

#[cfg(target_os = "macos")]
fn microphone_authorization_status_macos() -> i64 {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    unsafe {
        let ns_string_cls = class!(NSString);
        let audio_type: *mut Object =
            msg_send![ns_string_cls, stringWithUTF8String: b"soun\0".as_ptr() as *const i8];
        msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: audio_type]
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_permission_macos() -> Option<MicrophonePermissionStatus> {
    use block::ConcreteBlock;
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    let current_status = microphone_permission_status();
    if matches!(
        current_status,
        MicrophonePermissionStatus::Authorized
            | MicrophonePermissionStatus::Denied
            | MicrophonePermissionStatus::Restricted
            | MicrophonePermissionStatus::Unsupported
    ) {
        return Some(current_status);
    }

    let completed = Arc::new((Mutex::new(None::<bool>), Condvar::new()));
    let completed_clone = completed.clone();

    unsafe {
        let ns_string_cls = class!(NSString);
        let audio_type: *mut Object =
            msg_send![ns_string_cls, stringWithUTF8String: b"soun\0".as_ptr() as *const i8];

        let block = ConcreteBlock::new(move |granted: bool| {
            let (lock, condvar) = &*completed_clone;
            *lock.lock().unwrap() = Some(granted);
            condvar.notify_all();
        })
        .copy();

        let _: () = msg_send![
            class!(AVCaptureDevice),
            requestAccessForMediaType: audio_type
            completionHandler: &*block
        ];
    }

    let (lock, condvar) = &*completed;
    let result = condvar
        .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(8), |state| {
            state.is_none()
        })
        .ok()?;

    match *result.0 {
        Some(true) => Some(MicrophonePermissionStatus::Authorized),
        Some(false) => Some(microphone_permission_status()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permission() {
        let has_permission = check_accessibility_permission();
        println!("Accessibility permission status: {}", has_permission);
    }
}
