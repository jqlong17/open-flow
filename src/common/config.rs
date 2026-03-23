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
    #[serde(default = "default_provider")]
    pub provider: String,
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
    #[serde(default)]
    pub chinese_conversion: String,
}

fn default_provider() -> String {
    "local".into()
}
fn default_groq_model() -> String {
    "whisper-large-v3-turbo".into()
}
fn default_hotkey() -> String {
    "right_cmd".into()
}
fn default_trigger_mode() -> String {
    "toggle".into()
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
            provider: default_provider(),
            groq_api_key: String::new(),
            groq_model: default_groq_model(),
            groq_language: String::new(),
            hotkey: default_hotkey(),
            trigger_mode: default_trigger_mode(),
            chinese_conversion: String::new(),
        }
    }
}

impl Config {
    pub fn resolved_groq_api_key(&self) -> String {
        std::env::var("GROQ_API_KEY").unwrap_or_else(|_| self.groq_api_key.clone())
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

    /// 设置模型预设并写回 config
    pub fn set_model_preset(&mut self, preset: ModelPreset) -> Result<()> {
        self.model_preset = Some(preset.as_str().to_string());
        self.save()
    }
}
