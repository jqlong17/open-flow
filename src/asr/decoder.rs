use ndarray::Array2;
use std::collections::HashMap;
use tracing::info;

fn is_special_token(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.starts_with("<|") && s.ends_with("|>") {
        return true;
    }
    let lower = s.to_lowercase();
    matches!(
        lower.as_str(),
        "<unk>" | "<s>" | "</s>" | "<blank>" | "<blk>" | "<space>"
    )
}

fn postprocess_tokens(tokens: Vec<String>) -> String {
    let s = tokens.join("").replace('▁', " ").replace("<space>", " ");
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// CTC 解码器
pub struct CTCDecoder {
    #[allow(dead_code)]
    token_to_id: HashMap<String, i32>,
    id_to_token: HashMap<i32, String>,
    blank_id: i32,
    has_explicit_blank: bool,
}

impl CTCDecoder {
    /// 从 tokens.json 文件创建解码器
    pub fn from_tokens_file(tokens_path: &std::path::Path) -> anyhow::Result<Self> {
        info!("📖 加载 tokens 文件: {:?}", tokens_path);

        let content = std::fs::read_to_string(tokens_path)?;
        let tokens: serde_json::Value = serde_json::from_str(&content)?;

        let mut token_to_id = HashMap::new();
        let mut id_to_token = HashMap::new();

        if let Some(obj) = tokens.as_object() {
            for (token, id_val) in obj {
                if let Some(id) = id_val.as_i64() {
                    let id_i32 = id as i32;
                    token_to_id.insert(token.clone(), id_i32);
                    id_to_token.insert(id_i32, token.clone());
                }
            }
        } else if let Some(arr) = tokens.as_array() {
            for (id, token_val) in arr.iter().enumerate() {
                if let Some(token) = token_val.as_str() {
                    let id_i32 = id as i32;
                    token_to_id.insert(token.to_string(), id_i32);
                    id_to_token.insert(id_i32, token.to_string());
                }
            }
        }

        if token_to_id.is_empty() {
            anyhow::bail!("tokens.json 解析失败：既不是 token->id 映射，也不是 token 列表");
        }

        let blank_token_id = token_to_id
            .get("<blank>")
            .copied()
            .or_else(|| token_to_id.get("<blk>").copied());
        let has_explicit_blank = blank_token_id.is_some();
        let blank_id = blank_token_id.unwrap_or(0);

        info!("✓ Tokens 加载完成: {} 个 tokens", token_to_id.len());
        info!("  Blank ID: {}", blank_id);

        Ok(Self {
            token_to_id,
            id_to_token,
            blank_id,
            has_explicit_blank,
        })
    }

    /// CTC 贪婪解码。debug 为 true 时打印前几帧的 top-k token
    pub fn decode(&self, logits: &Array2<f32>, debug: bool) -> String {
        let num_frames = logits.nrows();
        let num_classes = logits.ncols();

        let mut frame_ids = Vec::with_capacity(num_frames);
        for i in 0..num_frames {
            let mut max_prob = f32::NEG_INFINITY;
            let mut max_id = 0i32;
            for j in 0..num_classes {
                let prob = logits[[i, j]];
                if prob > max_prob {
                    max_prob = prob;
                    max_id = j as i32;
                }
            }
            frame_ids.push(max_id);
        }

        if debug && logits.nrows() > 0 {
            let k = 5.min(logits.ncols());
            let frames_to_show = [0, logits.nrows() / 2, logits.nrows().saturating_sub(1)];
            for &frame_idx in &frames_to_show {
                if frame_idx >= logits.nrows() {
                    continue;
                }
                let mut probs: Vec<(usize, f32)> = (0..logits.ncols())
                    .map(|j| (j, logits[[frame_idx, j]]))
                    .collect();
                probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let top: Vec<String> = probs
                    .iter()
                    .take(k)
                    .map(|(id, p)| {
                        let t = self
                            .id_to_token
                            .get(&(*id as i32))
                            .cloned()
                            .unwrap_or_else(|| "?".to_string());
                        format!("{}:{:.2}", t, p)
                    })
                    .collect();
                tracing::info!(
                    "[DEBUG] logits frame {} top-{}: {}",
                    frame_idx,
                    k,
                    top.join(", ")
                );
            }
            let blank = if self.has_explicit_blank {
                self.blank_id
            } else {
                0
            };
            let non_blank: usize = frame_ids.iter().filter(|&&id| id != blank).count();
            tracing::info!(
                "[DEBUG] non_blank frames: {} / {}",
                non_blank,
                frame_ids.len()
            );
        }

        let blank_id_use = if self.has_explicit_blank {
            self.blank_id
        } else {
            0
        };

        // 当 blank(含<unk>) 过强时可选用每帧“最佳非 blank”以得到非空结果；设 OPEN_FLOW_BEST_NON_BLANK=0 恢复纯 argmax
        // 纯 CTC argmax；设 OPEN_FLOW_BEST_NON_BLANK=1 可开启调试模式
        let use_best_non_blank = std::env::var("OPEN_FLOW_BEST_NON_BLANK")
            .map(|v| v == "1")
            .unwrap_or(false);

        let mut prev_id = -1i32;
        let mut result = Vec::new();

        for (frame_idx, &max_id) in frame_ids.iter().enumerate() {
            let id_to_emit = if max_id == blank_id_use && use_best_non_blank {
                let mut best = (0i32, f32::NEG_INFINITY);
                for j in 0..logits.ncols() {
                    if j as i32 == blank_id_use {
                        continue;
                    }
                    let p = logits[[frame_idx, j]];
                    if p > best.1 {
                        best = (j as i32, p);
                    }
                }
                best.0
            } else {
                max_id
            };

            if id_to_emit == blank_id_use || id_to_emit == prev_id {
                prev_id = if max_id == blank_id_use {
                    max_id
                } else {
                    id_to_emit
                };
                continue;
            }
            if let Some(token) = self.id_to_token.get(&id_to_emit) {
                if is_special_token(token) {
                    prev_id = id_to_emit;
                    continue;
                }
                result.push(token.clone());
            }
            prev_id = id_to_emit;
        }

        postprocess_tokens(result)
    }

    /// 获取 token 数
    #[allow(dead_code)]
    pub fn vocab_size(&self) -> usize {
        self.token_to_id.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctc_decode_simple() {
        // 创建简单的测试 tokens
        let mut token_to_id = HashMap::new();
        let mut id_to_token = HashMap::new();

        token_to_id.insert("<blank>".to_string(), 0);
        token_to_id.insert("a".to_string(), 1);
        token_to_id.insert("b".to_string(), 2);

        id_to_token.insert(0, "<blank>".to_string());
        id_to_token.insert(1, "a".to_string());
        id_to_token.insert(2, "b".to_string());

        let decoder = CTCDecoder {
            token_to_id,
            id_to_token,
            blank_id: 0,
            has_explicit_blank: true,
        };

        // 创建模拟的 logits: [frames, classes]
        // 假设 5 帧，3 个类别（blank=0, a=1, b=2）
        let logits = Array2::from_shape_vec(
            (5, 3),
            vec![
                1.0, 0.0, 0.0, // blank
                0.0, 1.0, 0.0, // a
                1.0, 0.0, 0.0, // blank
                0.0, 0.0, 1.0, // b
                1.0, 0.0, 0.0, // blank
            ],
        )
        .unwrap();

        let result = decoder.decode(&logits, false);
        assert_eq!(result, "ab");
    }

    fn make_decoder(tokens: &[(&str, i32)]) -> CTCDecoder {
        let mut token_to_id = HashMap::new();
        let mut id_to_token = HashMap::new();
        for (t, id) in tokens {
            token_to_id.insert(t.to_string(), *id);
            id_to_token.insert(*id, t.to_string());
        }
        let blank_id = token_to_id.get("<blank>").copied().unwrap_or(0);
        let has_explicit_blank = token_to_id.contains_key("<blank>");
        CTCDecoder {
            token_to_id,
            id_to_token,
            blank_id,
            has_explicit_blank,
        }
    }

    /// 全 blank → 空字符串
    #[test]
    fn test_decode_all_blank_returns_empty() {
        let dec = make_decoder(&[("<blank>", 0), ("你", 1), ("好", 2)]);
        let logits = Array2::from_shape_vec(
            (4, 3),
            vec![
                1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            ],
        )
        .unwrap();
        assert_eq!(dec.decode(&logits, false), "");
    }

    /// 0 帧 logits → 空字符串，不 panic
    #[test]
    fn test_decode_empty_logits_no_panic() {
        let dec = make_decoder(&[("<blank>", 0), ("a", 1)]);
        let logits = Array2::zeros((0, 2));
        assert_eq!(dec.decode(&logits, false), "");
    }

    /// 特殊 token（<|zh|> 等）不出现在结果中
    #[test]
    fn test_decode_special_tokens_filtered() {
        // 模拟 SenseVoice 输出中含语言/情感 token
        let dec = make_decoder(&[
            ("<blank>", 0),
            ("<|zh|>", 1),
            ("<|HAPPY|>", 2),
            ("你", 3),
            ("好", 4),
        ]);
        // 帧序列：lang_tag, emotion_tag, 你, 好
        let logits = Array2::from_shape_vec(
            (4, 5),
            vec![
                0.0f32, 1.0, 0.0, 0.0, 0.0, // <|zh|>
                0.0, 0.0, 1.0, 0.0, 0.0, // <|HAPPY|>
                0.0, 0.0, 0.0, 1.0, 0.0, // 你
                0.0, 0.0, 0.0, 0.0, 1.0, // 好
            ],
        )
        .unwrap();
        let result = dec.decode(&logits, false);
        assert!(
            !result.contains("<|"),
            "特殊 token 不应出现在结果中，got: {:?}",
            result
        );
        assert!(
            result.contains("你") && result.contains("好"),
            "正常汉字应出现在结果中，got: {:?}",
            result
        );
    }

    /// 连续相同 token 合并（CTC 规则）
    #[test]
    fn test_decode_consecutive_same_token_collapsed() {
        let dec = make_decoder(&[("<blank>", 0), ("a", 1)]);
        // a a a blank a → "aa"（三个连续 a 合并为 1，blank 后的 a 是新的）
        let logits = Array2::from_shape_vec(
            (5, 2),
            vec![
                0.0f32, 1.0, // a
                0.0, 1.0, // a
                0.0, 1.0, // a
                1.0, 0.0, // blank
                0.0, 1.0, // a
            ],
        )
        .unwrap();
        assert_eq!(dec.decode(&logits, false), "aa");
    }

    /// ▁ 前缀正确转换为空格
    #[test]
    fn test_decode_triangle_space_prefix() {
        let dec = make_decoder(&[("<blank>", 0), ("▁hello", 1), ("▁world", 2)]);
        let logits = Array2::from_shape_vec((2, 3), vec![0.0f32, 1.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
        assert_eq!(dec.decode(&logits, false), "hello world");
    }
}
