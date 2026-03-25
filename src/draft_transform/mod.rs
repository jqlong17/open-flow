#[cfg(target_os = "macos")]
mod platform {
    use cocoa::appkit::NSPasteboardTypeString;
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSPoint, NSRange, NSRect, NSSize, NSString};
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    use tracing::{info, warn};

    use crate::common::config::Config;
    use crate::common::ui::UiLanguage;
    use crate::llm::LlmClient;

    const AUTORESIZE_WIDTH_HEIGHT: u64 = 18;
    const COMMAND_KEY_MASK: u64 = 1 << 20;
    const SEGMENT_TRACKING_SELECT_ONE: u64 = 0;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    enum TransformTab {
        RemoveFillers,
        Subtitle,
        Markdown,
    }

    impl TransformTab {
        fn index(self) -> i64 {
            match self {
                Self::RemoveFillers => 0,
                Self::Subtitle => 1,
                Self::Markdown => 2,
            }
        }

        fn from_index(index: i64) -> Self {
            match index {
                1 => Self::Subtitle,
                2 => Self::Markdown,
                _ => Self::RemoveFillers,
            }
        }

        fn title(self, ui: UiLanguage) -> &'static str {
            match self {
                Self::RemoveFillers => ui.pick("去除口语", "Polish"),
                Self::Subtitle => ui.pick("转为字幕", "Subtitles"),
                Self::Markdown => ui.pick("转为MD", "Markdown"),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct PromptStore {
        remove_fillers: String,
        subtitle: String,
        markdown: String,
    }

    impl Default for PromptStore {
        fn default() -> Self {
            Self {
                remove_fillers: "你是一个文本整理助手。请将用户提供的语音转写内容整理成更自然的书面表达，删除口头禅、重复词、语气词和无意义停顿，但不要改变原意，不要补充事实，不要总结。保留原有信息密度，只输出整理后的正文。".to_string(),
                subtitle: "你是一个专业的字幕整理助手。请将用户提供的语音转写内容转换为标准 SRT 字幕格式。输出必须严格符合 SRT 规范：每一条字幕包含连续编号、时间轴行、正文行，字幕之间空一行。时间轴格式必须为 `00:00:00,000 --> 00:00:00,000`。\n\n要求：\n1. 保留原意，不要编造信息，不要额外解释。\n2. 自动补齐必要标点，并按自然语义断句。\n3. 单条字幕尽量简洁，避免过长；必要时拆成多条。\n4. 默认根据全文文字量、句子长度和正常中文阅读速度，自动估算整段字幕的总时长与每条字幕时长，使节奏自然。\n5. 如果用户在提示词中额外写明了目标总时长、单条字幕时长、节奏要求或时间范围，则优先遵循这些明确要求。\n6. 如果原文非常短，也仍然输出完整合法的 SRT 格式。\n7. 最终只输出 SRT 正文，不要输出说明、标题或 Markdown 代码块。".to_string(),
                markdown: "你是一个 Markdown 笔记助手。请将用户提供的语音转写内容整理成结构清晰的 Markdown 文档。可根据内容自动拆分标题、列表和段落，但不要编造信息，不要输出额外说明，只输出 Markdown 正文。".to_string(),
            }
        }
    }

    impl PromptStore {
        fn path() -> Option<std::path::PathBuf> {
            Config::data_dir()
                .ok()
                .map(|dir| dir.join("draft_transform_prompts.toml"))
        }

        fn load() -> Self {
            let Some(path) = Self::path() else {
                return Self::default();
            };

            let Ok(content) = fs::read_to_string(path) else {
                return Self::default();
            };

            toml::from_str(&content).unwrap_or_default()
        }

        fn save(&self) {
            let Some(path) = Self::path() else {
                return;
            };

            let Ok(content) = toml::to_string_pretty(self) else {
                return;
            };

            if let Err(err) = fs::write(path, content) {
                warn!("保存 AI 转换提示词失败: {}", err);
            }
        }

        fn get(&self, tab: TransformTab) -> &str {
            match tab {
                TransformTab::RemoveFillers => &self.remove_fillers,
                TransformTab::Subtitle => &self.subtitle,
                TransformTab::Markdown => &self.markdown,
            }
        }

        fn set(&mut self, tab: TransformTab, value: String) {
            match tab {
                TransformTab::RemoveFillers => self.remove_fillers = value,
                TransformTab::Subtitle => self.subtitle = value,
                TransformTab::Markdown => self.markdown = value,
            }
        }

        fn reset(&mut self, tab: TransformTab) {
            let defaults = Self::default();
            self.set(tab, defaults.get(tab).to_string());
        }
    }

    enum GenerationEvent {
        Completed {
            request_id: u64,
            result: Result<String, String>,
        },
    }

    struct TransformPanelController {
        ui: UiLanguage,
        root_view: id,
        prompt_scroll_view: id,
        prompt_view: id,
        result_scroll_view: id,
        result_view: id,
        tab_control: id,
        reset_button: id,
        clear_button: id,
        copy_button: id,
        generate_button: id,
        prompts: Mutex<PromptStore>,
        active_tab: Mutex<TransformTab>,
        source_text: Mutex<String>,
        generation_rx: Mutex<Option<Receiver<GenerationEvent>>>,
        latest_request_id: AtomicU64,
        generation_inflight: AtomicBool,
        visible: AtomicBool,
    }

    impl TransformPanelController {
        fn new(ui: UiLanguage) -> Self {
            Self {
                ui,
                root_view: nil,
                prompt_scroll_view: nil,
                prompt_view: nil,
                result_scroll_view: nil,
                result_view: nil,
                tab_control: nil,
                reset_button: nil,
                clear_button: nil,
                copy_button: nil,
                generate_button: nil,
                prompts: Mutex::new(PromptStore::load()),
                active_tab: Mutex::new(TransformTab::RemoveFillers),
                source_text: Mutex::new(String::new()),
                generation_rx: Mutex::new(None),
                latest_request_id: AtomicU64::new(0),
                generation_inflight: AtomicBool::new(false),
                visible: AtomicBool::new(false),
            }
        }

        fn set_source_text(&self, text: &str) {
            if let Ok(mut source_text) = self.source_text.lock() {
                *source_text = text.to_string();
            }
        }

        fn sync_prompt_view(&self) {
            let active_tab = self
                .active_tab
                .lock()
                .map(|guard| *guard)
                .unwrap_or(TransformTab::RemoveFillers);
            let prompt = self
                .prompts
                .lock()
                .map(|store| store.get(active_tab).to_string())
                .unwrap_or_default();

            unsafe {
                let ns_str = NSString::alloc(nil).init_str(&prompt);
                let _: () = msg_send![self.prompt_view, setString: ns_str];
                let _: () = msg_send![self.tab_control, setSelectedSegment: active_tab.index()];
            }
        }

        fn save_current_prompt(&self) {
            let active_tab = self
                .active_tab
                .lock()
                .map(|guard| *guard)
                .unwrap_or(TransformTab::RemoveFillers);
            let prompt_text = unsafe { text_view_string(self.prompt_view) };
            if let Ok(mut prompts) = self.prompts.lock() {
                prompts.set(active_tab, prompt_text);
                prompts.save();
            }
        }

        fn switch_tab(&self, tab: TransformTab) {
            self.save_current_prompt();
            if let Ok(mut active_tab) = self.active_tab.lock() {
                *active_tab = tab;
            }
            self.sync_prompt_view();
        }

        fn reset_current_prompt(&self) {
            let active_tab = self
                .active_tab
                .lock()
                .map(|guard| *guard)
                .unwrap_or(TransformTab::RemoveFillers);
            if let Ok(mut prompts) = self.prompts.lock() {
                prompts.reset(active_tab);
                prompts.save();
            }
            self.sync_prompt_view();
        }

        fn clear_result(&self) {
            unsafe {
                let empty = NSString::alloc(nil).init_str("");
                let _: () = msg_send![self.result_view, setString: empty];
            }
        }

        fn copy_result(&self) {
            unsafe {
                copy_all_text(self.result_view);
            }
        }

        fn set_result_text(&self, text: &str) {
            unsafe {
                let ns_str = NSString::alloc(nil).init_str(text);
                let _: () = msg_send![self.result_view, setString: ns_str];
                let text_storage: id = msg_send![self.result_view, textStorage];
                let length: u64 = msg_send![text_storage, length];
                let range = NSRange::new(length, 0);
                let _: () = msg_send![self.result_view, scrollRangeToVisible: range];
            }
        }

        fn set_generate_button_title(&self, title: &str) {
            unsafe {
                let _: () =
                    msg_send![self.generate_button, setTitle: NSString::alloc(nil).init_str(title)];
            }
        }

        fn focus_prompt(&self) {
            unsafe {
                let window: id = msg_send![self.root_view, window];
                if window != nil {
                    let _: () = msg_send![window, makeFirstResponder: self.prompt_view];
                }
            }
        }

        fn set_visible(&self, visible: bool) {
            let was_visible = self.visible.swap(visible, Ordering::SeqCst);
            if was_visible == visible {
                unsafe {
                    let _: () =
                        msg_send![self.root_view, setHidden: if visible { NO } else { YES }];
                }
                return;
            }

            if visible {
                self.sync_prompt_view();
            } else {
                self.save_current_prompt();
            }

            unsafe {
                let _: () = msg_send![self.root_view, setHidden: if visible { NO } else { YES }];
            }

            if visible {
                self.focus_prompt();
            }
        }

        fn is_visible(&self) -> bool {
            self.visible.load(Ordering::SeqCst)
        }

        fn layout_subviews(&self, width: f64, height: f64) {
            unsafe {
                let padding = 14.0f64;
                let header_height = 28.0f64;
                let controls_height = 28.0f64;
                let content_width = (width - padding * 2.0).max(240.0);
                let available_body =
                    (height - padding * 3.0 - header_height - controls_height - 24.0).max(240.0);
                let prompt_height = (available_body * 0.42).clamp(110.0, 190.0);
                let header_y = height - padding - header_height;
                let prompt_y = header_y - 10.0 - prompt_height;
                let controls_y = prompt_y - 10.0 - controls_height;
                let result_y = padding;
                let result_height = (controls_y - 8.0 - result_y).max(120.0);
                let tab_width = content_width.min(300.0);

                let _: () = msg_send![
                    self.tab_control,
                    setFrame: NSRect::new(
                        NSPoint::new(padding, header_y),
                        NSSize::new(tab_width, header_height)
                    )
                ];
                let _: () = msg_send![
                    self.reset_button,
                    setFrame: NSRect::new(
                        NSPoint::new(width - padding - 88.0, header_y),
                        NSSize::new(88.0, header_height)
                    )
                ];

                let _: () = msg_send![
                    self.prompt_scroll_view,
                    setFrame: NSRect::new(
                        NSPoint::new(padding, prompt_y),
                        NSSize::new(content_width, prompt_height)
                    )
                ];
                let _: () = msg_send![
                    self.clear_button,
                    setFrame: NSRect::new(
                        NSPoint::new(width - padding - 176.0, controls_y),
                        NSSize::new(52.0, controls_height)
                    )
                ];
                let _: () = msg_send![
                    self.copy_button,
                    setFrame: NSRect::new(
                        NSPoint::new(width - padding - 116.0, controls_y),
                        NSSize::new(52.0, controls_height)
                    )
                ];
                let _: () = msg_send![
                    self.generate_button,
                    setFrame: NSRect::new(
                        NSPoint::new(width - padding - 56.0, controls_y),
                        NSSize::new(56.0, controls_height)
                    )
                ];
                let _: () = msg_send![
                    self.result_scroll_view,
                    setFrame: NSRect::new(
                        NSPoint::new(padding, result_y),
                        NSSize::new(content_width, result_height)
                    )
                ];
            }
        }

        fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            unsafe {
                let _: () = msg_send![
                    self.root_view,
                    setFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
                ];
            }
            self.layout_subviews(width, height);
        }

        fn generate(&self) {
            self.save_current_prompt();

            let source_text = self
                .source_text
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if source_text.trim().is_empty() {
                self.set_result_text(self.ui.pick(
                    "当前草稿为空，先录一点内容再生成。",
                    "The draft is empty. Record something first.",
                ));
                return;
            }

            let active_tab = self
                .active_tab
                .lock()
                .map(|guard| *guard)
                .unwrap_or(TransformTab::RemoveFillers);
            let prompt = self
                .prompts
                .lock()
                .map(|store| store.get(active_tab).to_string())
                .unwrap_or_default();

            if prompt.trim().is_empty() {
                self.set_result_text(self.ui.pick(
                    "当前提示词为空，请先填写提示词。",
                    "The prompt is empty. Please enter a prompt first.",
                ));
                return;
            }

            let config = Config::load().unwrap_or_default();
            let model = config.resolved_correction_model();
            let api_key = config.resolved_correction_api_key();
            if api_key.trim().is_empty() {
                self.set_result_text(self.ui.pick(
                    "尚未配置智谱 API Key。请先在设置 > 模型 中完成配置。",
                    "No Zhipu API key is configured yet. Configure it in Settings > Models first.",
                ));
                return;
            }

            let request_id = self.latest_request_id.fetch_add(1, Ordering::SeqCst) + 1;
            self.generation_inflight.store(true, Ordering::SeqCst);
            self.clear_result();
            self.set_result_text(self.ui.pick("正在生成，请稍候...", "Generating..."));
            self.set_generate_button_title(self.ui.pick("生成中...", "Generating..."));

            let (tx, rx) = mpsc::channel();
            if let Ok(mut rx_slot) = self.generation_rx.lock() {
                *rx_slot = Some(rx);
            }

            info!(
                "[DraftTransform] generate_start request_id={} tab={:?} model={} source_chars={}",
                request_id,
                active_tab,
                model,
                source_text.chars().count()
            );

            std::thread::spawn(move || {
                let started_at = Instant::now();
                let result = match LlmClient::new(api_key, model.clone()) {
                    Ok(client) => {
                        let rt = match tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            Ok(rt) => rt,
                            Err(err) => {
                                let _ = tx.send(GenerationEvent::Completed {
                                    request_id,
                                    result: Err(format!("创建运行时失败: {}", err)),
                                });
                                return;
                            }
                        };

                        match rt.block_on(async { client.generate(&prompt, &source_text).await }) {
                            Ok(text) => Ok(text),
                            Err(err) => Err(format_error_chain(&err)),
                        }
                    }
                    Err(err) => Err(err.to_string()),
                };

                info!(
                    "[DraftTransform] generate_complete request_id={} model={} duration_ms={}",
                    request_id,
                    model,
                    started_at.elapsed().as_millis()
                );

                let _ = tx.send(GenerationEvent::Completed { request_id, result });
            });
        }

        fn poll_events(&self) {
            let mut clear_slot = false;
            if let Ok(mut rx_slot) = self.generation_rx.lock() {
                if let Some(rx) = rx_slot.as_ref() {
                    loop {
                        match rx.try_recv() {
                            Ok(GenerationEvent::Completed { request_id, result }) => {
                                if request_id != self.latest_request_id.load(Ordering::SeqCst) {
                                    continue;
                                }

                                self.generation_inflight.store(false, Ordering::SeqCst);
                                self.set_generate_button_title(self.ui.pick("生成", "Generate"));
                                match result {
                                    Ok(text) => {
                                        let content = if text.trim().is_empty() {
                                            self.ui
                                                .pick(
                                                    "模型返回了空结果。",
                                                    "The model returned an empty result.",
                                                )
                                                .to_string()
                                        } else {
                                            text
                                        };
                                        self.set_result_text(&content);
                                    }
                                    Err(err) => {
                                        self.set_result_text(&self.ui.pick(
                                            format!("生成失败：{}", err),
                                            format!("Generation failed: {}", err),
                                        ));
                                    }
                                }
                                clear_slot = true;
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                self.generation_inflight.store(false, Ordering::SeqCst);
                                self.set_generate_button_title(self.ui.pick("生成", "Generate"));
                                clear_slot = true;
                                break;
                            }
                        }
                    }
                }

                if clear_slot {
                    *rx_slot = None;
                }
            }
        }

        fn prepare_to_close(&self) {
            self.save_current_prompt();
        }
    }

    pub struct DraftTransformPanel {
        controller: Box<TransformPanelController>,
        _action_target: id,
    }

    unsafe impl Send for DraftTransformPanel {}
    unsafe impl Sync for DraftTransformPanel {}

    fn action_target_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) =
                ClassDecl::new("OpenFlowDraftTransformActionTarget", class!(NSObject))
            {
                decl.add_ivar::<usize>("controllerPtr");
                decl.add_method(
                    sel!(selectTransformPreset:),
                    select_transform_preset as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(resetTransformPrompt:),
                    reset_transform_prompt as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(clearTransformResult:),
                    clear_transform_result as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(copyTransformResult:),
                    copy_transform_result as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(generateTransformResult:),
                    generate_transform_result as extern "C" fn(&Object, Sel, id),
                );
                decl.register() as *const Class as usize
            } else {
                class!(OpenFlowDraftTransformActionTarget) as *const Class as usize
            }
        });
        ptr as *const Class
    }

    extern "C" fn select_transform_preset(this: &Object, _cmd: Sel, sender: id) {
        unsafe {
            let controller_ptr: usize = *this.get_ivar("controllerPtr");
            if controller_ptr == 0 {
                return;
            }
            let selected: i64 = msg_send![sender, selectedSegment];
            let controller = controller_ptr as *const TransformPanelController;
            (*controller).switch_tab(TransformTab::from_index(selected));
        }
    }

    extern "C" fn reset_transform_prompt(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let controller_ptr: usize = *this.get_ivar("controllerPtr");
            if controller_ptr == 0 {
                return;
            }
            let controller = controller_ptr as *const TransformPanelController;
            (*controller).reset_current_prompt();
        }
    }

    extern "C" fn clear_transform_result(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let controller_ptr: usize = *this.get_ivar("controllerPtr");
            if controller_ptr == 0 {
                return;
            }
            let controller = controller_ptr as *const TransformPanelController;
            (*controller).clear_result();
        }
    }

    extern "C" fn copy_transform_result(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let controller_ptr: usize = *this.get_ivar("controllerPtr");
            if controller_ptr == 0 {
                return;
            }
            let controller = controller_ptr as *const TransformPanelController;
            (*controller).copy_result();
        }
    }

    extern "C" fn generate_transform_result(this: &Object, _cmd: Sel, _sender: id) {
        unsafe {
            let controller_ptr: usize = *this.get_ivar("controllerPtr");
            if controller_ptr == 0 {
                return;
            }
            let controller = controller_ptr as *const TransformPanelController;
            (*controller).generate();
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

    fn format_error_chain(err: &anyhow::Error) -> String {
        let mut parts = Vec::new();
        for cause in err.chain() {
            let text = cause.to_string();
            if parts.last() == Some(&text) {
                continue;
            }
            parts.push(text);
        }
        parts.join("\n原因：")
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

    extern "C" fn transform_text_view_perform_key_equivalent(
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

    fn text_view_class() -> *const Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe {
            if let Some(mut decl) =
                ClassDecl::new("OpenFlowDraftTransformTextView", class!(NSTextView))
            {
                decl.add_method(
                    sel!(performKeyEquivalent:),
                    transform_text_view_perform_key_equivalent
                        as extern "C" fn(&Object, Sel, id) -> bool,
                );
                decl.register() as *const Class as usize
            } else {
                class!(OpenFlowDraftTransformTextView) as *const Class as usize
            }
        });
        ptr as *const Class
    }

    unsafe fn build_text_view(editable: bool) -> id {
        let text_view_class = text_view_class();
        let text_view: id = msg_send![text_view_class, alloc];
        let text_view: id = msg_send![
            text_view,
            initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0))
        ];
        let _: () = msg_send![text_view, setEditable: if editable { YES } else { NO }];
        let _: () = msg_send![text_view, setSelectable: YES];
        let _: () = msg_send![text_view, setRichText: NO];
        let inset = NSSize::new(12.0, 12.0);
        let _: () = msg_send![text_view, setTextContainerInset: inset];
        let _: () = msg_send![text_view, setAutoresizingMask: AUTORESIZE_WIDTH_HEIGHT];
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.5f64];
        let _: () = msg_send![text_view, setFont: font];
        text_view
    }

    unsafe fn build_scroll_view(document_view: id) -> id {
        let scroll_view: id = msg_send![class!(NSScrollView), alloc];
        let scroll_view: id = msg_send![
            scroll_view,
            initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0))
        ];
        let _: () = msg_send![scroll_view, setHasVerticalScroller: YES];
        let _: () = msg_send![scroll_view, setHasHorizontalScroller: NO];
        let _: () = msg_send![scroll_view, setAutoresizingMask: AUTORESIZE_WIDTH_HEIGHT];
        let _: () = msg_send![scroll_view, setDocumentView: document_view];
        scroll_view
    }

    unsafe fn build_button(title: &str, target: id, action: Sel) -> id {
        let button: id = msg_send![class!(NSButton), alloc];
        let button: id = msg_send![
            button,
            initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(80.0, 28.0))
        ];
        let _: () = msg_send![button, setTitle: NSString::alloc(nil).init_str(title)];
        let _: () = msg_send![button, setBezelStyle: 1i64];
        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: action];
        button
    }

    impl DraftTransformPanel {
        pub fn new(ui: UiLanguage) -> Option<Self> {
            unsafe {
                let mut controller = Box::new(TransformPanelController::new(ui));
                let controller_ptr = (&mut *controller as *mut TransformPanelController) as usize;

                let action_target_class = action_target_class();
                let action_target: id = msg_send![action_target_class, new];
                (&mut *action_target).set_ivar("controllerPtr", controller_ptr);

                let root_view: id = msg_send![class!(NSView), alloc];
                let root_view: id = msg_send![
                    root_view,
                    initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0))
                ];
                let _: () = msg_send![root_view, setHidden: YES];

                let prompt_view = build_text_view(true);
                let prompt_scroll_view = build_scroll_view(prompt_view);
                let result_view = build_text_view(true);
                let result_scroll_view = build_scroll_view(result_view);

                let tab_control: id = msg_send![class!(NSSegmentedControl), alloc];
                let tab_control: id = msg_send![
                    tab_control,
                    initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, 28.0))
                ];
                let _: () = msg_send![tab_control, setSegmentCount: 3i64];
                let _: () = msg_send![tab_control, setTrackingMode: SEGMENT_TRACKING_SELECT_ONE];
                let _: () = msg_send![
                    tab_control,
                    setLabel: NSString::alloc(nil).init_str(TransformTab::RemoveFillers.title(ui))
                    forSegment: 0i64
                ];
                let _: () = msg_send![
                    tab_control,
                    setLabel: NSString::alloc(nil).init_str(TransformTab::Subtitle.title(ui))
                    forSegment: 1i64
                ];
                let _: () = msg_send![
                    tab_control,
                    setLabel: NSString::alloc(nil).init_str(TransformTab::Markdown.title(ui))
                    forSegment: 2i64
                ];
                let _: () = msg_send![tab_control, setSelectedSegment: 0i64];
                let _: () = msg_send![tab_control, setTarget: action_target];
                let _: () = msg_send![tab_control, setAction: sel!(selectTransformPreset:)];

                let reset_button = build_button(
                    ui.pick("重置提示词", "Reset Prompt"),
                    action_target,
                    sel!(resetTransformPrompt:),
                );
                let clear_button = build_button(
                    ui.pick("清空", "Clear"),
                    action_target,
                    sel!(clearTransformResult:),
                );
                let copy_button = build_button(
                    ui.pick("复制", "Copy"),
                    action_target,
                    sel!(copyTransformResult:),
                );
                let generate_button = build_button(
                    ui.pick("生成", "Generate"),
                    action_target,
                    sel!(generateTransformResult:),
                );

                let _: () = msg_send![root_view, addSubview: tab_control];
                let _: () = msg_send![root_view, addSubview: reset_button];
                let _: () = msg_send![root_view, addSubview: prompt_scroll_view];
                let _: () = msg_send![root_view, addSubview: clear_button];
                let _: () = msg_send![root_view, addSubview: copy_button];
                let _: () = msg_send![root_view, addSubview: generate_button];
                let _: () = msg_send![root_view, addSubview: result_scroll_view];

                controller.root_view = root_view;
                controller.prompt_scroll_view = prompt_scroll_view;
                controller.prompt_view = prompt_view;
                controller.result_scroll_view = result_scroll_view;
                controller.result_view = result_view;
                controller.tab_control = tab_control;
                controller.reset_button = reset_button;
                controller.clear_button = clear_button;
                controller.copy_button = copy_button;
                controller.generate_button = generate_button;
                controller.sync_prompt_view();
                controller.layout_subviews(100.0, 100.0);

                Some(Self {
                    controller,
                    _action_target: action_target,
                })
            }
        }

        pub fn attach_to_parent(&self, parent_view: id) {
            unsafe {
                let _: () = msg_send![parent_view, addSubview: self.controller.root_view];
            }
        }

        pub fn set_source_text(&self, text: &str) {
            self.controller.set_source_text(text);
        }

        pub fn set_visible(&self, visible: bool) {
            self.controller.set_visible(visible);
        }

        pub fn is_visible(&self) -> bool {
            self.controller.is_visible()
        }

        pub fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            self.controller.set_frame(x, y, width, height);
        }

        pub fn poll_events(&self) {
            self.controller.poll_events();
        }

        pub fn prepare_to_close(&self) {
            self.controller.prepare_to_close();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::common::ui::UiLanguage;

    pub struct DraftTransformPanel;

    impl DraftTransformPanel {
        pub fn new(_ui: UiLanguage) -> Option<Self> {
            Some(Self)
        }

        pub fn attach_to_parent(&self, _parent_view: *mut std::ffi::c_void) {}

        pub fn set_source_text(&self, _text: &str) {}

        pub fn set_visible(&self, _visible: bool) {}

        pub fn is_visible(&self) -> bool {
            false
        }

        pub fn set_frame(&self, _x: f64, _y: f64, _width: f64, _height: f64) {}

        pub fn poll_events(&self) {}

        pub fn prepare_to_close(&self) {}
    }
}

pub use platform::DraftTransformPanel;
