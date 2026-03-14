use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tracing::info;

use super::AsrProvider;
use crate::common::types::TranscriptionResult;

pub struct GroqAsrProvider {
    api_key: String,
    model: String,
    language: String,
    client: reqwest::Client,
}

impl GroqAsrProvider {
    pub fn new(api_key: String, model: String, language: String) -> Result<Self> {
        if api_key.trim().is_empty() {
            anyhow::bail!(
                "Groq API key is required. Set GROQ_API_KEY env var or groq_api_key in config.toml"
            );
        }
        Ok(Self {
            api_key,
            model,
            language,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl AsrProvider for GroqAsrProvider {
    async fn transcribe(&self, audio: &[f32], sample_rate: u32) -> Result<TranscriptionResult> {
        let start = std::time::Instant::now();
        info!(
            "Groq transcription: {} samples @ {}Hz, model={}",
            audio.len(),
            sample_rate,
            self.model
        );

        // Encode PCM f32 to WAV in memory
        let wav_bytes = encode_wav(audio, sample_rate)?;

        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = reqwest::multipart::Form::new()
            .text("model", self.model.clone())
            .part("file", file_part);

        if !self.language.is_empty() {
            form = form.text("language", self.language.clone());
        }

        let res = self
            .client
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("Groq API request failed")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!("Groq transcription failed: {} {}", status, body));
        }

        let body = res.text().await.context("Failed to read Groq response body")?;
        let parsed: GroqTranscriptionResponse =
            serde_json::from_str(&body).context("Failed to parse Groq response JSON")?;

        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            "Groq transcription complete: {}ms, text length={}",
            duration_ms,
            parsed.text.len()
        );

        Ok(TranscriptionResult {
            text: parsed.text,
            confidence: 1.0,
            language: if self.language.is_empty() {
                None
            } else {
                Some(self.language.clone())
            },
            duration_ms,
        })
    }

    fn check_status(&self) -> Result<String> {
        if self.api_key.trim().is_empty() {
            anyhow::bail!("Groq API key not configured")
        }
        Ok(format!("ready (model: {})", self.model))
    }

    fn name(&self) -> &str {
        "groq (Whisper)"
    }
}

#[derive(serde::Deserialize)]
struct GroqTranscriptionResponse {
    text: String,
}

/// Encode f32 PCM samples to WAV bytes in memory.
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    use std::io::Cursor;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).context("Failed to create WAV writer")?;
        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let i16_val = (clamped * 32767.0) as i16;
            writer
                .write_sample(i16_val)
                .context("Failed to write WAV sample")?;
        }
        writer.finalize().context("Failed to finalize WAV")?;
    }

    Ok(cursor.into_inner())
}
