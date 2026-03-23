# Qwen3-TTS CustomVoice 微调（仅 MLX 路线）

本目录用于 moxin-tts 的 **MLX 侧 speaker embedding 微调**。

目标：
- 针对 `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` 微调一个说话人行向量
- 输出 speaker embedding checkpoint（`spk_emb_*.npz`）
- （可选，高风险）回写 `model.safetensors`

## 流程

1. 准备 wav + 文本配对
- 音频目录：`<speaker>/raw/*.wav`
- 文本目录：`<speaker>/text/*.txt`（与音频同名）

2. 音频编码为 codec 帧

```bash
python encode_audio.py \
  --in_dir /path/to/<speaker>/raw \
  --out_dir /path/to/<speaker>/encoded \
  --tokenizer_path ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit/speech_tokenizer
```

3. 执行 MLX 训练

```bash
python train.py \
  --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
  --encoded_dir /path/to/<speaker>/encoded \
  --text_dir /path/to/<speaker>/text \
  --speaker_name <speaker_name> \
  --speaker_id <new_id> \
  --language chinese \
  --epochs 20 \
  --batch_size 4
```

说明：
- `--speaker_id` 现在是必传（手动模式）。
- 如果 `speaker_id` 已被已有音色占用，训练默认直接报错。
- 默认是**安全模式**：只输出 `spk_emb_*.npz`，不改模型权重。
- tokenizer 现在是“失败即报错”，不再走字符级兜底。

如需强制回写模型（不建议在 8bit 模型上这样做）：

```bash
python train.py ... --patch_model --allow_patch_quantized
```

4. 注册 speaker 到 config（仅在你确实回写了模型时需要）

```bash
python register_speaker.py \
  --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
  --speaker_name <speaker_name> \
  --speaker_id <resolved_id> \
  --language chinese
```

5. 重启后端并在 UI / node 测试

## 辅助脚本

- `apply_checkpoint.py`：把 `spk_emb_*.npz` 应用到模型权重
- `build_train_jsonl.py`、`official_prepare_data.py`、`official_dataset.py`：
  仅保留为辅助/参考工具，不是当前主训练路径
