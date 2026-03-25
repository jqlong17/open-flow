#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiLanguage {
    #[default]
    Zh,
    En,
}

impl UiLanguage {
    pub fn is_english(self) -> bool {
        matches!(self, Self::En)
    }

    pub fn pick<T>(self, zh: T, en: T) -> T {
        match self {
            Self::Zh => zh,
            Self::En => en,
        }
    }

    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en_gb" | "english" => Self::En,
            _ => Self::Zh,
        }
    }

    pub fn from_config(config: &crate::common::config::Config) -> Self {
        Self::from_config_value(&config.ui_language)
    }

    pub fn status_idle(self) -> &'static str {
        match self {
            Self::Zh => "状态：待机",
            Self::En => "Status: Idle",
        }
    }

    pub fn status_recording(self) -> &'static str {
        match self {
            Self::Zh => "状态：录音中",
            Self::En => "Status: Recording",
        }
    }

    pub fn status_transcribing(self) -> &'static str {
        match self {
            Self::Zh => "状态：转写中",
            Self::En => "Status: Transcribing",
        }
    }

    pub fn tray_update(self) -> &'static str {
        match self {
            Self::Zh => "检查更新",
            Self::En => "Check for Updates",
        }
    }

    pub fn tray_update_downloading(self) -> &'static str {
        match self {
            Self::Zh => "正在后台下载更新...",
            Self::En => "Downloading Update...",
        }
    }

    pub fn tray_update_progress(self, percent: u8) -> String {
        match self {
            Self::Zh => format!("正在下载更新... {}%", percent),
            Self::En => format!("Downloading Update... {}%", percent),
        }
    }

    pub fn tray_restart_to_apply_update(self) -> &'static str {
        match self {
            Self::Zh => "重启以应用更新",
            Self::En => "Restart to Apply Update",
        }
    }

    pub fn tray_draft(self) -> &'static str {
        match self {
            Self::Zh => "录音草稿",
            Self::En => "Recording Draft",
        }
    }

    pub fn tray_draft_checked(self) -> &'static str {
        match self {
            Self::Zh => "录音草稿 ✓",
            Self::En => "Recording Draft ✓",
        }
    }

    pub fn tray_preferences(self) -> &'static str {
        match self {
            Self::Zh => "偏好设置...",
            Self::En => "Preferences...",
        }
    }

    pub fn tray_exit(self) -> &'static str {
        match self {
            Self::Zh => "退出",
            Self::En => "Quit",
        }
    }

    pub fn tray_tooltip(self) -> &'static str {
        match self {
            Self::Zh => "Open Flow - 语音输入",
            Self::En => "Open Flow - Voice Input",
        }
    }

    pub fn draft_panel_title(self) -> &'static str {
        match self {
            Self::Zh => "录音草稿",
            Self::En => "Recording Draft",
        }
    }

    pub fn clear(self) -> &'static str {
        match self {
            Self::Zh => "清空",
            Self::En => "Clear",
        }
    }

    pub fn copy(self) -> &'static str {
        match self {
            Self::Zh => "复制",
            Self::En => "Copy",
        }
    }

    pub fn draft_panel_enabled(self) -> &'static str {
        match self {
            Self::Zh => "草稿模式",
            Self::En => "Draft Mode",
        }
    }

    pub fn draft_panel_help(self) -> &'static str {
        match self {
            Self::Zh => "说明",
            Self::En => "Info",
        }
    }

    pub fn draft_panel_enabled_tooltip(self) -> &'static str {
        match self {
            Self::Zh => "开启后，所有转写结果都会写入录音草稿，不再直接输出到原来的目标位置；关闭后，将恢复传统模式。",
            Self::En => "When enabled, all transcription results are sent to the recording draft instead of the original output target. Turn it off to restore the traditional output mode.",
        }
    }

    pub fn settings_window_title(self) -> &'static str {
        match self {
            Self::Zh => "Open Flow 偏好设置",
            Self::En => "Open Flow Preferences",
        }
    }

    pub fn settings_heading(self) -> &'static str {
        match self {
            Self::Zh => "Open Flow 设置",
            Self::En => "Open Flow Settings",
        }
    }

    pub fn settings_edit_config_hint(self) -> &'static str {
        match self {
            Self::Zh => "编辑 config.toml 可修改设置。",
            Self::En => "Edit config.toml to change settings.",
        }
    }

    pub fn settings_restart_hint(self) -> &'static str {
        match self {
            Self::Zh => "修改后请重启 daemon。",
            Self::En => "Restart daemon after changes.",
        }
    }

    pub fn settings_current_config(self) -> &'static str {
        match self {
            Self::Zh => "当前配置：",
            Self::En => "Current configuration:",
        }
    }

    pub fn settings_provider(self) -> &'static str {
        match self {
            Self::Zh => "Provider",
            Self::En => "Provider",
        }
    }

    pub fn settings_hotkey(self) -> &'static str {
        match self {
            Self::Zh => "热键",
            Self::En => "Hotkey",
        }
    }

    pub fn settings_trigger(self) -> &'static str {
        match self {
            Self::Zh => "触发模式",
            Self::En => "Trigger",
        }
    }

    pub fn settings_groq_model(self) -> &'static str {
        match self {
            Self::Zh => "Groq 模型",
            Self::En => "Groq Model",
        }
    }

    pub fn settings_open_config_file(self) -> &'static str {
        match self {
            Self::Zh => "打开配置文件",
            Self::En => "Open Config File",
        }
    }
}
