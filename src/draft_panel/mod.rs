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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;

    use crate::draft_transform::DraftTransformPanel;

    const PANEL_WIDTH: f64 = 340.0;
    const PANEL_EXPANDED_WIDTH: f64 = 860.0;
    const PANEL_HEIGHT: f64 = 410.0;
    const PANEL_SPLIT_GAP: f64 = 10.0;
    const PANEL_MIN_RIGHT_WIDTH: f64 = 260.0;
    const PANEL_MIN_LEFT_WIDTH: f64 = 240.0;
    const PANEL_TOOLBAR_HEIGHT: f64 = 40.0;
    const NS_BACKING_STORE_BUFFERED: u64 = 2;
    const AUTORESIZE_WIDTH_HEIGHT: u64 = 18;
    const AUTORESIZE_BOTTOM_RIGHT: u64 = 33;
    const AUTORESIZE_BOTTOM_LEFT: u64 = 36;
    const COMMAND_KEY_MASK: u64 = 1 << 20;
    const NS_SWITCH_BUTTON: i64 = 3;

    pub struct DraftPanel {
        window: id,
        content_view: id,
        text_view: id,
        scroll_view: id,
        toggle_button: id,
        transform_button: id,
        clear_button: id,
        copy_button: id,
        transform_panel: Option<Box<DraftTransformPanel>>,
        transform_expanded: Box<AtomicBool>,
        _action_target: id,
        _window_delegate: id,
        draft_mode_active: std::sync::Arc<AtomicBool>,
        visible: Box<AtomicBool>,
        close_requested: Box<AtomicBool>,
    }

    unsafe impl Send for DraftPanel {}
    unsafe impl Sync for DraftPanel {}

    fn action_target_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) = ClassDecl::new("OpenFlowDraftActionTarget", class!(NSObject)) {
                decl.add_ivar::<id>("textView");
                decl.add_ivar::<usize>("draftModePtr");
                decl.add_ivar::<usize>("transformPanelPtr");
                decl.add_ivar::<usize>("transformExpandedPtr");
                decl.add_ivar::<id>("window");
                decl.add_ivar::<id>("contentView");
                decl.add_ivar::<id>("scrollView");
                decl.add_ivar::<id>("toggleButton");
                decl.add_ivar::<id>("transformButton");
                decl.add_ivar::<id>("clearButton");
                decl.add_ivar::<id>("copyButton");
                decl.add_method(
                    sel!(clearClicked:),
                    clear_clicked as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(toggleDraftMode:),
                    toggle_draft_mode as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(copyAllClicked:),
                    copy_all_clicked as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(showTransformClicked:),
                    show_transform_clicked as extern "C" fn(&Object, Sel, id),
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

    fn draft_window_delegate_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) = ClassDecl::new("OpenFlowDraftWindowDelegate", class!(NSObject))
            {
                decl.add_ivar::<usize>("visiblePtr");
                decl.add_ivar::<usize>("closeRequestedPtr");
                decl.add_ivar::<id>("contentView");
                decl.add_ivar::<id>("scrollView");
                decl.add_ivar::<id>("toggleButton");
                decl.add_ivar::<id>("transformButton");
                decl.add_ivar::<id>("clearButton");
                decl.add_ivar::<id>("copyButton");
                decl.add_ivar::<usize>("transformPanelPtr");
                decl.add_ivar::<usize>("transformExpandedPtr");
                decl.add_method(
                    sel!(windowWillClose:),
                    draft_window_will_close as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(windowDidResize:),
                    draft_window_did_resize as extern "C" fn(&Object, Sel, id),
                );
                decl.register() as *const Class as usize
            } else {
                class!(OpenFlowDraftWindowDelegate) as *const Class as usize
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

    extern "C" fn clear_clicked(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let text_view: id = *this.get_ivar("textView");
            if text_view != nil {
                let empty = NSString::alloc(nil).init_str("");
                let _: () = msg_send![text_view, setString: empty];
            }
        }
    }

    extern "C" fn show_transform_clicked(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let text_view: id = *this.get_ivar("textView");
            let transform_panel_ptr: usize = *this.get_ivar("transformPanelPtr");
            let transform_expanded_ptr: usize = *this.get_ivar("transformExpandedPtr");
            if text_view == nil || transform_panel_ptr == 0 || transform_expanded_ptr == 0 {
                return;
            }

            let current_text = text_view_string(text_view);
            let transform_panel = transform_panel_ptr as *const DraftTransformPanel;
            (*transform_panel).set_source_text(&current_text);

            let transform_expanded = transform_expanded_ptr as *const AtomicBool;
            let next = !(*transform_expanded).load(Ordering::SeqCst);
            (*transform_expanded).store(next, Ordering::SeqCst);

            let window: id = *this.get_ivar("window");
            apply_transform_window_width(window, next);

            relayout_from_action_target(this, next);
        }
    }

    unsafe fn relayout_from_action_target(this: &Object, expanded: bool) {
        let content_view: id = *this.get_ivar("contentView");
        let scroll_view: id = *this.get_ivar("scrollView");
        let toggle_button: id = *this.get_ivar("toggleButton");
        let transform_button: id = *this.get_ivar("transformButton");
        let clear_button: id = *this.get_ivar("clearButton");
        let copy_button: id = *this.get_ivar("copyButton");
        let transform_panel_ptr: usize = *this.get_ivar("transformPanelPtr");
        let transform_panel = if transform_panel_ptr == 0 {
            None
        } else {
            Some(&*(transform_panel_ptr as *const DraftTransformPanel))
        };

        relayout_draft_content(
            content_view,
            scroll_view,
            toggle_button,
            transform_button,
            clear_button,
            copy_button,
            transform_panel,
            expanded,
        );
    }

    unsafe fn apply_transform_window_width(window: id, expanded: bool) {
        if window == nil {
            return;
        }
        let mut frame: NSRect = msg_send![window, frame];
        if expanded {
            if frame.size.width < PANEL_EXPANDED_WIDTH {
                frame.size.width = PANEL_EXPANDED_WIDTH;
                let _: () = msg_send![window, setFrame: frame display: YES animate: NO];
            }
        } else {
            frame.size.width = PANEL_WIDTH;
            let _: () = msg_send![window, setFrame: frame display: YES animate: NO];
        }
    }

    unsafe fn relayout_draft_content(
        content_view: id,
        scroll_view: id,
        toggle_button: id,
        transform_button: id,
        clear_button: id,
        copy_button: id,
        transform_panel: Option<&DraftTransformPanel>,
        expanded: bool,
    ) {
        if content_view == nil {
            return;
        }

        let content_frame: NSRect = msg_send![content_view, bounds];
        let total_width = content_frame.size.width;
        let total_height = content_frame.size.height;

        let left_width = if expanded {
            let usable_width = (total_width - PANEL_SPLIT_GAP).max(0.0);
            if usable_width <= PANEL_MIN_RIGHT_WIDTH {
                total_width
            } else {
                let max_right_width =
                    (usable_width - PANEL_MIN_LEFT_WIDTH).max(PANEL_MIN_RIGHT_WIDTH);
                let right_width =
                    (usable_width * 0.44).clamp(PANEL_MIN_RIGHT_WIDTH, max_right_width);
                (usable_width - right_width).max(PANEL_MIN_LEFT_WIDTH)
            }
        } else {
            total_width
        };

        let scroll_frame = NSRect::new(
            NSPoint::new(0.0, PANEL_TOOLBAR_HEIGHT),
            NSSize::new(left_width, total_height - PANEL_TOOLBAR_HEIGHT),
        );
        let _: () = msg_send![scroll_view, setFrame: scroll_frame];

        let _: () = msg_send![
            toggle_button,
            setFrame: NSRect::new(NSPoint::new(18.0, 9.0), NSSize::new(92.0, 20.0))
        ];
        let _: () = msg_send![
            transform_button,
            setFrame: NSRect::new(
                NSPoint::new(left_width - 214.0, 8.0),
                NSSize::new(70.0, 24.0)
            )
        ];
        let _: () = msg_send![
            clear_button,
            setFrame: NSRect::new(
                NSPoint::new(left_width - 136.0, 8.0),
                NSSize::new(56.0, 24.0)
            )
        ];
        let _: () = msg_send![
            copy_button,
            setFrame: NSRect::new(
                NSPoint::new(left_width - 72.0, 8.0),
                NSSize::new(56.0, 24.0)
            )
        ];

        if let Some(panel) = transform_panel {
            if expanded {
                let panel_x = left_width + PANEL_SPLIT_GAP;
                let available_width = total_width - panel_x;
                if available_width >= 220.0 {
                    panel.set_frame(panel_x, 0.0, available_width, total_height);
                    panel.set_visible(true);
                } else {
                    panel.set_visible(false);
                }
            } else {
                panel.set_visible(false);
            }
        }
    }

    extern "C" fn toggle_draft_mode(this: &Object, _cmd: Sel, sender: id) {
        unsafe {
            let draft_mode_ptr: usize = *this.get_ivar("draftModePtr");
            if draft_mode_ptr != 0 {
                let state: i64 = msg_send![sender, state];
                let enabled = state != 0;
                let draft_mode = draft_mode_ptr as *const AtomicBool;
                (*draft_mode).store(enabled, Ordering::SeqCst);
            }
        }
    }

    extern "C" fn draft_window_will_close(this: &Object, _cmd: Sel, _notification: id) {
        unsafe {
            let visible_ptr: usize = *this.get_ivar("visiblePtr");
            if visible_ptr != 0 {
                let visible = visible_ptr as *const AtomicBool;
                (*visible).store(false, Ordering::SeqCst);
            }

            let close_requested_ptr: usize = *this.get_ivar("closeRequestedPtr");
            if close_requested_ptr != 0 {
                let close_requested = close_requested_ptr as *const AtomicBool;
                (*close_requested).store(true, Ordering::SeqCst);
            }
        }
    }

    extern "C" fn draft_window_did_resize(this: &Object, _cmd: Sel, _notification: id) {
        unsafe {
            let content_view: id = *this.get_ivar("contentView");
            let scroll_view: id = *this.get_ivar("scrollView");
            let toggle_button: id = *this.get_ivar("toggleButton");
            let transform_button: id = *this.get_ivar("transformButton");
            let clear_button: id = *this.get_ivar("clearButton");
            let copy_button: id = *this.get_ivar("copyButton");
            let transform_panel_ptr: usize = *this.get_ivar("transformPanelPtr");
            let transform_expanded_ptr: usize = *this.get_ivar("transformExpandedPtr");

            if content_view == nil
                || scroll_view == nil
                || toggle_button == nil
                || transform_button == nil
                || clear_button == nil
                || copy_button == nil
                || transform_expanded_ptr == 0
            {
                return;
            }

            let expanded = (*(transform_expanded_ptr as *const AtomicBool)).load(Ordering::SeqCst);
            let transform_panel = if transform_panel_ptr == 0 {
                None
            } else {
                Some(&*(transform_panel_ptr as *const DraftTransformPanel))
            };

            relayout_draft_content(
                content_view,
                scroll_view,
                toggle_button,
                transform_button,
                clear_button,
                copy_button,
                transform_panel,
                expanded,
            );
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

    unsafe fn text_view_string(text_view: id) -> String {
        let current: id = msg_send![text_view, string];
        let current_str: *const i8 = msg_send![current, UTF8String];
        if current_str.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(current_str)
                .to_string_lossy()
                .into_owned()
        }
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
        pub fn new(draft_mode_active: std::sync::Arc<AtomicBool>) -> Option<Self> {
            unsafe {
                let ui_language = crate::common::config::Config::load()
                    .map(|config| crate::common::ui::UiLanguage::from_config(&config))
                    .unwrap_or_default();
                let visible = Box::new(AtomicBool::new(false));
                let close_requested = Box::new(AtomicBool::new(false));
                let transform_panel = DraftTransformPanel::new(ui_language).map(Box::new);
                let transform_expanded = Box::new(AtomicBool::new(false));
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

                let title = NSString::alloc(nil).init_str(ui_language.draft_panel_title());
                let _: () = msg_send![window, setTitle: title];
                let _: () = msg_send![window, setReleasedWhenClosed: NO];

                let delegate_class = draft_window_delegate_class();
                let window_delegate: id = msg_send![delegate_class, new];
                (&mut *window_delegate)
                    .set_ivar("visiblePtr", (&*visible as *const AtomicBool) as usize);
                (&mut *window_delegate).set_ivar(
                    "closeRequestedPtr",
                    (&*close_requested as *const AtomicBool) as usize,
                );
                (&mut *window_delegate).set_ivar("contentView", nil);
                (&mut *window_delegate).set_ivar("scrollView", nil);
                (&mut *window_delegate).set_ivar("toggleButton", nil);
                (&mut *window_delegate).set_ivar("transformButton", nil);
                (&mut *window_delegate).set_ivar("clearButton", nil);
                (&mut *window_delegate).set_ivar("copyButton", nil);
                (&mut *window_delegate).set_ivar("transformPanelPtr", 0usize);
                (&mut *window_delegate).set_ivar("transformExpandedPtr", 0usize);
                let _: () = msg_send![window, setDelegate: window_delegate];

                let content_view: id = msg_send![window, contentView];
                let content_frame: NSRect = msg_send![content_view, bounds];
                let scroll_frame = NSRect::new(
                    NSPoint::new(0.0, PANEL_TOOLBAR_HEIGHT),
                    NSSize::new(
                        content_frame.size.width,
                        content_frame.size.height - PANEL_TOOLBAR_HEIGHT,
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

                let clear_button_frame = NSRect::new(
                    NSPoint::new(content_frame.size.width - 136.0, 8.0),
                    NSSize::new(56.0, 24.0),
                );
                let clear_button: id = msg_send![class!(NSButton), alloc];
                let clear_button: id = msg_send![clear_button, initWithFrame: clear_button_frame];
                let _: () = msg_send![clear_button, setTitle: NSString::alloc(nil).init_str(ui_language.clear())];
                let _: () = msg_send![clear_button, setBezelStyle: 1i64];
                let _: () = msg_send![clear_button, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let copy_button_frame = NSRect::new(
                    NSPoint::new(content_frame.size.width - 72.0, 8.0),
                    NSSize::new(56.0, 24.0),
                );
                let copy_button: id = msg_send![class!(NSButton), alloc];
                let copy_button: id = msg_send![copy_button, initWithFrame: copy_button_frame];
                let _: () = msg_send![copy_button, setTitle: NSString::alloc(nil).init_str(ui_language.copy())];
                let _: () = msg_send![copy_button, setBezelStyle: 1i64];
                let _: () = msg_send![copy_button, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let transform_button_frame = NSRect::new(
                    NSPoint::new(content_frame.size.width - 214.0, 8.0),
                    NSSize::new(70.0, 24.0),
                );
                let transform_button: id = msg_send![class!(NSButton), alloc];
                let transform_button: id =
                    msg_send![transform_button, initWithFrame: transform_button_frame];
                let _: () = msg_send![transform_button, setTitle: NSString::alloc(nil).init_str(ui_language.pick("AI转换", "AI"))];
                let _: () = msg_send![transform_button, setBezelStyle: 1i64];
                let _: () =
                    msg_send![transform_button, setAutoresizingMask: AUTORESIZE_BOTTOM_RIGHT];

                let target_class = action_target_class();
                let action_target: id = msg_send![target_class, new];
                (&mut *action_target).set_ivar("textView", text_view);
                (&mut *action_target).set_ivar(
                    "draftModePtr",
                    std::sync::Arc::as_ptr(&draft_mode_active) as usize,
                );
                (&mut *action_target).set_ivar(
                    "transformPanelPtr",
                    transform_panel
                        .as_deref()
                        .map(|panel| panel as *const DraftTransformPanel as usize)
                        .unwrap_or(0),
                );
                (&mut *action_target).set_ivar(
                    "transformExpandedPtr",
                    (&*transform_expanded as *const AtomicBool) as usize,
                );
                (&mut *action_target).set_ivar("window", window);
                (&mut *action_target).set_ivar("contentView", content_view);
                (&mut *action_target).set_ivar("scrollView", scroll_view);
                (&mut *action_target).set_ivar("toggleButton", nil);
                (&mut *action_target).set_ivar("transformButton", nil);
                (&mut *action_target).set_ivar("clearButton", nil);
                (&mut *action_target).set_ivar("copyButton", nil);

                let toggle_button_frame =
                    NSRect::new(NSPoint::new(18.0, 9.0), NSSize::new(92.0, 20.0));
                let toggle_button: id = msg_send![class!(NSButton), alloc];
                let toggle_button: id =
                    msg_send![toggle_button, initWithFrame: toggle_button_frame];
                let _: () = msg_send![toggle_button, setButtonType: NS_SWITCH_BUTTON];
                let _: () = msg_send![toggle_button, setTitle: NSString::alloc(nil).init_str(ui_language.draft_panel_enabled())];
                let _: () = msg_send![toggle_button, setState: if draft_mode_active.load(Ordering::SeqCst) { 1i64 } else { 0i64 }];
                let _: () = msg_send![toggle_button, setAutoresizingMask: AUTORESIZE_BOTTOM_LEFT];
                let _: () = msg_send![toggle_button, setToolTip: NSString::alloc(nil).init_str(ui_language.draft_panel_enabled_tooltip())];
                let _: () = msg_send![toggle_button, setTarget: action_target];
                let _: () = msg_send![toggle_button, setAction: sel!(toggleDraftMode:)];
                (&mut *action_target).set_ivar("toggleButton", toggle_button);

                let _: () = msg_send![clear_button, setTarget: action_target];
                let _: () = msg_send![clear_button, setAction: sel!(clearClicked:)];
                let _: () = msg_send![copy_button, setTarget: action_target];
                let _: () = msg_send![copy_button, setAction: sel!(copyAllClicked:)];
                let _: () = msg_send![transform_button, setTarget: action_target];
                let _: () = msg_send![transform_button, setAction: sel!(showTransformClicked:)];
                (&mut *action_target).set_ivar("transformButton", transform_button);
                (&mut *action_target).set_ivar("clearButton", clear_button);
                (&mut *action_target).set_ivar("copyButton", copy_button);

                let _: () = msg_send![scroll_view, setDocumentView: text_view];
                if let Some(panel) = transform_panel.as_deref() {
                    panel.attach_to_parent(content_view);
                    panel.set_visible(false);
                }
                let _: () = msg_send![content_view, addSubview: scroll_view];
                let _: () = msg_send![content_view, addSubview: toggle_button];
                let _: () = msg_send![content_view, addSubview: transform_button];
                let _: () = msg_send![content_view, addSubview: clear_button];
                let _: () = msg_send![content_view, addSubview: copy_button];
                (&mut *window_delegate).set_ivar("contentView", content_view);
                (&mut *window_delegate).set_ivar("scrollView", scroll_view);
                (&mut *window_delegate).set_ivar("toggleButton", toggle_button);
                (&mut *window_delegate).set_ivar("transformButton", transform_button);
                (&mut *window_delegate).set_ivar("clearButton", clear_button);
                (&mut *window_delegate).set_ivar("copyButton", copy_button);
                (&mut *window_delegate).set_ivar(
                    "transformPanelPtr",
                    transform_panel
                        .as_deref()
                        .map(|panel| panel as *const DraftTransformPanel as usize)
                        .unwrap_or(0),
                );
                (&mut *window_delegate).set_ivar(
                    "transformExpandedPtr",
                    (&*transform_expanded as *const AtomicBool) as usize,
                );
                relayout_draft_content(
                    content_view,
                    scroll_view,
                    toggle_button,
                    transform_button,
                    clear_button,
                    copy_button,
                    transform_panel.as_deref(),
                    false,
                );
                let _: () = msg_send![window, orderOut: nil];

                Some(Self {
                    window,
                    content_view,
                    text_view,
                    scroll_view,
                    toggle_button,
                    transform_button,
                    clear_button,
                    copy_button,
                    transform_panel,
                    transform_expanded,
                    _action_target: action_target,
                    _window_delegate: window_delegate,
                    draft_mode_active,
                    visible,
                    close_requested,
                })
            }
        }

        pub fn show(&self) {
            unsafe {
                ensure_edit_shortcuts_menu();
                self.close_requested.store(false, Ordering::SeqCst);
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
                self.transform_expanded.store(false, Ordering::SeqCst);
                apply_transform_window_width(self.window, false);
                relayout_draft_content(
                    self.content_view,
                    self.scroll_view,
                    self.toggle_button,
                    self.transform_button,
                    self.clear_button,
                    self.copy_button,
                    self.transform_panel.as_deref(),
                    false,
                );
                if let Some(transform_panel) = self.transform_panel.as_deref() {
                    transform_panel.prepare_to_close();
                }
                if self.visible.swap(false, Ordering::SeqCst) {
                    let _: () = msg_send![self.window, orderOut: nil];
                }
            }
        }

        pub fn consume_close_requested(&self) -> bool {
            self.close_requested.swap(false, Ordering::SeqCst)
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

        pub fn set_draft_mode_enabled(&self, enabled: bool) {
            self.draft_mode_active.store(enabled, Ordering::SeqCst);
            unsafe {
                let _: () =
                    msg_send![self.toggle_button, setState: if enabled { 1i64 } else { 0i64 }];
            }
        }

        pub fn poll_aux_windows(&self) {
            unsafe {
                let expanded = self.transform_expanded.load(Ordering::SeqCst);
                relayout_draft_content(
                    self.content_view,
                    self.scroll_view,
                    self.toggle_button,
                    self.transform_button,
                    self.clear_button,
                    self.copy_button,
                    self.transform_panel.as_deref(),
                    expanded,
                );
            }
            if let Some(transform_panel) = self.transform_panel.as_deref() {
                transform_panel.poll_events();
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub struct DraftPanel;

    impl DraftPanel {
        pub fn new(
            _draft_mode_active: std::sync::Arc<std::sync::atomic::AtomicBool>,
        ) -> Option<Self> {
            Some(Self)
        }

        pub fn show(&self) {}

        pub fn hide(&self) {}

        pub fn consume_close_requested(&self) -> bool {
            false
        }

        pub fn is_key_window(&self) -> bool {
            false
        }

        pub fn set_text(&self, _text: &str) {}

        pub fn append_text(&self, _text: &str) {}

        pub fn clear(&self) {}

        pub fn set_draft_mode_enabled(&self, _enabled: bool) {}

        pub fn poll_aux_windows(&self) {}
    }
}

pub use platform::DraftPanel;
