use ndarray::{Array2, Axis};
use ort::{
    logging::LogLevel,
    session::{builder::GraphOptimizationLevel, Session},
    value::{Tensor, TensorRef, Value},
};
use std::path::Path;
use tracing::info;

/// 将首输出张量解析为 f32（兼容 float32 与 float16 模型）
fn extract_logits_as_f32(value: &Value) -> anyhow::Result<ndarray::ArrayD<f32>> {
    if let Ok(view) = value.try_extract_array::<f32>() {
        return Ok(view.to_owned().into_dyn());
    }
    let view = value
        .try_extract_array::<half::f16>()
        .map_err(|e: ort::Error| anyhow::anyhow!(e.to_string()))?;
    let vec_f32: Vec<f32> = view.iter().map(|x: &half::f16| x.to_f32()).collect();
    let shape: Vec<usize> = view.shape().to_vec();
    Ok(ndarray::Array::from_shape_vec(shape, vec_f32)?.into_dyn())
}

/// ONNX 推理引擎
pub struct OnnxInference {
    session: Session,
}

impl OnnxInference {
    /// 加载 ONNX 模型
    pub fn new(model_path: &Path) -> anyhow::Result<Self> {
        info!("🧠 加载 ONNX 模型: {:?}", model_path);

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_log_level(LogLevel::Warning)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        info!("✓ ONNX 模型加载成功");
        info!("  输入数: {}", session.inputs().len());
        info!("  输出数: {}", session.outputs().len());
        for input in session.inputs() {
            info!("  输入: {} {:?}", input.name(), input.dtype());
        }
        for output in session.outputs() {
            info!("  输出: {} {:?}", output.name(), output.dtype());
        }

        Ok(Self { session })
    }

    /// 运行推理，返回 (logits, encoder_out_lens)
    pub fn infer(
        &mut self,
        features: &Array2<f32>,
        language_id: i32,
        textnorm_id: i32,
    ) -> anyhow::Result<(Array2<f32>, Vec<i32>)> {
        let speech = TensorRef::from_array_view(features.view().insert_axis(Axis(0)))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let speech_lengths = Tensor::from_array(([1usize], vec![features.nrows() as i32]))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let language = Tensor::from_array(([1usize], vec![language_id]))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let textnorm = Tensor::from_array(([1usize], vec![textnorm_id]))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs! {
                "speech" => speech,
                "speech_lengths" => speech_lengths,
                "language" => language,
                "textnorm" => textnorm
            })
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // 支持 float32 与 float16 输出（FP16 模型经图修复后首输出多为 f32，少数仍为 f16）
        let output = extract_logits_as_f32(&outputs[0])?;

        let encoder_out_lens: Vec<i32> = if outputs.len() > 1 {
            outputs[1]
                .try_extract_array::<i32>()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
                .iter()
                .copied()
                .collect()
        } else {
            vec![output.shape()[1] as i32]
        };

        let valid_len = encoder_out_lens
            .first()
            .copied()
            .unwrap_or(output.shape()[1] as i32)
            .max(1) as usize;

        let mut output_2d = match output.ndim() {
            2 => output.into_dimensionality::<ndarray::Ix2>()?.to_owned(),
            3 => output
                .into_dimensionality::<ndarray::Ix3>()?
                .index_axis(Axis(0), 0)
                .to_owned(),
            ndim => anyhow::bail!("未预期的输出维度: {ndim}"),
        };

        if valid_len < output_2d.nrows() {
            output_2d = output_2d.slice(ndarray::s![0..valid_len, ..]).to_owned();
        }

        // SenseVoice 在 encoder 前拼接了 4 个 embed（language, emotion, event, itn），
        // 前 4 帧输出对应这些控制 token，CTC 解码应跳过
        const SENSEVOICE_CTC_SKIP_FRAMES: usize = 4;
        if output_2d.nrows() > SENSEVOICE_CTC_SKIP_FRAMES {
            output_2d = output_2d
                .slice(ndarray::s![SENSEVOICE_CTC_SKIP_FRAMES.., ..])
                .to_owned();
        }

        Ok((output_2d, encoder_out_lens))
    }
}
