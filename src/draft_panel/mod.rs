#[derive(Debug, Clone)]
pub enum DraftPanelEvent {
    Show,
    Hide,
    Clear,
    SetText(String),
    AppendText(String),
}

#[cfg(target_os = "macos")]
mod platform {
    use cocoa::appkit::NSPasteboardTypeString;
    use cocoa::appkit::NSWindowStyleMask;
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSPoint, NSRange, NSRect, NSSize, NSString};
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    const PANEL_WIDTH: f64 = 600.0;
    const PANEL_HEIGHT: f64 = 170.0;
    const NS_BACKING_STORE_BUFFERED: u64 = 2;
    const AUTORESIZE_WIDTH_HEIGHT: u64 = 18;
    const AUTORESIZE_BOTTOM_RIGHT: u64 = 33;
    const AUTORESIZE_BOTTOM_LEFT: u64 = 36;
    const AUTORESIZE_BOTTOM_WIDTH: u64 = 34;
    const COMMAND_KEY_MASK: u64 = 1 << 20;

    enum TtsUiMessage {
        Completed {
            target: usize,
            path: PathBuf,
            duration_secs: Option<f64>,
        },
        Failed {
            target: usize,
            error: String,
        },
    }

    fn tts_ui_queue() -> &'static Mutex<Vec<TtsUiMessage>> {
        static Q: OnceLock<Mutex<Vec<TtsUiMessage>>> = OnceLock::new();
        Q.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn push_tts_ui_message(msg: TtsUiMessage) {
        if let Ok(mut q) = tts_ui_queue().lock() {
            q.push(msg);
        }
    }

    fn take_tts_messages_for(target: usize) -> Vec<TtsUiMessage> {
        let mut out = Vec::new();
        if let Ok(mut q) = tts_ui_queue().lock() {
            let mut i = 0usize;
            while i < q.len() {
                let matched = match &q[i] {
                    TtsUiMessage::Completed { target: t, .. } => *t == target,
                    TtsUiMessage::Failed { target: t, .. } => *t == target,
                };
                if matched {
                    out.push(q.remove(i));
                } else {
                    i += 1;
                }
            }
        }
        out
    }

    pub struct DraftPanel {
        window: id,
        text_view: id,
        _action_target: id,
        visible: AtomicBool,
    }

    unsafe impl Send for DraftPanel {}
    unsafe impl Sync for DraftPanel {}

    fn action_target_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) = ClassDecl::new("OpenFlowDraftActionTarget", class!(NSObject)) {
                decl.add_ivar::<id>("textView");
                decl.add_ivar::<id>("ttsButton");
                decl.add_ivar::<id>("playButton");
                decl.add_ivar::<id>("seekSlider");
                decl.add_ivar::<id>("durationLabel");
                decl.add_ivar::<id>("audioPlayer");
                decl.add_ivar::<id>("audioPath");
                decl.add_ivar::<id>("playbackTimer");
                decl.add_method(
                    sel!(copyAllClicked:),
                    copy_all_clicked as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(ttsClicked:),
                    tts_clicked as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(playPauseClicked:),
                    play_pause_clicked as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(seekChanged:),
                    seek_changed as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(playbackTick:),
                    playback_tick as extern "C" fn(&Object, Sel, id),
                );
                decl.register() as *const Class as usize
            } else {
                class!(OpenFlowDraftActionTarget) as *const Class as usize
            }
        });
        ptr as *const Class
    }

    fn draft_text_view_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) = ClassDecl::new("OpenFlowDraftTextView", class!(NSTextView)) {
                decl.add_method(
                    sel!(performKeyEquivalent:),
                    draft_text_view_perform_key_equivalent
                        as extern "C" fn(&Object, Sel, id) -> bool,
                );
                decl.register() as *const Class as usize
            } else {
                class!(OpenFlowDraftTextView) as *const Class as usize
            }
        });
        ptr as *const Class
    }

    extern "C" fn draft_text_view_perform_key_equivalent(
        this: &Object,
        _cmd: Sel,
        event: id,
    ) -> bool {
        unsafe {
            let flags: u64 = msg_send![event, modifierFlags];
            if (flags & COMMAND_KEY_MASK) == 0 {
                return msg_send![super(this, class!(NSTextView)), performKeyEquivalent: event];
            }

            let chars: id = msg_send![event, charactersIgnoringModifiers];
            if chars == nil {
                return msg_send![super(this, class!(NSTextView)), performKeyEquivalent: event];
            }
            let chars_ptr: *const i8 = msg_send![chars, UTF8String];
            if chars_ptr.is_null() {
                return msg_send![super(this, class!(NSTextView)), performKeyEquivalent: event];
            }

            let key = std::ffi::CStr::from_ptr(chars_ptr)
                .to_string_lossy()
                .chars()
                .next()
                .unwrap_or('\0')
                .to_ascii_lowercase();

            match key {
                'a' => {
                    let _: () = msg_send![this, selectAll: nil];
                    true
                }
                'c' => {
                    let _: () = msg_send![this, copy: nil];
                    true
                }
                'v' => {
                    let _: () = msg_send![this, paste: nil];
                    true
                }
                'x' => {
                    let _: () = msg_send![this, cut: nil];
                    true
                }
                _ => msg_send![super(this, class!(NSTextView)), performKeyEquivalent: event],
            }
        }
    }

    extern "C" fn copy_all_clicked(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let text_view: id = *this.get_ivar("textView");
            if text_view != nil {
                copy_all_text(text_view);
            }
        }
    }

    extern "C" fn tts_clicked(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let text_view: id = *this.get_ivar("textView");
            let tts_button: id = *this.get_ivar("ttsButton");
            let play_button: id = *this.get_ivar("playButton");
            let duration_label: id = *this.get_ivar("durationLabel");
            let seek_slider: id = *this.get_ivar("seekSlider");

            if text_view == nil || tts_button == nil {
                return;
            }

            let enabled: bool = msg_send![tts_button, isEnabled];
            if !enabled {
                return;
            }

            let current: id = msg_send![text_view, string];
            let current_str: *const i8 = msg_send![current, UTF8String];
            let text = if current_str.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(current_str)
                    .to_string_lossy()
                    .trim()
                    .to_string()
            };

            if text.is_empty() {
                return;
            }

            let _: () = msg_send![tts_button, setEnabled: NO];
            let _: () = msg_send![tts_button, setTitle: NSString::alloc(nil).init_str("转换中...")];
            if play_button != nil {
                let _: () = msg_send![play_button, setEnabled: NO];
                let _: () = msg_send![play_button, setTitle: NSString::alloc(nil).init_str("播放")];
            }
            if seek_slider != nil {
                let _: () = msg_send![seek_slider, setDoubleValue: 0.0f64];
            }
            if duration_label != nil {
                let _: () = msg_send![duration_label, setStringValue: NSString::alloc(nil).init_str("处理中")];
            }

            let target = this as *const Object as usize;
            std::thread::spawn(move || {
                let msg = match crate::tts::synthesize_to_mp3(&text) {
                    Ok(path) => TtsUiMessage::Completed {
                        target,
                        duration_secs: crate::tts::audio_duration_secs(&path),
                        path,
                    },
                    Err(error) => TtsUiMessage::Failed { target, error },
                };
                push_tts_ui_message(msg);
            });
        }
    }

    extern "C" fn play_pause_clicked(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let play_button: id = *this.get_ivar("playButton");
            let duration_label: id = *this.get_ivar("durationLabel");
            let mut player: id = *this.get_ivar("audioPlayer");

            if player == nil {
                let audio_path: id = *this.get_ivar("audioPath");
                if audio_path != nil {
                    let path_ptr: *const i8 = msg_send![audio_path, UTF8String];
                    if !path_ptr.is_null() {
                        let path = PathBuf::from(
                            std::ffi::CStr::from_ptr(path_ptr)
                                .to_string_lossy()
                                .to_string(),
                        );
                        if load_audio_player(this, &path, duration_label).is_ok() {
                            player = *this.get_ivar("audioPlayer");
                        }
                    }
                }
            }

            if player == nil {
                return;
            }

            let is_playing: bool = msg_send![player, isPlaying];
            if is_playing {
                let _: () = msg_send![player, pause];
                let _: () = msg_send![play_button, setTitle: NSString::alloc(nil).init_str("播放")];
            } else {
                let _: bool = msg_send![player, play];
                let _: () = msg_send![play_button, setTitle: NSString::alloc(nil).init_str("暂停")];
            }
        }
    }

    extern "C" fn seek_changed(this: &Object, _cmd: Sel, sender: id) {
        unsafe {
            let player: id = *this.get_ivar("audioPlayer");
            if player == nil {
                return;
            }
            let duration: f64 = msg_send![player, duration];
            if duration <= 0.0 {
                return;
            }
            let value: f64 = msg_send![sender, doubleValue];
            let target = (value.clamp(0.0, 1.0)) * duration;
            let _: () = msg_send![player, setCurrentTime: target];
        }
    }

    extern "C" fn playback_tick(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            apply_pending_tts_messages(this);

            let player: id = *this.get_ivar("audioPlayer");
            let slider: id = *this.get_ivar("seekSlider");
            let play_button: id = *this.get_ivar("playButton");
            if player == nil || slider == nil {
                return;
            }
            let duration: f64 = msg_send![player, duration];
            if duration <= 0.0 {
                return;
            }
            let current: f64 = msg_send![player, currentTime];
            let _: () = msg_send![slider, setDoubleValue: (current / duration).clamp(0.0, 1.0)];

            let is_playing: bool = msg_send![player, isPlaying];
            if !is_playing && (duration - current).abs() < 0.05 {
                let _: () = msg_send![slider, setDoubleValue: 0.0f64];
                let _: () = msg_send![player, setCurrentTime: 0.0f64];
                if play_button != nil {
                    let _: () =
                        msg_send![play_button, setTitle: NSString::alloc(nil).init_str("播放")];
                }
            }
        }
    }

    unsafe fn apply_pending_tts_messages(this: &Object) {
        let target = this as *const Object as usize;
        let pending = take_tts_messages_for(target);
        if pending.is_empty() {
            return;
        }

        let tts_button: id = *this.get_ivar("ttsButton");
        let play_button: id = *this.get_ivar("playButton");
        let duration_label: id = *this.get_ivar("durationLabel");

        for item in pending {
            match item {
                TtsUiMessage::Completed {
                    path,
                    duration_secs,
                    ..
                } => {
                    let path_ns = NSString::alloc(nil).init_str(path.to_string_lossy().as_ref());
                    set_obj_ivar(this, "audioPath", path_ns);

                    if load_audio_player(this, &path, duration_label).is_ok() {
                        if play_button != nil {
                            let _: () = msg_send![play_button, setEnabled: YES];
                            let _: () = msg_send![play_button, setTitle: NSString::alloc(nil).init_str("播放")];
                        }
                        if duration_label != nil {
                            let text = format_duration(duration_secs.unwrap_or(0.0));
                            let ns = NSString::alloc(nil).init_str(&text);
                            let _: () = msg_send![duration_label, setStringValue: ns];
                        }
                    } else if duration_label != nil {
                        let _: () = msg_send![duration_label, setStringValue: NSString::alloc(nil).init_str("加载失败")];
                    }

                    if tts_button != nil {
                        let _: () = msg_send![tts_button, setEnabled: YES];
                        let _: () = msg_send![tts_button, setTitle: NSString::alloc(nil).init_str("转音频")];
                    }
                }
                TtsUiMessage::Failed { error, .. } => {
                    eprintln!("⚠️  文本转音频失败: {}", error);
                    if duration_label != nil {
                        let _: () = msg_send![duration_label, setStringValue: NSString::alloc(nil).init_str("转换失败")];
                    }
                    if tts_button != nil {
                        let _: () = msg_send![tts_button, setEnabled: YES];
                        let _: () =
                            msg_send![tts_button, setTitle: NSString::alloc(nil).init_str("重试")];
                    }
                }
            }
        }
    }

    unsafe fn copy_all_text(text_view: id) {
        let text_storage: id = msg_send![text_view, textStorage];
        let length: u64 = msg_send![text_storage, length];
        let _: () = msg_send![text_view, setSelectedRange: NSRange::new(0, length)];
        let text_value: id = msg_send![text_view, string];
        let pasteboard: id = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: i64 = msg_send![pasteboard, clearContents];
        let _: bool = msg_send![pasteboard,
            setString: text_value
            forType: NSPasteboardTypeString
        ];
    }

    unsafe fn load_audio_player(
        this: &Object,
        path: &Path,
        duration_label: id,
    ) -> Result<(), String> {
        let path_ns = NSString::alloc(nil).init_str(path.to_string_lossy().as_ref());
        let url: id = msg_send![class!(NSURL), fileURLWithPath: path_ns];
        let mut err: id = nil;
        let player_alloc: id = msg_send![class!(AVAudioPlayer), alloc];
        let player: id = msg_send![player_alloc, initWithContentsOfURL: url error: &mut err];
        if player == nil {
            return Err("AVAudioPlayer 初始化失败".to_string());
        }
        let _: bool = msg_send![player, prepareToPlay];

        let slider: id = *this.get_ivar("seekSlider");
        if slider != nil {
            let _: () = msg_send![slider, setDoubleValue: 0.0f64];
        }

        if duration_label != nil {
            let duration: f64 = msg_send![player, duration];
            let title = NSString::alloc(nil).init_str(&format_duration(duration));
            let _: () = msg_send![duration_label, setStringValue: title];
        }

        set_obj_ivar(this, "audioPlayer", player);

        Ok(())
    }

    fn format_duration(seconds: f64) -> String {
        let total = seconds.max(0.0).round() as u64;
        let m = total / 60;
        let s = total % 60;
        format!("{:02}:{:02}", m, s)
    }

    unsafe fn set_obj_ivar(this: &Object, key: &str, value: id) {
        let key_ns = NSString::alloc(nil).init_str(key);
        let _: () = msg_send![this, setValue: value forKey: key_ns];
    }

    unsafe fn add_edit_menu_item(menu: id, title: &str, action: Sel, key: &str) {
        let item: id = msg_send![class!(NSMenuItem), alloc];
        let title_ns = NSString::alloc(nil).init_str(title);
        let key_ns = NSString::alloc(nil).init_str(key);
        let item: id = msg_send![item,
            initWithTitle: title_ns
            action: action
            keyEquivalent: key_ns
        ];
        let _: () = msg_send![item, setKeyEquivalentModifierMask: COMMAND_KEY_MASK];
        let _: () = msg_send![menu, addItem: item];
    }

    unsafe fn ensure_edit_shortcuts_menu() {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let existing: id = msg_send![app, mainMenu];
        if existing != nil {
            return;
        }

        let main_menu: id = msg_send![class!(NSMenu), alloc];
        let main_menu: id =
            msg_send![main_menu, initWithTitle: NSString::alloc(nil).init_str("Main")];

        let edit_root: id = msg_send![class!(NSMenuItem), alloc];
        let edit_root: id = msg_send![edit_root,
            initWithTitle: NSString::alloc(nil).init_str("Edit")
            action: nil
            keyEquivalent: NSString::alloc(nil).init_str("")
        ];

        let edit_menu: id = msg_send![class!(NSMenu), alloc];
        let edit_menu: id =
            msg_send![edit_menu, initWithTitle: NSString::alloc(nil).init_str("Edit")];

        add_edit_menu_item(edit_menu, "Select All", sel!(selectAll:), "a");
        add_edit_menu_item(edit_menu, "Copy", sel!(copy:), "c");
        add_edit_menu_item(edit_menu, "Paste", sel!(paste:), "v");

        let _: () = msg_send![edit_root, setSubmenu: edit_menu];
        let _: () = msg_send![main_menu, addItem: edit_root];
        let _: () = msg_send![app, setMainMenu: main_menu];
    }

    impl DraftPanel {
        pub fn new() -> Option<Self> {
            unsafe {
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                let screen_frame: NSRect = msg_send![screen, frame];
                let x = (screen_frame.size.width - PANEL_WIDTH) / 2.0;
                let y = screen_frame.size.height * 0.6;
                let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(PANEL_WIDTH, PANEL_HEIGHT));

                let style_mask = NSWindowStyleMask::NSTitledWindowMask
                    | NSWindowStyleMask::NSClosableWindowMask
                    | NSWindowStyleMask::NSResizableWindowMask;

                let window: id = msg_send![class!(NSWindow), alloc];
                let window: id = msg_send![window,
                    initWithContentRect: frame
                    styleMask: style_mask
                    backing: NS_BACKING_STORE_BUFFERED
                    defer: NO
                ];
                if window == nil {
                    return None;
                }

                let title = NSString::alloc(nil).init_str("录音草稿");
                let _: () = msg_send![window, setTitle: title];
                let _: () = msg_send![window, setReleasedWhenClosed: NO];

                let content_view: id = msg_send![window, contentView];
                let content_frame: NSRect = msg_send![content_view, bounds];
                let toolbar_height = 40.0f64;
                let scroll_frame = NSRect::new(
                    NSPoint::new(0.0, toolbar_height),
                    NSSize::new(
                        content_frame.size.width,
                        content_frame.size.height - toolbar_height,
                    ),
                );

                let scroll_view: id = msg_send![class!(NSScrollView), alloc];
                let scroll_view: id = msg_send![scroll_view, initWithFrame: scroll_frame];
                let _: () = msg_send![scroll_view, setHasVerticalScroller: YES];
                let _: () = msg_send![scroll_view, setHasHorizontalScroller: NO];
                let _: () = msg_send![scroll_view, setAutoresizingMask: AUTORESIZE_WIDTH_HEIGHT];

                let text_view_class = draft_text_view_class();
                let text_view: id = msg_send![text_view_class, alloc];
                let text_view: id = msg_send![text_view, initWithFrame: scroll_frame];
                let _: () = msg_send![text_view, setEditable: YES];
                let _: () = msg_send![text_view, setSelectable: YES];
                let _: () = msg_send![text_view, setRichText: NO];
                let inset = NSSize::new(10.0, 10.0);
                let _: () = msg_send![text_view, setTextContainerInset: inset];
                let _: () = msg_send![text_view, setAutoresizingMask: AUTORESIZE_WIDTH_HEIGHT];
                let font: id = msg_send![class!(NSFont), systemFontOfSize: 14.0f64];
                let _: () = msg_send![text_view, setFont: font];

                let play_button_frame =
                    NSRect::new(NSPoint::new(10.0, 8.0), NSSize::new(56.0, 24.0));
                let play_button: id = msg_send![class!(NSButton), alloc];
                let play_button: id = msg_send![play_button, initWithFrame: play_button_frame];
                let _: () = msg_send![play_button, setTitle: NSString::alloc(nil).init_str("播放")];
                let _: () = msg_send![play_button, setBezelStyle: 1i64];
                let _: () = msg_send![play_button, setEnabled: NO];
                let _: () = msg_send![play_button, setAutoresizingMask: AUTORESIZE_BOTTOM_LEFT];

                let slider_frame = NSRect::new(NSPoint::new(72.0, 11.0), NSSize::new(220.0, 18.0));
                let seek_slider: id = msg_send![class!(NSSlider), alloc];
                let seek_slider: id = msg_send![seek_slider, initWithFrame: slider_frame];
                let _: () = msg_send![seek_slider, setMinValue: 0.0f64];
                let _: () = msg_send![seek_slider, setMaxValue: 1.0f64];
                let _: () = msg_send![seek_slider, setDoubleValue: 0.0f64];
                let _: () = msg_send![seek_slider, setAutoresizingMask: AUTORESIZE_BOTTOM_WIDTH];

                let duration_frame =
                    NSRect::new(NSPoint::new(298.0, 10.0), NSSize::new(52.0, 20.0));
                let duration_label: id = msg_send![class!(NSTextField), alloc];
                let duration_label: id = msg_send![duration_label, initWithFrame: duration_frame];
                let _: () = msg_send![duration_label, setBezeled: NO];
                let _: () = msg_send![duration_label, setDrawsBackground: NO];
                let _: () = msg_send![duration_label, setEditable: NO];
                let _: () = msg_send![duration_label, setSelectable: NO];
                let _: () = msg_send![duration_label, setStringValue: NSString::alloc(nil).init_str("00:00")];
                let _: () = msg_send![duration_label, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let tts_button_frame = NSRect::new(
                    NSPoint::new(content_frame.size.width - 168.0, 8.0),
                    NSSize::new(74.0, 24.0),
                );
                let tts_button: id = msg_send![class!(NSButton), alloc];
                let tts_button: id = msg_send![tts_button, initWithFrame: tts_button_frame];
                let _: () =
                    msg_send![tts_button, setTitle: NSString::alloc(nil).init_str("转音频")];
                let _: () = msg_send![tts_button, setBezelStyle: 1i64];
                let _: () = msg_send![tts_button, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let copy_button_frame = NSRect::new(
                    NSPoint::new(content_frame.size.width - 88.0, 8.0),
                    NSSize::new(72.0, 24.0),
                );
                let copy_button: id = msg_send![class!(NSButton), alloc];
                let copy_button: id = msg_send![copy_button, initWithFrame: copy_button_frame];
                let copy_title = NSString::alloc(nil).init_str("复制");
                let _: () = msg_send![copy_button, setTitle: copy_title];
                let _: () = msg_send![copy_button, setBezelStyle: 1i64];
                let _: () = msg_send![copy_button, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let target_class = action_target_class();
                let action_target: id = msg_send![target_class, new];
                (&mut *action_target).set_ivar("textView", text_view);
                (&mut *action_target).set_ivar("ttsButton", tts_button);
                (&mut *action_target).set_ivar("playButton", play_button);
                (&mut *action_target).set_ivar("seekSlider", seek_slider);
                (&mut *action_target).set_ivar("durationLabel", duration_label);
                (&mut *action_target).set_ivar("audioPlayer", nil);
                (&mut *action_target).set_ivar("audioPath", nil);
                let tick_timer: id = msg_send![class!(NSTimer),
                    scheduledTimerWithTimeInterval: 0.2f64
                    target: action_target
                    selector: sel!(playbackTick:)
                    userInfo: nil
                    repeats: YES
                ];
                (&mut *action_target).set_ivar("playbackTimer", tick_timer);

                let _: () = msg_send![copy_button, setTarget: action_target];
                let _: () = msg_send![copy_button, setAction: sel!(copyAllClicked:)];
                let _: () = msg_send![tts_button, setTarget: action_target];
                let _: () = msg_send![tts_button, setAction: sel!(ttsClicked:)];
                let _: () = msg_send![play_button, setTarget: action_target];
                let _: () = msg_send![play_button, setAction: sel!(playPauseClicked:)];
                let _: () = msg_send![seek_slider, setTarget: action_target];
                let _: () = msg_send![seek_slider, setAction: sel!(seekChanged:)];

                let _: () = msg_send![scroll_view, setDocumentView: text_view];
                let _: () = msg_send![content_view, addSubview: scroll_view];
                let _: () = msg_send![content_view, addSubview: play_button];
                let _: () = msg_send![content_view, addSubview: seek_slider];
                let _: () = msg_send![content_view, addSubview: duration_label];
                let _: () = msg_send![content_view, addSubview: tts_button];
                let _: () = msg_send![content_view, addSubview: copy_button];
                let _: () = msg_send![window, orderOut: nil];

                Some(Self {
                    window,
                    text_view,
                    _action_target: action_target,
                    visible: AtomicBool::new(false),
                })
            }
        }

        pub fn show(&self) {
            unsafe {
                ensure_edit_shortcuts_menu();
                if !self.visible.swap(true, Ordering::SeqCst) {
                    let _: () = msg_send![self.window, makeKeyAndOrderFront: nil];
                }
                let app: id = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: YES];
                let _: () = msg_send![self.window, makeFirstResponder: self.text_view];
            }
        }

        pub fn hide(&self) {
            unsafe {
                if self.visible.swap(false, Ordering::SeqCst) {
                    let _: () = msg_send![self.window, orderOut: nil];
                }
            }
        }

        pub fn is_key_window(&self) -> bool {
            unsafe {
                let is_key: bool = msg_send![self.window, isKeyWindow];
                is_key
            }
        }

        pub fn set_text(&self, text: &str) {
            unsafe {
                let ns_str = NSString::alloc(nil).init_str(text);
                let _: () = msg_send![self.text_view, setString: ns_str];
            }
        }

        pub fn append_text(&self, text: &str) {
            unsafe {
                let current: id = msg_send![self.text_view, string];
                let current_str: *const i8 = msg_send![current, UTF8String];
                let base = if current_str.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(current_str)
                        .to_string_lossy()
                        .into_owned()
                };
                let merged = base + text;
                let ns_str = NSString::alloc(nil).init_str(&merged);
                let _: () = msg_send![self.text_view, setString: ns_str];

                let text_storage: id = msg_send![self.text_view, textStorage];
                let length: u64 = msg_send![text_storage, length];
                let range = NSRange::new(length, 0);
                let _: () = msg_send![self.text_view, scrollRangeToVisible: range];
            }
        }

        pub fn clear(&self) {
            self.set_text("");
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub struct DraftPanel;

    impl DraftPanel {
        pub fn new() -> Option<Self> {
            Some(Self)
        }

        pub fn show(&self) {}

        pub fn hide(&self) {}

        pub fn is_key_window(&self) -> bool {
            false
        }

        pub fn set_text(&self, _text: &str) {}

        pub fn append_text(&self, _text: &str) {}

        pub fn clear(&self) {}
    }
}

pub use platform::DraftPanel;
