//! Floating indicator overlay: macOS uses native NSPanel near cursor,
//! other platforms are no-op stubs.

use crate::tray::TrayIconState;

// ─────────────────────────────────────────────────────────────────────────────
// macOS implementation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use cocoa::foundation::{NSPoint, NSRect, NSSize};
    use objc::runtime::{Object, NO, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::atomic::{AtomicBool, Ordering};

    const PILL_WIDTH: f64 = 140.0;
    const PILL_HEIGHT: f64 = 28.0;
    const CURSOR_OFFSET_Y: f64 = 20.0;

    pub struct OverlayWindow {
        panel: *mut Object,
        text_field: *mut Object,
        dot_view: *mut Object,
        visible: AtomicBool,
    }

    unsafe impl Send for OverlayWindow {}
    unsafe impl Sync for OverlayWindow {}

    impl OverlayWindow {
        pub fn new() -> Option<Self> {
            unsafe {
                let frame = NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(PILL_WIDTH, PILL_HEIGHT),
                );

                let style_mask: u64 = 0; // NSWindowStyleMaskBorderless
                let panel: *mut Object = msg_send![class!(NSPanel), alloc];
                let panel: *mut Object = msg_send![panel,
                    initWithContentRect: frame
                    styleMask: style_mask
                    backing: 2u64 // NSBackingStoreBuffered
                    defer: NO
                ];

                if panel.is_null() {
                    return None;
                }

                // Configure panel
                let _: () = msg_send![panel, setLevel: 25i64]; // NSStatusWindowLevel (above everything)
                let _: () = msg_send![panel, setOpaque: NO];
                let _: () = msg_send![panel, setHasShadow: NO];
                let _: () = msg_send![panel, setIgnoresMouseEvents: YES];
                // canJoinAllSpaces | stationary (visible on all spaces, doesn't move with space switch)
                let _: () = msg_send![panel, setCollectionBehavior: (1u64 << 0) | (1u64 << 4)];
                // Allow panel to show even when app is not active (critical for Accessory apps)
                let _: () = msg_send![panel, setHidesOnDeactivate: NO];

                // Set transparent background
                let clear_color: *mut Object = msg_send![class!(NSColor), clearColor];
                let _: () = msg_send![panel, setBackgroundColor: clear_color];

                // Create content view with rounded dark background
                let content_view: *mut Object = msg_send![panel, contentView];

                // Create a visual effect view for blur
                let effect_view: *mut Object = msg_send![class!(NSVisualEffectView), alloc];
                let content_frame: NSRect = msg_send![content_view, bounds];
                let effect_view: *mut Object =
                    msg_send![effect_view, initWithFrame: content_frame];
                let _: () = msg_send![effect_view, setMaterial: 13i64]; // HUDWindow
                let _: () = msg_send![effect_view, setBlendingMode: 0i64]; // behindWindow
                let _: () = msg_send![effect_view, setState: 1i64]; // active
                let _: () = msg_send![effect_view, setWantsLayer: YES];

                // Round corners
                let layer: *mut Object = msg_send![effect_view, layer];
                if !layer.is_null() {
                    let _: () = msg_send![layer, setCornerRadius: (PILL_HEIGHT / 2.0) as f64];
                    let _: () = msg_send![layer, setMasksToBounds: YES];
                }

                let _: () = msg_send![content_view, addSubview: effect_view];

                // Red dot view (8x8, vertically centered)
                let dot_size: f64 = 8.0;
                let dot_frame = NSRect::new(
                    NSPoint::new(10.0, (PILL_HEIGHT - dot_size) / 2.0),
                    NSSize::new(dot_size, dot_size),
                );
                let dot_view: *mut Object = msg_send![class!(NSView), alloc];
                let dot_view: *mut Object = msg_send![dot_view, initWithFrame: dot_frame];
                let _: () = msg_send![dot_view, setWantsLayer: YES];
                let dot_layer: *mut Object = msg_send![dot_view, layer];
                if !dot_layer.is_null() {
                    let _: () = msg_send![dot_layer, setCornerRadius: (dot_size / 2.0) as f64];
                    let red: *mut Object = msg_send![class!(NSColor), redColor];
                    let cg_red: *mut Object = msg_send![red, CGColor];
                    let _: () = msg_send![dot_layer, setBackgroundColor: cg_red];
                }
                let _: () = msg_send![effect_view, addSubview: dot_view];

                // Text label (vertically centered with proper line height)
                let text_x: f64 = 24.0;
                let text_h: f64 = 16.0;
                let text_frame = NSRect::new(
                    NSPoint::new(text_x, (PILL_HEIGHT - text_h) / 2.0),
                    NSSize::new(PILL_WIDTH - text_x - 10.0, text_h),
                );
                let text_field: *mut Object = msg_send![class!(NSTextField), alloc];
                let text_field: *mut Object = msg_send![text_field, initWithFrame: text_frame];
                let _: () = msg_send![text_field, setBezeled: NO];
                let _: () = msg_send![text_field, setDrawsBackground: NO];
                let _: () = msg_send![text_field, setEditable: NO];
                let _: () = msg_send![text_field, setSelectable: NO];

                let white_alpha: *mut Object = msg_send![class!(NSColor),
                    colorWithWhite: 1.0f64 alpha: 0.9f64];
                let _: () = msg_send![text_field, setTextColor: white_alpha];

                let font: *mut Object = msg_send![class!(NSFont), systemFontOfSize: 11.0f64];
                let _: () = msg_send![text_field, setFont: font];

                let ns_str = ns_string("Recording...");
                let _: () = msg_send![text_field, setStringValue: ns_str];

                let _: () = msg_send![effect_view, addSubview: text_field];

                // Start hidden
                let _: () = msg_send![panel, orderOut: std::ptr::null::<Object>()];

                Some(Self {
                    panel,
                    text_field,
                    dot_view,
                    visible: AtomicBool::new(false),
                })
            }
        }

        pub fn update_state(&self, state: TrayIconState) {
            unsafe {
                match state {
                    TrayIconState::Idle => {
                        if self.visible.swap(false, Ordering::SeqCst) {
                            let _: () =
                                msg_send![self.panel, orderOut: std::ptr::null::<Object>()];
                        }
                    }
                    TrayIconState::Recording => {
                        // Position near cursor
                        self.position_near_cursor();
                        let ns_str = ns_string("Recording...");
                        let _: () = msg_send![self.text_field, setStringValue: ns_str];
                        // Red dot
                        let dot_layer: *mut Object = msg_send![self.dot_view, layer];
                        if !dot_layer.is_null() {
                            let red: *mut Object = msg_send![class!(NSColor), redColor];
                            let cg_red: *mut Object = msg_send![red, CGColor];
                            let _: () = msg_send![dot_layer, setBackgroundColor: cg_red];
                            // Start pulse animation
                            Self::add_pulse_animation(dot_layer);
                        }
                        if !self.visible.swap(true, Ordering::SeqCst) {
                            let _: () = msg_send![self.panel, orderFrontRegardless];
                        }
                    }
                    TrayIconState::Transcribing => {
                        let ns_str = ns_string("Transcribing...");
                        let _: () = msg_send![self.text_field, setStringValue: ns_str];
                        // Orange dot
                        let dot_layer: *mut Object = msg_send![self.dot_view, layer];
                        if !dot_layer.is_null() {
                            let orange: *mut Object = msg_send![class!(NSColor), orangeColor];
                            let cg_orange: *mut Object = msg_send![orange, CGColor];
                            let _: () = msg_send![dot_layer, setBackgroundColor: cg_orange];
                            // Remove pulse for transcribing state
                            let key = ns_string("pulse");
                            let _: () = msg_send![dot_layer, removeAnimationForKey: key];
                        }
                        if !self.visible.load(Ordering::SeqCst) {
                            self.visible.store(true, Ordering::SeqCst);
                            let _: () = msg_send![self.panel, orderFrontRegardless];
                        }
                    }
                }
            }
        }

        unsafe fn add_pulse_animation(layer: *mut Object) {
            if layer.is_null() {
                return;
            }
            let anim: *mut Object = msg_send![class!(CABasicAnimation),
                animationWithKeyPath: ns_string("opacity")];
            let from: *mut Object = msg_send![class!(NSNumber), numberWithFloat: 1.0f32];
            let to: *mut Object = msg_send![class!(NSNumber), numberWithFloat: 0.3f32];
            let _: () = msg_send![anim, setFromValue: from];
            let _: () = msg_send![anim, setToValue: to];
            let _: () = msg_send![anim, setDuration: 0.8f64];
            let _: () = msg_send![anim, setAutoreverses: YES];
            let _: () = msg_send![anim, setRepeatCount: f32::MAX];
            let key = ns_string("pulse");
            let _: () = msg_send![layer, addAnimation: anim forKey: key];
        }

        unsafe fn position_near_cursor(&self) {
            // Get cursor position (screen coordinates, bottom-left origin)
            let mouse_loc: NSPoint = msg_send![class!(NSEvent), mouseLocation];

            // Find the screen containing the cursor
            let screens: *mut Object = msg_send![class!(NSScreen), screens];
            let screen_count: u64 = msg_send![screens, count];
            let mut target_frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(1920.0, 1080.0),
            );

            for i in 0..screen_count {
                let screen: *mut Object = msg_send![screens, objectAtIndex: i];
                let frame: NSRect = msg_send![screen, frame];
                if mouse_loc.x >= frame.origin.x
                    && mouse_loc.x <= frame.origin.x + frame.size.width
                    && mouse_loc.y >= frame.origin.y
                    && mouse_loc.y <= frame.origin.y + frame.size.height
                {
                    target_frame = frame;
                    break;
                }
            }

            // Position below cursor, centered, clamped to screen
            let mut x = mouse_loc.x - PILL_WIDTH / 2.0;
            let mut y = mouse_loc.y - PILL_HEIGHT - CURSOR_OFFSET_Y;

            // Clamp to screen bounds
            x = x
                .max(target_frame.origin.x)
                .min(target_frame.origin.x + target_frame.size.width - PILL_WIDTH);
            y = y
                .max(target_frame.origin.y)
                .min(target_frame.origin.y + target_frame.size.height - PILL_HEIGHT);

            let origin = NSPoint::new(x, y);
            let _: () = msg_send![self.panel, setFrameOrigin: origin];
        }
    }

    impl Drop for OverlayWindow {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.panel, orderOut: std::ptr::null::<Object>()];
                let _: () = msg_send![self.panel, close];
            }
        }
    }

    /// Create an NSString from a Rust &str (null-terminated for ObjC).
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
    use super::*;

    pub struct OverlayWindow;

    impl OverlayWindow {
        pub fn new() -> Option<Self> {
            Some(Self)
        }

        pub fn update_state(&self, _state: TrayIconState) {}
    }
}

pub use platform::OverlayWindow;
