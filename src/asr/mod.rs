pub mod decoder;
pub mod groq;
pub mod onnx_inference;
pub mod preprocess;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::asr::decoder::CTCDecoder;
use crate::asr::onnx_inference::OnnxInference;
use crate::asr::preprocess::{AudioPreprocessor, TARGET_SAMPLE_RATE};
use crate::common::types::TranscriptionResult;

/// Trait abstracting speech recognition backends (local ONNX vs cloud API).
#[async_trait]
pub trait AsrProvider: Send + Sync {
    /// Transcribe PCM audio samples. Returns transcribed text.
    async fn transcribe(&self, audio: &[f32], sample_rate: u32) -> Result<TranscriptionResult>;

    /// Optional warmup (e.g. load model, establish connection). Default no-op.
    async fn warmup(&self) -> Result<()> {
        Ok(())
    }

    /// Check provider readiness. Returns human-readable status string on success.
    fn check_status(&self) -> Result<String> {
        Ok("ready".into())
    }

    /// Provider name for display purposes.
    fn name(&self) -> &str;
}

/// 调试与调参用环境变量（可开关）：
/// - OPEN_FLOW_DEBUG_ASR: 打印 features shape、encoder_out_lens、logits 首/中/末帧 top-k 及 non_blank 帧数
/// - OPEN_FLOW_LFR_LEFT_PAD=0: 关闭 LFR 左填充
/// - OPEN_FLOW_SKIP_CMVN: 跳过 CMVN
/// - OPEN_FLOW_BEST_NON_BLANK=0: 使用纯 CTC argmax（否则每帧取最佳非 blank，便于在 blank 过强时得到非空结果）
/// - OPEN_FLOW_LANG_ID, OPEN_FLOW_TEXTNORM_ID: 覆盖 language/textnorm 输入
pub struct AsrEngine {
    model_path: PathBuf,
    preprocessor: Option<AudioPreprocessor>,
    inference: Option<OnnxInference>,
    decoder: Option<CTCDecoder>,
    ready: bool,
}

impl AsrEngine {
    /// SenseVoice ONNX language id：与 FunASR/sherpa 导出一致（lid_dict）
    /// auto=0, zh=3, en=4, yue=5, ja=6, ko=7, nospeech=8
    fn resolve_language_id(language: Option<&str>) -> i32 {
        if let Ok(v) = std::env::var("OPEN_FLOW_LANG_ID") {
            if let Ok(parsed) = v.parse::<i32>() {
                return parsed;
            }
        }
        match language.unwrap_or("auto") {
            "auto" => 0,
            "zh" | "zh-cn" | "cn" => 3,
            "en" => 4,
            "yue" => 5,
            "ja" => 6,
            "ko" => 7,
            "nospeech" => 8,
            _ => 0,
        }
    }

    /// SenseVoice ONNX textnorm id：与 FunASR 导出一致（textnorm_dict）
    /// 0=woitn(无逆文本正则), 1=withitn(有逆文本正则)。可通过 OPEN_FLOW_TEXTNORM_ID 覆盖
    fn resolve_textnorm_id() -> i32 {
        if let Ok(v) = std::env::var("OPEN_FLOW_TEXTNORM_ID") {
            if let Ok(parsed) = v.parse::<i32>() {
                return parsed;
            }
        }
        0
    }

    /// 实时口述场景默认优先中文，避免 auto 在短句上误判成韩语/空白。
    /// 仍可通过 OPEN_FLOW_LANG_ID 显式覆盖。
    fn resolve_live_language_id() -> i32 {
        if std::env::var("OPEN_FLOW_LANG_ID").is_ok() {
            return Self::resolve_language_id(Some("auto"));
        }
        Self::resolve_language_id(Some("zh"))
    }

    /// 创建新的 ASR 引擎
    pub fn new(model_path: PathBuf) -> Self {
        info!("🧠 ASR 引擎初始化: {:?}", model_path);

        let mut engine = Self {
            model_path,
            preprocessor: None,
            inference: None,
            decoder: None,
            ready: false,
        };

        // 尝试加载模型
        if let Err(e) = engine.load_model() {
            warn!("⚠️  模型加载失败: {}", e);
            warn!("   将使用模拟模式");
        }

        engine
    }

    /// 查找 ONNX 模型文件（支持 model.onnx 和 model_quant.onnx 两种文件名）
    fn find_model_file(model_path: &Path) -> Option<PathBuf> {
        for name in &["model.onnx", "model_quant.onnx"] {
            let p = model_path.join(name);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// 加载模型
    fn load_model(&mut self) -> Result<()> {
        let model_file = Self::find_model_file(&self.model_path)
            .ok_or_else(|| anyhow::anyhow!("模型文件不存在（已查找 model.onnx / model_quant.onnx）: {:?}", self.model_path))?;
        let tokens_file = self.model_path.join("tokens.json");

        if !tokens_file.exists() {
            anyhow::bail!("tokens 文件不存在: {:?}", tokens_file);
        }

        info!("🔄 正在加载模型组件...");

        // 1. 加载预处理器
        let mut pre = AudioPreprocessor::new(TARGET_SAMPLE_RATE);
        let cmvn_file = self.model_path.join("am.mvn");
        if cmvn_file.exists() {
            pre.load_cmvn_from_file(&cmvn_file)?;
            info!("✓ CMVN 加载完成: {:?}", cmvn_file);
        } else {
            warn!("⚠️ 未找到 am.mvn，识别精度可能下降: {:?}", cmvn_file);
        }
        self.preprocessor = Some(pre);
        info!("✓ 预处理器加载完成");

        // 2. 加载 ONNX 模型
        self.inference = Some(OnnxInference::new(&model_file)?);
        info!("✓ ONNX 模型加载完成");

        // 3. 加载解码器
        self.decoder = Some(CTCDecoder::from_tokens_file(&tokens_file)?);
        info!("✓ CTC 解码器加载完成");

        self.ready = true;
        info!("🎉 ASR 引擎完全就绪！");

        Ok(())
    }

    /// 预热：用 1 秒静音跑一次完整推理，消除 ORT JIT 首次编译开销。
    /// 在 daemon 就绪前调用，使第一次真实转写延迟正常。
    pub fn warmup(&mut self) {
        if !self.ready {
            return;
        }
        let silence = vec![0.0f32; 16000]; // 1 秒 16kHz 静音
        let preprocessor = self.preprocessor.as_ref().unwrap();
        if let Ok(features) = preprocessor.process(&silence, 16000) {
            let inference = self.inference.as_mut().unwrap();
            let _ = inference.infer(&features, 0, 0); // language=auto, textnorm=off
        }
        info!("✓ 模型预热完成");
    }

    /// 直接从内存 PCM 数据转写，避免磁盘 I/O round-trip。
    /// samples: 单声道 f32 样本（已混音）；sample_rate: 采样率（Hz）
    pub fn transcribe_pcm(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<TranscriptionResult> {
        let start = Instant::now();

        info!("📝 开始转写（内存 PCM，{} 样本，{}Hz）", samples.len(), sample_rate);

        if !self.ready {
            warn!("⚠️  模型未就绪，使用模拟转写");
            return Ok(self.mock_transcribe());
        }

        // 1. 预处理
        let preprocessor = self.preprocessor.as_ref().unwrap();
        let features = preprocessor.process(samples, sample_rate)?;
        if std::env::var("OPEN_FLOW_DEBUG_ASR").is_ok() {
            info!("[DEBUG] features shape: {:?}", features.dim());
        }
        info!("✓ 特征提取完成: {:?}", features.dim());

        // 2. ONNX 推理
        let inference = self.inference.as_mut().unwrap();
        let language_id = Self::resolve_live_language_id();
        let textnorm_id = Self::resolve_textnorm_id();
        info!(
            "ASR 推理参数: language_id={} textnorm_id={}",
            language_id, textnorm_id
        );
        let (logits, encoder_out_lens) = inference.infer(&features, language_id, textnorm_id)?;
        if std::env::var("OPEN_FLOW_DEBUG_ASR").is_ok() {
            info!("[DEBUG] encoder_out_lens: {:?}", encoder_out_lens);
        }
        info!("✓ 推理完成: {:?}", logits.dim());

        // 3. CTC 解码
        let decoder = self.decoder.as_ref().unwrap();
        let text = decoder.decode(&logits, std::env::var("OPEN_FLOW_DEBUG_ASR").is_ok());
        info!("✓ 解码完成: {}", text);

        Ok(TranscriptionResult {
            text,
            confidence: 0.95,
            language: Some("zh".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// 转录音频文件（供 `transcribe` CLI 命令使用，内部复用 transcribe_pcm）
    pub fn transcribe(
        &mut self,
        audio_path: &Path,
        language: Option<&str>,
    ) -> Result<TranscriptionResult> {
        info!("📝 开始转写（文件）: {:?}", audio_path);

        if !audio_path.exists() {
            anyhow::bail!("音频文件不存在: {:?}", audio_path);
        }

        if !self.ready {
            warn!("⚠️  模型未就绪，使用模拟转写");
            return Ok(self.mock_transcribe());
        }

        let audio = self.load_audio(audio_path)?;
        info!("✓ 音频加载完成: {} 样本", audio.data.len());

        // language 参数仅文件转写路径使用，内存路径固定 "auto"
        let _ = language; // 当前固定用 "auto"，保留参数供后续扩展
        self.transcribe_pcm(&audio.data, audio.sample_rate)
    }

    /// 加载音频文件
    fn load_audio(&self, path: &Path) -> Result<AudioData> {
        use hound::WavReader;

        let reader =
            WavReader::open(path).with_context(|| format!("无法打开音频文件: {:?}", path))?;

        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        info!("音频文件信息:");
        info!("  采样率: {}Hz", sample_rate);
        info!("  通道数: {}", channels);
        info!("  位深: {} bits", spec.bits_per_sample);

        // 读取样本并转换为 f32
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect(),
            hound::SampleFormat::Int => {
                let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
        };

        // 如果是多通道，转为单通道（取平均）
        let mono_samples: Vec<f32> = if channels > 1 {
            samples
                .chunks(channels)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            samples
        };

        Ok(AudioData {
            data: mono_samples,
            sample_rate,
        })
    }

    /// 模拟转写（模型未就绪时使用）
    fn mock_transcribe(&self) -> TranscriptionResult {
        std::thread::sleep(std::time::Duration::from_millis(500));

        TranscriptionResult {
            text: "[模拟转写结果] 你好，这是一个测试。".to_string(),
            confidence: 0.95,
            language: Some("zh".to_string()),
            duration_ms: 500,
        }
    }

    /// 检查 ASR 引擎状态
    pub fn check_status(&self) -> AsrStatus {
        let model_exists = self.model_path.exists();
        let onnx_exists = Self::find_model_file(&self.model_path).is_some();
        let tokens_exists = self.model_path.join("tokens.json").exists();

        AsrStatus {
            model_path: self.model_path.clone(),
            model_exists,
            onnx_exists,
            tokens_exists,
            ready: self.ready,
        }
    }
}

/// 音频数据结构
struct AudioData {
    data: Vec<f32>,
    sample_rate: u32,
}

/// ASR 状态
#[derive(Debug, Clone)]
pub struct AsrStatus {
    pub model_path: PathBuf,
    pub model_exists: bool,
    pub onnx_exists: bool,
    #[allow(dead_code)]
    pub tokens_exists: bool,
    pub ready: bool,
}

/// Local ASR provider wrapping the existing ONNX-based AsrEngine.
/// Uses Arc<Mutex<>> so the engine can be moved into spawn_blocking.
pub struct LocalAsrProvider {
    engine: Arc<Mutex<AsrEngine>>,
}

impl LocalAsrProvider {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            engine: Arc::new(Mutex::new(AsrEngine::new(model_path))),
        }
    }
}

#[async_trait]
impl AsrProvider for LocalAsrProvider {
    async fn transcribe(&self, audio: &[f32], sample_rate: u32) -> Result<TranscriptionResult> {
        let audio = audio.to_vec();
        let engine = self.engine.clone();
        tokio::task::spawn_blocking(move || {
            engine.lock().unwrap().transcribe_pcm(&audio, sample_rate)
        })
        .await
        .context("spawn_blocking failed")?
    }

    async fn warmup(&self) -> Result<()> {
        let engine = self.engine.clone();
        tokio::task::spawn_blocking(move || {
            engine.lock().unwrap().warmup();
        })
        .await
        .context("warmup spawn_blocking failed")?;
        Ok(())
    }

    fn check_status(&self) -> Result<String> {
        let status = self.engine.lock().unwrap().check_status();
        if status.ready {
            Ok("ready".into())
        } else {
            anyhow::bail!(
                "Model not ready: {:?} (onnx={}, model={})",
                status.model_path,
                status.onnx_exists,
                status.model_exists
            )
        }
    }

    fn name(&self) -> &str {
        "local (SenseVoice)"
    }
}

#[cfg(test)]
mod regression_tests {
    use super::AsrEngine;
    use std::path::Path;

    /// 固定音频回归：需设置 OPEN_FLOW_REGRESSION_MODEL 指向 SenseVoice 目录，运行 cargo test regression_mixed_zh_en -- --ignored --nocapture
    #[test]
    #[ignore]
    fn regression_mixed_zh_en() {
        let model_path = match std::env::var("OPEN_FLOW_REGRESSION_MODEL") {
            Ok(p) => Path::new(&p).to_path_buf(),
            Err(_) => return,
        };
        let wav = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/mixed_zh_en.wav");
        if !wav.exists() {
            eprintln!("跳过回归：testdata/mixed_zh_en.wav 不存在");
            return;
        }
        if AsrEngine::find_model_file(&model_path).is_none() {
            eprintln!("跳过回归：模型目录无 model.onnx / model_quant.onnx");
            return;
        }
        let mut engine = AsrEngine::new(model_path);
        let result = engine.transcribe(&wav, Some("auto")).expect("transcribe");
        assert!(!result.text.is_empty(), "回归要求输出非空");
    }
}
