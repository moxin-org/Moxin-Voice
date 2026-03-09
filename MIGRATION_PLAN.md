# 迁移执行计划：dora-primespeech → gpt-sovits-mlx（纯 Rust Dora 节点）

> ⚠️ 本文档已并入统一文档：`MLX_CORE_MIGRATION.md`。  
> 当前请优先参考：`./MLX_CORE_MIGRATION.md`。

> 制定日期：2026-03-06
> 目标：替换 Python TTS 节点为纯 Rust 实现，保留全部现有功能

---

## 功能保留清单（必须全部验证通过）

| 功能                    | 当前实现                | 迁移后实现                 |
| ----------------------- | ----------------------- | -------------------------- |
| 预置音色 TTS（15 个）   | dora-primespeech Python | moxin-tts-node Rust        |
| Express Mode 零样本克隆 | `VOICE:CUSTOM\|` 协议   | 相同协议，Rust 解析        |
| Pro Mode 训练音色       | `VOICE:TRAINED\|` 协议  | 相同协议，Rust 解析        |
| ASR 语音识别            | dora-asr Python         | **保持不变**（本次不迁移） |
| 音色实时切换            | Python 模型热切换       | Rust VoiceCloner 重载      |

---

## 目录结构总览

```
Moxin-Voice/
├── node-hub/
│   ├── dora-primespeech/          # 保留（迁移完成后停用）
│   ├── dora-asr/                  # 保留不动
│   └── moxin-tts-node/            # ← Phase 2 新建
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── protocol.rs
│           ├── voice_registry.rs
│           └── voice_state.rs
├── apps/moxin-voice/dataflow/
│   └── tts.yml                    # ← Phase 3 修改
└── Cargo.toml                     # ← Phase 2 添加成员

~/.OminiX/models/gpt-sovits-mlx/   # ← Phase 1 创建
├── encoders/
│   ├── hubert.safetensors
│   └── bert.safetensors
├── bert-tokenizer/
│   └── tokenizer.json
├── voices/
│   ├── voices.json               # 音色注册表
│   ├── Doubao/
│   │   ├── gpt.safetensors
│   │   ├── sovits.safetensors
│   │   └── reference.wav
│   ├── LuoXiang/
│   │   └── ...
│   └── ...(共 15 个音色目录)
└── (可选) vits.onnx
```

---

## Phase 1：模型转换

> 前提：`conda activate moxin-studio`（或项目 Python 环境），已安装 `mlx`、`torch`、`safetensors`

### Step 1.1：准备转换环境

```bash
cd /Users/alan0x/Documents/projects/OminiX-MLX/gpt-sovits-mlx

# 安装转换依赖
pip install torch safetensors numpy

# 创建目标目录结构
mkdir -p ~/.OminiX/models/gpt-sovits-mlx/encoders
mkdir -p ~/.OminiX/models/gpt-sovits-mlx/bert-tokenizer
mkdir -p ~/.OminiX/models/gpt-sovits-mlx/voices
```

### Step 1.2：转换共享编码器（每类只需做一次）

```bash
SRC=~/.dora/models/primespeech/moyoyo
DST=~/.OminiX/models/gpt-sovits-mlx

# 转换 CNHuBERT（零样本克隆需要）
python scripts/convert_cnhubert_coreml.py \
    --input $SRC/chinese-hubert-base \
    --output $DST/encoders/hubert.safetensors

# 转换中文 BERT（音素编码需要）
python scripts/convert_roberta_coreml.py \
    --input $SRC/chinese-roberta-wwm-ext-large \
    --output $DST/encoders/bert.safetensors

# 复制 BERT tokenizer
cp -r $SRC/chinese-roberta-tokenizer/ $DST/bert-tokenizer/
```

### Step 1.3：批量转换所有预置音色

以下 15 个音色逐一转换，格式统一：

```bash
SCRIPT=/Users/alan0x/Documents/projects/OminiX-MLX/gpt-sovits-mlx/scripts/convert_gpt_weights.py
SRC=~/.dora/models/primespeech/moyoyo
DST=~/.OminiX/models/gpt-sovits-mlx/voices

# 使用如下函数批量转换
convert_voice() {
    local name=$1        # 目标目录名（与 voices.json key 一致）
    local gpt_src=$2     # 源 GPT .ckpt 路径（相对于 $SRC）
    local sovits_src=$3  # 源 SoVITS .pth 路径（相对于 $SRC）
    local ref_src=$4     # 源参考音频路径（相对于 $SRC）

    mkdir -p $DST/$name

    echo "Converting $name GPT weights..."
    python $SCRIPT \
        --input $SRC/$gpt_src \
        --output $DST/$name/gpt.safetensors

    echo "Converting $name SoVITS weights..."
    python $SCRIPT \
        --input $SRC/$sovits_src \
        --output $DST/$name/sovits.safetensors \
        --type sovits

    echo "Copying reference audio..."
    cp $SRC/$ref_src $DST/$name/reference.wav

    echo "Done: $name"
}

# 执行 15 个音色转换
convert_voice "Doubao"     "GPT_weights/doubao-mixed.ckpt"          "SoVITS_weights/doubao-mixed.pth"          "ref_audios/doubao_ref_mix_new.wav"
convert_voice "LuoXiang"   "GPT_weights/luoxiang_best_gpt.ckpt"     "SoVITS_weights/luoxiang_best_sovits.pth"  "ref_audios/luoxiang_ref.wav"
convert_voice "YangMi"     "GPT_weights/yangmi_best_gpt.ckpt"       "SoVITS_weights/yangmi_best_sovits.pth"    "ref_audios/yangmi_ref.wav"
convert_voice "ZhouJielun" "GPT_weights/zjl_best_gpt.ckpt"          "SoVITS_weights/zjl_best_sovits.pth"       "ref_audios/zjl_ref.wav"
convert_voice "MaYun"      "GPT_weights/mayun_best_gpt.ckpt"        "SoVITS_weights/mayun_best_sovits.pth"     "ref_audios/mayun_ref.wav"
convert_voice "ChenYifan"  "GPT_weights/chenyifan_best_gpt.ckpt"    "SoVITS_weights/chenyifan_best_sovits.pth" "ref_audios/chenyifan_ref.wav"
convert_voice "ZhaoDaniu"  "GPT_weights/zhaodaniu_best_gpt.ckpt"    "SoVITS_weights/zhaodaniu_best_sovits.pth" "ref_audios/zhaodaniu_ref.wav"
convert_voice "Maple"      "GPT_weights/maple_best_gpt.ckpt"        "SoVITS_weights/maple_best_sovits.pth"     "ref_audios/maple_ref.wav"
convert_voice "Cove"       "GPT_weights/cove_best_gpt.ckpt"         "SoVITS_weights/cove_best_sovits.pth"      "ref_audios/cove_ref.wav"
convert_voice "BYS"        "GPT_weights/bys_best_gpt.ckpt"          "SoVITS_weights/bys_best_sovits.pth"       "ref_audios/bys_ref.wav"
convert_voice "Ellen"      "GPT_weights/ellen_best_gpt.ckpt"        "SoVITS_weights/ellen_best_sovits.pth"     "ref_audios/ellen_ref.wav"
convert_voice "Juniper"    "GPT_weights/juniper_best_gpt.ckpt"      "SoVITS_weights/juniper_best_sovits.pth"   "ref_audios/juniper_ref.wav"
convert_voice "MaBaoguo"   "GPT_weights/mabaoguo_best_gpt.ckpt"     "SoVITS_weights/mabaoguo_best_sovits.pth"  "ref_audios/mabaoguo_ref.wav"
convert_voice "ShenYi"     "GPT_weights/shenyi_best_gpt.ckpt"       "SoVITS_weights/shenyi_best_sovits.pth"    "ref_audios/shenyi_ref.wav"
convert_voice "Trump"      "GPT_weights/trump_best_gpt.ckpt"        "SoVITS_weights/trump_best_sovits.pth"     "ref_audios/trump_ref.wav"
```

> ⚠️ **注意**：上述路径中 GPT/SoVITS 文件名需根据 config.py 中 VOICE_CONFIGS 的实际值核对，执行前先用 `ls $SRC/GPT_weights/` 确认。

### Step 1.4：创建 voices.json（音色注册表）

```bash
cat > ~/.OminiX/models/gpt-sovits-mlx/voices/voices.json << 'EOF'
{
  "Doubao": {
    "gpt": "Doubao/gpt.safetensors",
    "sovits": "Doubao/sovits.safetensors",
    "reference": "Doubao/reference.wav",
    "prompt_text": "这家resturant的steak很有名，但是vegetable salad的price有点贵",
    "language": "zh",
    "speed_factor": 1.1
  },
  "LuoXiang": {
    "gpt": "LuoXiang/gpt.safetensors",
    "sovits": "LuoXiang/sovits.safetensors",
    "reference": "LuoXiang/reference.wav",
    "prompt_text": "（从 config.py 复制对应 prompt_text）",
    "language": "zh",
    "speed_factor": 1.0
  },
  "Trump": {
    "gpt": "Trump/gpt.safetensors",
    "sovits": "Trump/sovits.safetensors",
    "reference": "Trump/reference.wav",
    "prompt_text": "（从 config.py 复制对应 prompt_text）",
    "language": "en",
    "speed_factor": 1.1
  }
  // ... 其余 12 个音色，prompt_text 从 config.py:78-230 逐一复制
}
EOF
```

### Step 1.5：验证转换结果

```bash
# 检查目录结构完整性
for voice in Doubao LuoXiang YangMi ZhouJielun MaYun ChenYifan ZhaoDaniu Maple Cove BYS Ellen Juniper MaBaoguo ShenYi Trump; do
    dir=~/.OminiX/models/gpt-sovits-mlx/voices/$voice
    [ -f "$dir/gpt.safetensors" ] && echo "✅ $voice/gpt" || echo "❌ $voice/gpt MISSING"
    [ -f "$dir/sovits.safetensors" ] && echo "✅ $voice/sovits" || echo "❌ $voice/sovits MISSING"
    [ -f "$dir/reference.wav" ] && echo "✅ $voice/ref" || echo "❌ $voice/ref MISSING"
done

# 快速验证 Doubao 可推理（使用 gpt-sovits-mlx 自带的示例）
cd /Users/alan0x/Documents/projects/OminiX-MLX
cargo run --example voice_clone --release -- \
    --text "你好，这是测试" \
    --output /tmp/test_doubao.wav
```

---

## Phase 2：创建 Rust Dora TTS 节点

### Step 2.1：创建 Crate 目录

```bash
mkdir -p /Users/alan0x/Documents/projects/Moxin-Voice/node-hub/moxin-tts-node/src
```

### Step 2.2：Cargo.toml

**文件**：`node-hub/moxin-tts-node/Cargo.toml`

```toml
[package]
name = "moxin-tts-node"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "moxin-tts-node"
path = "src/main.rs"

[dependencies]
# Dora 节点 API
dora-node-api = "0.4.0"

# OminiX-MLX GPT-SoVITS
gpt-sovits-mlx = { path = "../../../OminiX-MLX/gpt-sovits-mlx" }

# 工具
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"

# Arrow 数据处理
arrow2 = { version = "0.18", features = ["io_ipc"] }
```

### Step 2.3：protocol.rs（协议解析，与 dora-primespeech 完全兼容）

**文件**：`node-hub/moxin-tts-node/src/protocol.rs`

```rust
//! 兼容 dora-primespeech 的三种语音协议解析

const VOICE_PREFIX: &str = "VOICE:";
const VOICE_CUSTOM_PREFIX: &str = "VOICE:CUSTOM|";
const VOICE_TRAINED_PREFIX: &str = "VOICE:TRAINED|";

/// 解析后的请求类型
pub enum TtsRequest {
    /// 普通文本（使用当前已加载的音色）
    PlainText(String),

    /// 切换到预置音色（VOICE:name|text）
    PresetVoice {
        voice_name: String,
        text: String,
    },

    /// Express Mode：零样本克隆（VOICE:CUSTOM|ref_audio|prompt_text|lang|text）
    CustomVoice {
        ref_audio_path: String,
        prompt_text: String,
        language: String,
        text: String,
    },

    /// Pro Mode：训练音色（VOICE:TRAINED|gpt|sovits|ref|prompt|lang|text）
    TrainedVoice {
        gpt_weights_path: String,
        sovits_weights_path: String,
        ref_audio_path: String,
        prompt_text: String,
        language: String,
        text: String,
    },
}

/// 从 Arrow string（JSON 包装）中解析请求
/// 输入格式：JSON `{"prompt": "..."}` 或直接文本
pub fn parse_request(raw: &str) -> anyhow::Result<TtsRequest> {
    // 尝试 JSON 解析（兼容 dora-primespeech 的输入格式）
    let text = if raw.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(raw)?;
        v["prompt"].as_str().unwrap_or(raw).to_string()
    } else {
        raw.to_string()
    };

    // VOICE:TRAINED|gpt|sovits|ref|prompt|lang|text  (6 个 | 分隔，最后一段是文本)
    if text.starts_with(VOICE_TRAINED_PREFIX) {
        let body = &text[VOICE_TRAINED_PREFIX.len()..];
        let parts: Vec<&str> = body.splitn(6, '|').collect();
        if parts.len() == 6 {
            return Ok(TtsRequest::TrainedVoice {
                gpt_weights_path:   parts[0].to_string(),
                sovits_weights_path: parts[1].to_string(),
                ref_audio_path:     parts[2].to_string(),
                prompt_text:        parts[3].to_string(),
                language:           parts[4].to_string(),
                text:               parts[5].to_string(),
            });
        }
        anyhow::bail!("Invalid TRAINED format: expected 6 fields, got {}", parts.len());
    }

    // VOICE:CUSTOM|ref_audio|prompt_text|lang|text  (4 个 | 分隔)
    if text.starts_with(VOICE_CUSTOM_PREFIX) {
        let body = &text[VOICE_CUSTOM_PREFIX.len()..];
        let parts: Vec<&str> = body.splitn(4, '|').collect();
        if parts.len() == 4 {
            return Ok(TtsRequest::CustomVoice {
                ref_audio_path: parts[0].to_string(),
                prompt_text:    parts[1].to_string(),
                language:       parts[2].to_string(),
                text:           parts[3].to_string(),
            });
        }
        anyhow::bail!("Invalid CUSTOM format: expected 4 fields, got {}", parts.len());
    }

    // VOICE:name|text
    if text.starts_with(VOICE_PREFIX) {
        let body = &text[VOICE_PREFIX.len()..];
        if let Some((name, t)) = body.split_once('|') {
            return Ok(TtsRequest::PresetVoice {
                voice_name: name.to_string(),
                text: t.to_string(),
            });
        }
    }

    // 普通文本
    Ok(TtsRequest::PlainText(text))
}
```

### Step 2.4：voice_registry.rs（音色注册表）

**文件**：`node-hub/moxin-tts-node/src/voice_registry.rs`

```rust
//! 预置音色注册表，从 voices.json 加载

use gpt_sovits_mlx::VoiceClonerConfig;
use serde::Deserialize;
use std::{collections::HashMap, path::{Path, PathBuf}};

#[derive(Debug, Deserialize)]
pub struct VoiceEntry {
    pub gpt: String,
    pub sovits: String,
    pub reference: String,
    pub prompt_text: String,
    pub language: String,
    pub speed_factor: f32,
}

pub struct VoiceRegistry {
    voices: HashMap<String, VoiceEntry>,
    voices_dir: PathBuf,
    encoders_dir: PathBuf,
    bert_tokenizer: PathBuf,
}

impl VoiceRegistry {
    /// 从 ~/.OminiX/models/gpt-sovits-mlx/voices/voices.json 加载
    pub fn load(model_base: impl AsRef<Path>) -> anyhow::Result<Self> {
        let model_base = model_base.as_ref();
        let voices_dir = model_base.join("voices");
        let json_path = voices_dir.join("voices.json");
        let content = std::fs::read_to_string(&json_path)
            .map_err(|e| anyhow::anyhow!("Cannot read voices.json at {:?}: {}", json_path, e))?;
        let voices: HashMap<String, VoiceEntry> = serde_json::from_str(&content)?;

        Ok(Self {
            voices,
            voices_dir,
            encoders_dir: model_base.join("encoders"),
            bert_tokenizer: model_base.join("bert-tokenizer"),
        })
    }

    /// 根据音色名称构建 VoiceClonerConfig
    pub fn build_config(&self, voice_name: &str) -> anyhow::Result<VoiceClonerConfig> {
        let entry = self.voices.get(voice_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown voice: {}", voice_name))?;

        Ok(VoiceClonerConfig {
            t2s_weights:   self.voices_dir.join(&entry.gpt).to_string_lossy().to_string(),
            vits_weights:  self.voices_dir.join(&entry.sovits).to_string_lossy().to_string(),
            hubert_weights: self.encoders_dir.join("hubert.safetensors").to_string_lossy().to_string(),
            bert_weights:   self.encoders_dir.join("bert.safetensors").to_string_lossy().to_string(),
            bert_tokenizer: self.bert_tokenizer.to_string_lossy().to_string(),
            sample_rate: 32000,
            speed: entry.speed_factor,
            use_gpu_mel: true,
            ..Default::default()
        })
    }

    /// 构建自定义路径的 VoiceClonerConfig（Pro Mode 训练音色）
    pub fn build_trained_config(
        &self,
        gpt_path: &str,
        sovits_path: &str,
        speed: f32,
    ) -> VoiceClonerConfig {
        VoiceClonerConfig {
            t2s_weights:    gpt_path.to_string(),
            vits_weights:   sovits_path.to_string(),
            hubert_weights: self.encoders_dir.join("hubert.safetensors").to_string_lossy().to_string(),
            bert_weights:   self.encoders_dir.join("bert.safetensors").to_string_lossy().to_string(),
            bert_tokenizer: self.bert_tokenizer.to_string_lossy().to_string(),
            sample_rate: 32000,
            speed,
            use_gpu_mel: true,
            ..Default::default()
        }
    }

    /// 获取音色的参考音频信息（用于 Express Mode 热切换）
    pub fn reference_info(&self, voice_name: &str) -> Option<(PathBuf, String, String)> {
        let entry = self.voices.get(voice_name)?;
        Some((
            self.voices_dir.join(&entry.reference),
            entry.prompt_text.clone(),
            entry.language.clone(),
        ))
    }

    pub fn voice_names(&self) -> Vec<&str> {
        self.voices.keys().map(|s| s.as_str()).collect()
    }
}
```

### Step 2.5：voice_state.rs（运行时音色状态管理）

**文件**：`node-hub/moxin-tts-node/src/voice_state.rs`

```rust
//! 运行时音色状态：管理 VoiceCloner 的加载和切换
//!
//! 关键优化：
//! - 同一对 (gpt, sovits) 模型下切换参考音频时，无需重新加载模型（快速，~50ms）
//! - 切换到不同模型时，重新构造 VoiceCloner（慢，~2-5s）

use gpt_sovits_mlx::{AudioOutput, VoiceCloner, VoiceClonerConfig};
use std::path::Path;

pub struct VoiceState {
    cloner: Option<VoiceCloner>,
    current_gpt: String,
    current_sovits: String,
}

impl VoiceState {
    pub fn new() -> Self {
        Self { cloner: None, current_gpt: String::new(), current_sovits: String::new() }
    }

    /// 确保指定的模型已加载（如果与当前相同则跳过重加载）
    pub fn ensure_model(&mut self, config: &VoiceClonerConfig) -> anyhow::Result<()> {
        if config.t2s_weights == self.current_gpt && config.vits_weights == self.current_sovits {
            tracing::debug!("Model already loaded, skipping reload");
            return Ok(());
        }
        tracing::info!("Loading model: {} + {}", config.t2s_weights, config.vits_weights);
        self.cloner = Some(VoiceCloner::new(config.clone())?);
        self.current_gpt = config.t2s_weights.clone();
        self.current_sovits = config.vits_weights.clone();
        tracing::info!("Model loaded");
        Ok(())
    }

    /// 设置参考音频（Few-shot 模式，带文本）
    pub fn set_reference_with_text(&mut self, ref_path: impl AsRef<Path>, prompt_text: &str) -> anyhow::Result<()> {
        let cloner = self.cloner.as_mut().ok_or_else(|| anyhow::anyhow!("No model loaded"))?;
        cloner.set_reference_audio_with_text(ref_path, prompt_text)?;
        Ok(())
    }

    /// 设置参考音频（Zero-shot 模式，无文本）
    pub fn set_reference(&mut self, ref_path: impl AsRef<Path>) -> anyhow::Result<()> {
        let cloner = self.cloner.as_mut().ok_or_else(|| anyhow::anyhow!("No model loaded"))?;
        cloner.set_reference_audio(ref_path)?;
        Ok(())
    }

    /// 执行 TTS 合成
    pub fn synthesize(&mut self, text: &str) -> anyhow::Result<AudioOutput> {
        let cloner = self.cloner.as_mut().ok_or_else(|| anyhow::anyhow!("No model loaded"))?;
        Ok(cloner.synthesize(text)?)
    }
}
```

### Step 2.6：main.rs（主节点逻辑）

**文件**：`node-hub/moxin-tts-node/src/main.rs`

```rust
mod protocol;
mod voice_registry;
mod voice_state;

use dora_node_api::{DoraNode, Event};
use protocol::{parse_request, TtsRequest};
use voice_registry::VoiceRegistry;
use voice_state::VoiceState;
use std::{path::PathBuf, time::SystemTime};

fn model_base() -> PathBuf {
    std::env::var("GPT_SOVITS_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".OminiX/models/gpt-sovits-mlx")
        })
}

fn send_log(node: &mut DoraNode, level: &str, message: &str) {
    let log = serde_json::json!({
        "node": "moxin-tts-node",
        "level": level,
        "message": message,
        "timestamp": SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    });
    // 忽略发送失败（节点可能正在停止）
    let _ = node.send_output(
        "log",
        Default::default(),
        serde_json::to_string(&log).unwrap_or_default().as_bytes().into(),
    );
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let (mut node, mut events) = DoraNode::init_from_env()?;

    let model_base = model_base();
    tracing::info!("Loading voice registry from {:?}", model_base);
    send_log(&mut node, "INFO", &format!("Loading voice registry from {:?}", model_base));

    let registry = VoiceRegistry::load(&model_base)?;
    let default_voice = std::env::var("VOICE_NAME").unwrap_or_else(|_| "Doubao".to_string());

    let mut state = VoiceState::new();

    // 加载默认音色
    let default_config = registry.build_config(&default_voice)?;
    state.ensure_model(&default_config)?;
    if let Some((ref_path, prompt_text, _lang)) = registry.reference_info(&default_voice) {
        state.set_reference_with_text(&ref_path, &prompt_text)?;
    }
    send_log(&mut node, "INFO", &format!("Ready. Default voice: {}", default_voice));

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, metadata, data } if id.as_str() == "text" => {
                // 提取 Arrow string 数据
                let raw = match extract_string(&data) {
                    Ok(s) => s,
                    Err(e) => {
                        send_log(&mut node, "ERROR", &format!("Failed to decode input: {}", e));
                        continue;
                    }
                };

                if raw.trim().is_empty() {
                    let _ = node.send_output("segment_complete", metadata.parameters.clone(), "skipped".as_bytes().into());
                    continue;
                }

                send_log(&mut node, "INFO", &format!("Received: {}...", &raw[..raw.len().min(60)]));

                // 解析请求类型
                let request = match parse_request(&raw) {
                    Ok(r) => r,
                    Err(e) => {
                        send_log(&mut node, "ERROR", &format!("Protocol parse error: {}", e));
                        let _ = node.send_output("segment_complete", metadata.parameters.clone(), "error".as_bytes().into());
                        continue;
                    }
                };

                // 处理各类请求
                let text = match request {
                    TtsRequest::PlainText(t) => t,

                    TtsRequest::PresetVoice { voice_name, text } => {
                        match registry.build_config(&voice_name) {
                            Ok(config) => {
                                if let Err(e) = state.ensure_model(&config) {
                                    send_log(&mut node, "ERROR", &format!("Failed to load model {}: {}", voice_name, e));
                                    let _ = node.send_output("segment_complete", metadata.parameters.clone(), "error".as_bytes().into());
                                    continue;
                                }
                                if let Some((ref_path, prompt_text, _)) = registry.reference_info(&voice_name) {
                                    let _ = state.set_reference_with_text(&ref_path, &prompt_text);
                                }
                            }
                            Err(e) => {
                                send_log(&mut node, "WARN", &format!("Unknown voice '{}': {}. Using current.", voice_name, e));
                            }
                        }
                        text
                    }

                    TtsRequest::CustomVoice { ref_audio_path, prompt_text, language: _, text } => {
                        // Express Mode: 使用当前已加载的 GPT+SoVITS（通常是 Doubao）
                        // 只切换参考音频，无需重加载模型（快速）
                        if let Err(e) = state.set_reference_with_text(&ref_audio_path, &prompt_text) {
                            send_log(&mut node, "ERROR", &format!("Failed to set reference audio: {}", e));
                            let _ = node.send_output("segment_complete", metadata.parameters.clone(), "error".as_bytes().into());
                            continue;
                        }
                        send_log(&mut node, "INFO", "Express Mode: reference audio set");
                        text
                    }

                    TtsRequest::TrainedVoice { gpt_weights_path, sovits_weights_path, ref_audio_path, prompt_text, language: _, text } => {
                        // Pro Mode: 加载用户训练的自定义模型
                        let config = registry.build_trained_config(
                            &gpt_weights_path,
                            &sovits_weights_path,
                            1.0,
                        );
                        if let Err(e) = state.ensure_model(&config) {
                            send_log(&mut node, "ERROR", &format!("Failed to load trained model: {}", e));
                            let _ = node.send_output("segment_complete", metadata.parameters.clone(), "error".as_bytes().into());
                            continue;
                        }
                        if let Err(e) = state.set_reference_with_text(&ref_audio_path, &prompt_text) {
                            send_log(&mut node, "ERROR", &format!("Failed to set reference: {}", e));
                        }
                        send_log(&mut node, "INFO", "Pro Mode: trained model loaded");
                        text
                    }
                };

                // 执行 TTS 合成
                send_log(&mut node, "INFO", &format!("Synthesizing: {}...", &text[..text.len().min(40)]));
                match state.synthesize(&text) {
                    Ok(audio) => {
                        // 发送音频（带 sample_rate 元数据，Rust 播放器依赖）
                        let mut params = metadata.parameters.clone();
                        params.insert("sample_rate".to_string(), audio.sample_rate.to_string().into());
                        let _ = node.send_output(
                            "audio",
                            params,
                            audio.samples.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>().into(),
                        );
                        let _ = node.send_output("segment_complete", metadata.parameters, "completed".as_bytes().into());
                        send_log(&mut node, "INFO", &format!("Done: {:.2}s audio", audio.duration));
                    }
                    Err(e) => {
                        send_log(&mut node, "ERROR", &format!("Synthesis failed: {}", e));
                        let _ = node.send_output("segment_complete", metadata.parameters, "error".as_bytes().into());
                    }
                }
            }

            Event::Stop(_) => {
                tracing::info!("Received stop signal");
                break;
            }

            _ => {}
        }
    }

    Ok(())
}

/// 从 Arrow 数据提取 UTF-8 字符串
fn extract_string(data: &dora_node_api::Data) -> anyhow::Result<String> {
    // Arrow2 Utf8Array 解码
    let bytes = data.as_ref();
    // 尝试直接 UTF-8 解码（Dora 通常用 IPC 格式）
    String::from_utf8(bytes.to_vec())
        .map_err(|e| anyhow::anyhow!("UTF-8 decode failed: {}", e))
}
```

> ⚠️ **注意**：`extract_string` 的实现需要根据 dora-node-api 0.4.0 的实际 Arrow 数据格式调整。实际使用时参考 OminiX-MLX 中已有的 Dora 节点代码（如 `gpt-sovits-mlx/src/dora_node.rs`）确认正确的 Arrow 解码方式。

### Step 2.7：将节点加入 Workspace

**修改** `/Users/alan0x/Documents/projects/Moxin-Voice/Cargo.toml`，在 `members` 中添加：

```toml
[workspace]
members = [
    "moxin-voice-shell",
    "moxin-widgets",
    "moxin-dora-bridge",
    "moxin-ui",
    "apps/moxin-voice",
    "node-hub/moxin-tts-node",    # ← 新增
]
```

### Step 2.8：编译验证

```bash
cd /Users/alan0x/Documents/projects/Moxin-Voice

# 仅编译 TTS 节点（快速验证）
cargo build -p moxin-tts-node --release

# 查看二进制
ls -la target/release/moxin-tts-node
```

---

## Phase 3：更新 tts.yml

**文件**：`apps/moxin-voice/dataflow/tts.yml`

将 `primespeech-tts` 节点从 Python 改为 Rust 二进制：

```yaml
nodes:
  # 音频输入（保持不变）
  - id: moxin-audio-input
    path: dynamic
    outputs:
      - audio

  # ASR（保持不变，继续使用 Python dora-asr）
  - id: asr
    build: pip install -e ../../../node-hub/dora-asr
    path: dora-asr
    inputs:
      audio: moxin-audio-input/audio
    outputs:
      - transcription
      - status
      - log
    env:
      USE_GPU: "false"
      ASR_ENGINE: "funasr"
      LANGUAGE: "zh"
      LOG_LEVEL: "INFO"

  # ASR 监听器（保持不变）
  - id: moxin-asr-listener
    path: dynamic
    inputs:
      transcription: asr/transcription

  # 文本输入（保持不变）
  - id: moxin-prompt-input
    path: dynamic
    outputs:
      - control

  # TTS 节点：从 Python dora-primespeech 替换为 Rust moxin-tts-node
  - id: primespeech-tts # ← id 保持不变！
    path: ../../../target/release/moxin-tts-node # ← Rust 二进制路径
    inputs:
      text: moxin-prompt-input/control
    outputs:
      - audio
      - segment_complete
      - log
    env:
      GPT_SOVITS_MODEL_DIR: "$HOME/.OminiX/models/gpt-sovits-mlx"
      VOICE_NAME: "Doubao"
      RUST_LOG: "info"

  # 音频播放器（保持不变）
  - id: moxin-audio-player
    path: dynamic
    inputs:
      audio: primespeech-tts/audio
    outputs:
      - buffer_status
```

---

## Phase 4：功能验证清单

按顺序逐一验证，每项通过后才进行下一项。

### 4.1 预置音色 TTS

```
□ 启动 dora up
□ dora start apps/moxin-voice/dataflow/tts.yml
□ 发送普通文本 → 确认有音频输出
□ 发送 VOICE:Doubao|你好，世界 → 确认切换到 Doubao 音色
□ 发送 VOICE:LuoXiang|你好，世界 → 确认切换到罗翔（验证模型重载）
□ 发送 VOICE:Trump|Hello world → 确认英文音色
□ 连续发送多条文本 → 确认无卡顿/崩溃
```

### 4.2 Express Mode（零样本克隆）

```
□ 录制 5-10 秒参考音频，保存为 /tmp/test_ref.wav
□ 发送协议：VOICE:CUSTOM|/tmp/test_ref.wav|参考文本|zh|要合成的内容
□ 确认输出音色与参考音频相似
□ 再次发送普通文本 → 确认仍使用克隆音色（无崩溃）
□ 发送 VOICE:Doubao|测试 → 确认可以从克隆模式切回预置音色
```

### 4.3 Pro Mode（训练音色）

```
□ 准备一个已训练的模型（PyTorch 格式需先转换为 MLX）
□ 发送协议：VOICE:TRAINED|/path/gpt.safetensors|/path/sovits.safetensors|/path/ref.wav|提示文本|zh|合成内容
□ 确认使用自定义模型合成
□ 发送 VOICE:Doubao|测试 → 确认可以切回预置音色
```

### 4.4 ASR 功能

```
□ 点击录音 → 说话 → 停止录音
□ 确认 asr 节点收到音频
□ 确认 moxin-asr-listener 收到转录文本
□ UI 中的文本框显示识别结果
（ASR 节点未改动，此步骤主要验证数据流连通性）
```

### 4.5 日志和错误处理

```
□ 发送空文本 → 确认返回 segment_complete: "skipped"（无崩溃）
□ 发送不存在的音色名 VOICE:Unknown|text → 确认日志警告，使用当前音色继续合成
□ 发送格式错误的 VOICE:CUSTOM 协议 → 确认返回 segment_complete: "error"（无崩溃）
□ 检查 log 输出通道有正常的结构化 JSON
```

---

## Phase 5：收尾

### Step 5.1：性能对比记录

```bash
# 记录迁移前后的性能数据
# 在合成日志中查找 "Done: Xs audio in Ys" 字样
# 与 dora-primespeech 的 RTF 对比（目标：RTF < 0.5x）
```

### Step 5.2：更新 MEMORY.md

迁移完成后更新记忆文件中的关键信息：

- 构建命令（不再需要 `--features moyoyo-ui`）
- 新的模型路径（`~/.OminiX/models/gpt-sovits-mlx/`）
- TTS 节点位置（`node-hub/moxin-tts-node/`）

### Step 5.3：保留旧节点（备用回滚）

```bash
# 不删除 dora-primespeech，仅在 tts.yml 中注释掉
# 如需回滚，将 tts.yml 的 primespeech-tts 节点改回：
#   path: dora-primespeech
#   build: pip install -e ../../../node-hub/dora-primespeech
```

---

## 回滚方案

如果任何阶段出现无法解决的问题，快速回滚步骤：

```bash
# 1. 编辑 tts.yml，将 primespeech-tts 节点还原为 Python 版
#    path: dora-primespeech
#    build: pip install -e ../../../node-hub/dora-primespeech
#    （删除 GPT_SOVITS_MODEL_DIR 等新增 env 变量，还原原有 env）

# 2. 重启 dora
dora stop <dataflow-id>
dora start apps/moxin-voice/dataflow/tts.yml

# 3. 验证 Python 版本工作正常
```

回滚代价：< 5 分钟。

---

## 已知风险与应对

| 风险                                       | 概率 | 应对                                                        |
| ------------------------------------------ | ---- | ----------------------------------------------------------- |
| 转换脚本路径参数与实际文件名不符           | 中   | Step 1.3 执行前先 `ls` 确认所有源文件存在                   |
| dora-node-api Arrow 数据格式与代码假设不符 | 中   | 参考 OminiX-MLX 中已有的 Dora 节点代码修正 `extract_string` |
| Pro Mode 训练的 PyTorch 模型无法转换       | 低   | 同样用 convert_gpt_weights.py，用户训练后需手动转换         |
| 部分音色转换后推理质量下降                 | 低   | 对比转换前后音频，若有差异检查转换脚本参数                  |
| Express Mode 切换音色后模型状态混乱        | 低   | voice_state.rs 的 ensure_model 做了幂等保证                 |
