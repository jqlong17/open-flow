//! Settings window: macOS has native NSWindow, other platforms are no-op stubs.
//! v1: Shows current settings as labels + button to open config file.

// ─────────────────────────────────────────────────────────────────────────────
// macOS implementation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use crate::common::{config::Config, ui::UiLanguage};
    use cocoa::foundation::{NSPoint, NSRect, NSSize};
    use objc::runtime::{Object, NO, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::atomic::{AtomicBool, Ordering};

    const WINDOW_WIDTH: f64 = 400.0;
    const WINDOW_HEIGHT: f64 = 300.0;

    pub struct SettingsWindow {
        window: *mut Object,
        visible: AtomicBool,
    }

    unsafe impl Send for SettingsWindow {}
    unsafe impl Sync for SettingsWindow {}

    impl SettingsWindow {
        pub fn new() -> Option<Self> {
            unsafe {
                let frame = NSRect::new(
                    NSPoint::new(200.0, 200.0),
                    NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
                );

                // NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                let style_mask: u64 = (1 << 0) | (1 << 1);
                let window: *mut Object = msg_send![class!(NSWindow), alloc];
                let window: *mut Object = msg_send![window,
                    initWithContentRect: frame
                    styleMask: style_mask
                    backing: 2u64
                    defer: NO
                ];

                if window.is_null() {
                    return None;
                }

                let ui_language = Config::load()
                    .map(|config| UiLanguage::from_config(&config))
                    .unwrap_or_default();
                let title = ns_string(ui_language.settings_window_title());
                let _: () = msg_send![window, setTitle: title];
                let _: () = msg_send![window, center];

                // Build UI
                Self::build_ui(window, ui_language);

                // Start hidden
                let _: () = msg_send![window, orderOut: std::ptr::null::<Object>()];

                Some(Self {
                    window,
                    visible: AtomicBool::new(false),
                })
            }
        }

        pub fn show(&self) {
            unsafe {
                self.reload_values();
                let _: () = msg_send![self.window,
                    makeKeyAndOrderFront: std::ptr::null::<Object>()];
                // Activate app so window can receive focus (LSUIElement apps need this)
                let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: YES];
                self.visible.store(true, Ordering::SeqCst);
            }
        }

        #[allow(dead_code)]
        pub fn is_visible(&self) -> bool {
            self.visible.load(Ordering::SeqCst)
        }

        unsafe fn build_ui(window: *mut Object, ui_language: UiLanguage) {
            let content: *mut Object = msg_send![window, contentView];
            let mut y = WINDOW_HEIGHT - 50.0;
            let label_x = 20.0;
            let row_height = 28.0;

            // Title
            Self::add_label(content, ui_language.settings_heading(), label_x, y, true);
            y -= row_height + 8.0;

            // Info text
            Self::add_label(content, ui_language.settings_edit_config_hint(), label_x, y, false);
            y -= row_height;
            Self::add_label(content, ui_language.settings_restart_hint(), label_x, y, false);
            y -= row_height + 16.0;

            // Current settings display
            Self::add_label(content, ui_language.settings_current_config(), label_x, y, true);
            y -= row_height;

            // These will be updated in reload_values, but show placeholders
            let tag_base = 100i64;
            for (i, label) in [
                "Provider: ...",
                "Hotkey: ...",
                "Trigger: ...",
                "Groq Model: ...",
            ]
            .iter()
            .enumerate()
            {
                let lbl = Self::create_label(content, label, label_x + 10.0, y);
                let _: () = msg_send![lbl, setTag: tag_base + i as i64];
                y -= row_height;
            }

            // "Open Config File" button
            let btn_frame = NSRect::new(
                NSPoint::new((WINDOW_WIDTH - 180.0) / 2.0, 20.0),
                NSSize::new(180.0, 32.0),
            );
            let button: *mut Object = msg_send![class!(NSButton), alloc];
            let button: *mut Object = msg_send![button, initWithFrame: btn_frame];
            let btn_title = ns_string(ui_language.settings_open_config_file());
            let _: () = msg_send![button, setTitle: btn_title];
            let _: () = msg_send![button, setBezelStyle: 1i64]; // rounded
            let _: () = msg_send![content, addSubview: button];
        }

        unsafe fn add_label(parent: *mut Object, text: &str, x: f64, y: f64, bold: bool) {
            let lbl = Self::create_label(parent, text, x, y);
            if bold {
                let font: *mut Object = msg_send![class!(NSFont), boldSystemFontOfSize: 13.0f64];
                let _: () = msg_send![lbl, setFont: font];
            }
        }

        unsafe fn create_label(parent: *mut Object, text: &str, x: f64, y: f64) -> *mut Object {
            let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(360.0, 20.0));
            let label: *mut Object = msg_send![class!(NSTextField), alloc];
            let label: *mut Object = msg_send![label, initWithFrame: frame];
            let _: () = msg_send![label, setBezeled: NO];
            let _: () = msg_send![label, setDrawsBackground: NO];
            let _: () = msg_send![label, setEditable: NO];
            let _: () = msg_send![label, setSelectable: NO];
            let ns_str = ns_string(text);
            let _: () = msg_send![label, setStringValue: ns_str];
            let _: () = msg_send![parent, addSubview: label];
            label
        }

        fn reload_values(&self) {
            let config = crate::common::config::Config::load().unwrap_or_default();
            let ui_language = UiLanguage::from_config(&config);
            unsafe {
                let content: *mut Object = msg_send![self.window, contentView];
                let tag_base = 100i64;
                let labels = [
                    format!("{}: {}", ui_language.settings_provider(), config.provider),
                    format!("{}: {}", ui_language.settings_hotkey(), config.hotkey),
                    format!("{}: {}", ui_language.settings_trigger(), config.trigger_mode),
                    format!("{}: {}", ui_language.settings_groq_model(), config.groq_model),
                ];
                for (i, text) in labels.iter().enumerate() {
                    let tag = tag_base + i as i64;
                    let view: *mut Object = msg_send![content, viewWithTag: tag];
                    if !view.is_null() {
                        let ns_str = ns_string(text);
                        let _: () = msg_send![view, setStringValue: ns_str];
                    }
                }
            }
        }
    }

    impl Drop for SettingsWindow {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.window, orderOut: std::ptr::null::<Object>()];
                let _: () = msg_send![self.window, close];
            }
        }
    }

    /// Create an NSString from a Rust &str.
    unsafe fn ns_string(s: &str) -> *mut Object {
        let cls = class!(NSString);
        let c_str = std::ffi::CString::new(s).unwrap();
        msg_send![cls, stringWithUTF8String: c_str.as_ptr()]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Non-macOS stub
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
mod platform {
    pub struct SettingsWindow;

    impl SettingsWindow {
        pub fn new() -> Option<Self> {
            Some(Self)
        }
        pub fn show(&self) {}
        #[allow(dead_code)]
        pub fn is_visible(&self) -> bool {
            false
        }
    }
}

pub use platform::SettingsWindow;
