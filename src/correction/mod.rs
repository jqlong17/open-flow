use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::common::config::Config;
use crate::llm::LlmClient;

pub struct TextCorrector {
    llm_client: LlmClient,
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
        let llm_client = match LlmClient::new(api_key, config.resolved_correction_model()) {
            Ok(client) => client,
            Err(err) => {
                warn!("创建文本纠错 LLM client 失败: {}", err);
                return None;
            }
        };

        Some(Self {
            llm_client,
            vocabulary,
        })
    }

    pub async fn correct(&self, raw_text: &str) -> Result<String> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            return Ok(String::new());
        }

        println!(
            "[Pipeline] llm_correction_start model={} raw_chars={} vocab_count={}",
            self.llm_client.model_name(),
            raw_text.chars().count(),
            self.vocabulary.len()
        );
        info!(
            "[Pipeline] stage=llm_correction_start model={} raw_chars={} vocab_count={}",
            self.llm_client.model_name(),
            raw_text.chars().count(),
            self.vocabulary.len()
        );

        let system_prompt = build_system_prompt(&self.vocabulary);
        let corrected = self
            .llm_client
            .generate(&system_prompt, raw_text)
            .await
            .context("调用 BigModel 纠错接口失败")?;

        if corrected.is_empty() {
            warn!("BigModel 纠错返回空结果，回退原文");
            println!(
                "[Pipeline] llm_correction_complete model={} changed=false raw_chars={} corrected_chars={} vocab_count={}",
                self.llm_client.model_name(),
                raw_text.chars().count(),
                raw_text.chars().count(),
                self.vocabulary.len()
            );
            return Ok(raw_text.to_string());
        }

        println!(
            "[Pipeline] llm_correction_complete model={} changed={} raw_chars={} corrected_chars={} vocab_count={}",
            self.llm_client.model_name(),
            corrected != raw_text,
            raw_text.chars().count(),
            corrected.chars().count(),
            self.vocabulary.len()
        );
        info!(
            "文本纠错完成: raw_chars={} corrected_chars={} vocab_count={}",
            raw_text.chars().count(),
            corrected.chars().count(),
            self.vocabulary.len()
        );
        Ok(corrected)
    }

    pub fn model_name(&self) -> &str {
        self.llm_client.model_name()
    }

    pub fn vocabulary_count(&self) -> usize {
        self.vocabulary.len()
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

fn default_system_prompt_template() -> &'static str {
    "你是一个语音转写轻量纠错器。你的唯一任务是修正明显的 ASR 识别错误。\n\
规则：\n\
1. 只修正明显错误，不要改写句式，不要润色，不要总结，不要解释。\n\
2. 不要补充用户没说过的事实，不要扩写。\n\
3. 如果原文已经合理，就原样输出。\n\
4. 优先参考个人词表中的常用词、品牌词、人名、项目名；当原文发音或拼写接近这些词时，可纠正为词表中的标准写法。\n\
5. 保留原有语言风格和中英文混排方式。\n\
6. 最终只输出纠错后的单段文本，不要输出任何说明。\n\
\n个人词表：\n{{personal_vocabulary}}"
}

fn load_custom_system_prompt_template() -> Option<String> {
    let path = Config::correction_system_prompt_path().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

    let template = load_custom_system_prompt_template()
        .unwrap_or_else(|| default_system_prompt_template().to_string());

    if template.contains("{{personal_vocabulary}}") {
        template.replace("{{personal_vocabulary}}", &vocab_block)
    } else {
        format!("{}\n\n个人词表：\n{}", template, vocab_block)
    }
}
