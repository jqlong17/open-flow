use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_path: None,
        }
    }
}

impl Config {
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

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
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
}
