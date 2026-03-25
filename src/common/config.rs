use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

const CONFIG_FILE: &str = "config.toml";

/// 模型预设：仅支持 quantized（默认）与 fp16
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelPreset {
    /// HuggingFace 量化版（~200MB），默认
    #[default]
    Quantized,
    /// FP16 半精度（~450MB），更高精度，需手动切换
    Fp16,
}

impl ModelPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelPreset::Quantized => "quantized",
            ModelPreset::Fp16 => "fp16",
        }
    }
}

impl std::str::FromStr for ModelPreset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase();
        match s.as_str() {
            "quantized" | "quant" => Ok(ModelPreset::Quantized),
            "fp16" | "medium" => Ok(ModelPreset::Fp16),
            _ => Err(format!("未知预设: {}，可选: quantized, fp16", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_preset: Option<String>,
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub correction_enabled: String,
    #[serde(default = "default_correction_model")]
    pub correction_model: String,
    #[serde(default)]
    pub correction_api_key: String,
    #[serde(default)]
    pub groq_api_key: String,
    #[serde(default = "default_groq_model")]
    pub groq_model: String,
    #[serde(default)]
    pub groq_language: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
    #[serde(default = "default_capture_mode")]
    pub capture_mode: String,
    #[serde(default)]
    pub input_source: String,
    #[serde(default)]
    pub system_audio_target_pid: String,
    #[serde(default)]
    pub system_audio_target_name: String,
    #[serde(default)]
    pub system_audio_target_bundle_id: String,
    #[serde(default)]
    pub chinese_conversion: String,
    #[serde(default)]
    pub performance_log_enabled: String,
}

fn default_provider() -> String {
    "local".into()
}
fn default_ui_language() -> String {
    "zh".into()
}
fn default_groq_model() -> String {
    "whisper-large-v3-turbo".into()
}
fn default_correction_model() -> String {
    "GLM-4.7-Flash".into()
}
fn default_hotkey() -> String {
    "right_cmd".into()
}
fn default_trigger_mode() -> String {
    "toggle".into()
}
fn default_capture_mode() -> String {
    "microphone".into()
}

impl Config {
    /// 当前生效的模型预设（config 未设置时默认 quantized）
    pub fn effective_preset(&self) -> ModelPreset {
        self.model_preset
            .as_deref()
            .and_then(|s| s.trim().to_lowercase().parse().ok())
            .unwrap_or_default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_path: None,
            model_preset: None,
            ui_language: default_ui_language(),
            provider: default_provider(),
            correction_enabled: String::new(),
            correction_model: default_correction_model(),
            correction_api_key: String::new(),
            groq_api_key: String::new(),
            groq_model: default_groq_model(),
            groq_language: String::new(),
            hotkey: default_hotkey(),
            trigger_mode: default_trigger_mode(),
            capture_mode: default_capture_mode(),
            input_source: String::new(),
            system_audio_target_pid: String::new(),
            system_audio_target_name: String::new(),
            system_audio_target_bundle_id: String::new(),
            chinese_conversion: String::new(),
            performance_log_enabled: String::new(),
        }
    }
}

impl Config {
    pub fn resolved_groq_api_key(&self) -> String {
        std::env::var("GROQ_API_KEY").unwrap_or_else(|_| self.groq_api_key.clone())
    }

    pub fn correction_enabled(&self) -> bool {
        matches!(
            self.correction_enabled.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        )
    }

    pub fn resolved_correction_api_key(&self) -> String {
        std::env::var("OPEN_FLOW_CORRECTION_API_KEY")
            .unwrap_or_else(|_| self.correction_api_key.clone())
    }

    pub fn resolved_correction_model(&self) -> String {
        let model = self.correction_model.trim();
        if model.is_empty() {
            default_correction_model()
        } else {
            model.to_string()
        }
    }

    pub fn resolved_input_source(&self) -> Option<String> {
        let source = self.input_source.trim();
        if source.is_empty() {
            None
        } else {
            Some(source.to_string())
        }
    }

    pub fn resolved_capture_mode(&self) -> String {
        match self.capture_mode.trim() {
            "system_audio_desktop" => "system_audio_desktop".to_string(),
            "system_audio_application" => "system_audio_application".to_string(),
            "system_audio_microphone" => "system_audio_microphone".to_string(),
            _ => default_capture_mode(),
        }
    }

    pub fn performance_log_enabled(&self) -> bool {
        matches!(
            self.performance_log_enabled
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        )
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            info!("Config file not found, creating default config");
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&config_path)?;
        let mut config: Config = toml::from_str(&content)?;

        // 迁移：移除 Shandianshuo 等第三方路径，仅使用 open-flow 自己的目录
        if let Some(ref p) = config.model_path {
            let s = p.to_string_lossy();
            if s.contains("Shandianshuo") || s.contains("shandianshuo") {
                config.model_path = None;
                config.save()?;
            }
        }

        if config
            .model_path
            .as_ref()
            .map_or(false, |p| p.as_os_str().is_empty())
        {
            config.model_path = None;
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        let tmp = config_path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, &config_path)?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "openflow", "open-flow")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        let config_dir = dirs.config_dir();
        fs::create_dir_all(config_dir)?;

        Ok(config_dir.join(CONFIG_FILE))
    }

    pub fn data_dir() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "openflow", "open-flow")
            .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;

        let data_dir = dirs.data_dir();
        fs::create_dir_all(data_dir)?;

        Ok(data_dir.to_path_buf())
    }

    pub fn personal_vocabulary_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("personal_vocabulary.txt"))
    }

    pub fn correction_system_prompt_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("correction_system_prompt.txt"))
    }

    /// 设置模型预设并写回 config
    pub fn set_model_preset(&mut self, preset: ModelPreset) -> Result<()> {
        self.model_preset = Some(preset.as_str().to_string());
        self.save()
    }
}
