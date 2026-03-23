use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

use crate::common::config::Config;

const BIGMODEL_CHAT_COMPLETIONS_URL: &str = "https://open.bigmodel.cn/api/paas/v4/chat/completions";

pub struct TextCorrector {
    client: Client,
    api_key: String,
    model: String,
    vocabulary: Vec<String>,
}

impl TextCorrector {
    pub fn from_config(config: &Config) -> Option<Self> {
        if !config.correction_enabled() {
            return None;
        }

        let api_key = config.resolved_correction_api_key();
        if api_key.trim().is_empty() {
            warn!("文本纠错已启用，但缺少 API key，已跳过纠错");
            return None;
        }

        let vocabulary = load_personal_vocabulary().unwrap_or_default();

        let client = match Client::builder().timeout(Duration::from_secs(20)).build() {
            Ok(client) => client,
            Err(err) => {
                warn!("创建文本纠错 HTTP client 失败: {}", err);
                return None;
            }
        };

        Some(Self {
            client,
            api_key,
            model: config.resolved_correction_model(),
            vocabulary,
        })
    }

    pub async fn correct(&self, raw_text: &str) -> Result<String> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            return Ok(String::new());
        }

        let system_prompt = build_system_prompt(&self.vocabulary);
        let request = BigModelChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".into(),
                    content: raw_text.to_string(),
                },
            ],
            stream: false,
            temperature: Some(0.1),
            do_sample: Some(false),
            max_tokens: Some(1024),
            thinking: Some(ThinkingConfig {
                r#type: "disabled".into(),
                clear_thinking: true,
            }),
        };

        let response = self
            .client
            .post(BIGMODEL_CHAT_COMPLETIONS_URL)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("调用 BigModel 纠错接口失败")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("BigModel 纠错失败: {} {}", status, body);
        }

        let parsed: BigModelChatResponse = response
            .json()
            .await
            .context("解析 BigModel 纠错响应失败")?;

        let corrected = parsed
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .unwrap_or_default();

        if corrected.is_empty() {
            warn!("BigModel 纠错返回空结果，回退原文");
            return Ok(raw_text.to_string());
        }

        info!(
            "文本纠错完成: raw_chars={} corrected_chars={} vocab_count={}",
            raw_text.chars().count(),
            corrected.chars().count(),
            self.vocabulary.len()
        );
        Ok(corrected)
    }
}

fn load_personal_vocabulary() -> Result<Vec<String>> {
    let path = Config::personal_vocabulary_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path).context("读取个人词表失败")?;
    let terms = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();
    Ok(terms)
}

fn build_system_prompt(vocabulary: &[String]) -> String {
    let vocab_block = if vocabulary.is_empty() {
        "(无个人词表)".to_string()
    } else {
        vocabulary
            .iter()
            .take(200)
            .map(|term| format!("- {}", term))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "你是一个语音转写轻量纠错器。你的唯一任务是修正明显的 ASR 识别错误。\n\
规则：\n\
1. 只修正明显错误，不要改写句式，不要润色，不要总结，不要解释。\n\
2. 不要补充用户没说过的事实，不要扩写。\n\
3. 如果原文已经合理，就原样输出。\n\
4. 优先参考个人词表中的常用词、品牌词、人名、项目名；当原文发音或拼写接近这些词时，可纠正为词表中的标准写法。\n\
5. 保留原有语言风格和中英文混排方式。\n\
6. 最终只输出纠错后的单段文本，不要输出任何说明。\n\
\n个人词表：\n{}",
        vocab_block
    )
}

#[derive(Serialize)]
struct BigModelChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    do_sample: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ThinkingConfig {
    r#type: String,
    clear_thinking: bool,
}

#[derive(Deserialize)]
struct BigModelChatResponse {
    choices: Vec<BigModelChoice>,
}

#[derive(Deserialize)]
struct BigModelChoice {
    message: BigModelMessage,
}

#[derive(Deserialize)]
struct BigModelMessage {
    content: String,
}
