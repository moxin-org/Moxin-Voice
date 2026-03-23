# moxin-tts 的 Qwen3-TTS 微调说明（官方方案优先）

本目录现在有两套方案：

- **推荐：官方微调方案**（对齐 Qwen 官方 `finetuning/`）
- **兼容保留：旧版 MLX 单行 embedding 方案**（`train.py`）

如果你要稳定拿到可用自定义音色，请优先用官方方案。

## 官方方案（推荐）

本地新增脚本：
- `build_train_jsonl.py`：从 wav/txt 生成 `train_raw.jsonl`
- `official_prepare_data.py`：提取 `audio_codes`
- `official_dataset.py`：官方数据集构造
- `official_sft_12hz.py`：官方 SFT 训练（加了可配置 `speaker_id` 和安全写行逻辑）

### 0）环境

官方 SFT 依赖 **PyTorch + CUDA**，不是 MLX 训练。

```bash
conda create -n qwen3-tts-ft python=3.12 -y
conda activate qwen3-tts-ft
pip install -U qwen-tts accelerate transformers safetensors soundfile librosa
pip install -U flash-attn --no-build-isolation   # 可选但建议
```

如果你不是 CUDA 环境，也可以尝试：
- `--device_map mps`（Apple Silicon）或 `--device_map cpu`
- `--dtype float16` 或 `--dtype float32`
- `--attn_implementation eager`

### 1）生成 `train_raw.jsonl`

```bash
python build_train_jsonl.py \
  --audio_dir /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/raw \
  --text_dir /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/text \
  --ref_audio /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/raw/001.wav \
  --output_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_raw.jsonl \
  --language Chinese
```

说明：
- `ref_audio` 建议全量样本统一使用同一条（官方建议）。
- 音频建议 24kHz 单声道 WAV。

### 2）提取 `audio_codes`

```bash
python official_prepare_data.py \
  --device cuda:0 \
  --tokenizer_model_path Qwen/Qwen3-TTS-Tokenizer-12Hz \
  --input_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_raw.jsonl \
  --output_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_with_codes.jsonl
```

### 3）执行官方风格 SFT

```bash
python official_sft_12hz.py \
  --init_model_path Qwen/Qwen3-TTS-12Hz-1.7B-Base \
  --output_model_path /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/output_official \
  --train_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_with_codes.jsonl \
  --device_map cuda:0 \
  --dtype bfloat16 \
  --attn_implementation flash_attention_2 \
  --batch_size 2 \
  --lr 2e-6 \
  --num_epochs 10 \
  --speaker_name yangyang_new \
  --speaker_id 3068
```

输出目录类似：
- `output_official/checkpoint-epoch-0`
- `output_official/checkpoint-epoch-1`
- ...

每个 checkpoint 内含：
- 已改写 `config.json`（`tts_model_type=custom_voice`）
- 已写入 `speaker_name/speaker_id`
- 新的 `model.safetensors`

### 4）接入 moxin-tts 节点

把节点的模型路径指向你训练出的 checkpoint：

```bash
export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR=/Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/output_official/checkpoint-epoch-9
```

然后按现有方式启动后端即可。

## 旧版方案（保留兼容）

以下脚本仍保留，但定位为快速实验：
- `encode_audio.py`
- `train.py`
- `apply_checkpoint.py`
- `register_speaker.py`

## 常见问题

- `PyTorch not found` / `torch>=2.4`：官方方案需要独立 CUDA 环境。
- 首轮 `loss=nan`：先降学习率，确认硬件支持 bf16，检查数据采样率与文本对齐。
- 合成全是噪音：优先回退到更早 checkpoint 对比，并确认 `QWEN3_TTS_CUSTOMVOICE_MODEL_DIR` 指向的是正确目录。
