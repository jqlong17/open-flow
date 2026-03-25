use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

use crate::common::config::Config;

const BIGMODEL_CHAT_COMPLETIONS_URL: &str = "https://open.bigmodel.cn/api/paas/v4/chat/completions";
const LLM_REQUEST_TIMEOUT_SECS: u64 = 90;
const LLM_CONNECT_TIMEOUT_SECS: u64 = 12;
const LLM_MAX_ATTEMPTS: usize = 2;

pub struct LlmClient {
    client: Client,
    api_key: String,
    model: String,
}

impl LlmClient {
    pub fn from_config(config: &Config) -> Option<Self> {
        let api_key = config.resolved_correction_api_key();
        if api_key.trim().is_empty() {
            return None;
        }

        Self::new(api_key, config.resolved_correction_model()).ok()
    }

    pub fn new(api_key: String, model: String) -> Result<Self> {
        if api_key.trim().is_empty() {
            anyhow::bail!("缺少 LLM API key");
        }

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(LLM_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(LLM_REQUEST_TIMEOUT_SECS))
            .build()
            .context("创建 LLM HTTP client 失败")?;

        Ok(Self {
            client,
            api_key,
            model: if model.trim().is_empty() {
                "GLM-4.7-Flash".to_string()
            } else {
                model.trim().to_string()
            },
        })
    }

    pub async fn generate(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let request = BigModelChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_prompt.to_string(),
                },
            ],
            stream: false,
            temperature: Some(0.2),
            do_sample: Some(false),
            max_tokens: Some(2048),
            thinking: Some(ThinkingConfig {
                r#type: "disabled".into(),
                clear_thinking: true,
            }),
        };

        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 1..=LLM_MAX_ATTEMPTS {
            let send_result = self
                .client
                .post(BIGMODEL_CHAT_COMPLETIONS_URL)
                .bearer_auth(&self.api_key)
                .json(&request)
                .send()
                .await;

            let response = match send_result {
                Ok(response) => response,
                Err(err) => {
                    let retriable = err.is_timeout() || err.is_connect();
                    let wrapped = if err.is_timeout() {
                        anyhow::anyhow!(
                            "调用 BigModel 接口超时，请检查网络状态或稍后重试。request_url={}",
                            BIGMODEL_CHAT_COMPLETIONS_URL
                        )
                    } else {
                        anyhow::Error::new(err).context("调用 BigModel 接口失败")
                    };
                    last_error = Some(wrapped);

                    if retriable && attempt < LLM_MAX_ATTEMPTS {
                        sleep(Duration::from_millis(800)).await;
                        continue;
                    }
                    break;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status == StatusCode::TOO_MANY_REQUESTS {
                    anyhow::bail!("BigModel 调用失败: 请求过于频繁（429）。{}", body);
                }
                if status == StatusCode::UNAUTHORIZED {
                    anyhow::bail!("BigModel 调用失败: API Key 无效或已失效（401）。{}", body);
                }
                if status.is_server_error() && attempt < LLM_MAX_ATTEMPTS {
                    last_error = Some(anyhow::anyhow!(
                        "BigModel 服务暂时不可用: {} {}",
                        status,
                        body
                    ));
                    sleep(Duration::from_millis(800)).await;
                    continue;
                }
                anyhow::bail!("BigModel 调用失败: {} {}", status, body);
            }

            let parsed: BigModelChatResponse = response
                .json()
                .await
                .context("解析 BigModel 响应失败")?;

            return Ok(parsed
                .choices
                .first()
                .map(|choice| choice.message.content.trim().to_string())
                .unwrap_or_default());
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("调用 BigModel 接口失败")))
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }
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
