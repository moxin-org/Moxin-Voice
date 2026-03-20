# 内置音色方案研究：Fine-tuning vs ICL

**日期**：2026-03-21  
**作者**：Moxin Voice 团队  
**状态**：已结案，ICL 方案落地

---

## 背景

Moxin Voice 使用 Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit 作为内置音色推理引擎。该模型出厂内置 9 个说话人（vivian、serena、uncle_fu 等），通过 `talker.model.codec_embedding.weight` 中的 speaker embedding 行来索引。

需求：将用户自定义录音（约 22 秒）打包为内置音色随 app 分发，效果应与官方内置音色相当。

---

## 方案一：Speaker Embedding Fine-tuning（训练新行）

### 原理

Qwen3-TTS 的说话人身份完全由 `codec_embedding.weight`（形状 `[3072, 2048]`, bfloat16）中的一行决定。28 层 transformer 完全冻结（8-bit 量化），只需训练新增的第 3067 行。

**Token 序列结构（训练目标）**：
- Pos 0–2：role tokens（text-only）
- Pos 3–7：tts_pad + codec 控制 token
- Pos 8：tts_bos + codec_pad
- Pos 9：first_text + codec_bos（起始，计入 loss）
- Pos 10+：codec frame（每帧 16 个 codebook token）

损失仅在 Pos 9 至结尾的 codec token 上计算。梯度只通过新的 speaker embedding 行回传。

### 实施过程

1. **编码参考音频**（`encode_audio.py`）：将 22s WAV 通过纯 MLX 实现的 SEANet encoder 转为 codec frames，输出 `(275, 16)` int16 张量。
2. **训练**（`train.py`）：22s 音频切为 7 个 3 秒片段，MLX 上跑梯度下降，AdamW，lr=1e-3，batch_size=4，20 epochs。
3. **注入模型**（`apply_checkpoint.py`）：用 `mx.load` + `mx.save_safetensors` 将新行追加写入 `model.safetensors`。
4. **注册**（`register_speaker.py`）：更新 `config.json` 中的 `spk_id` 映射。

### 遇到的问题

| 问题 | 根本原因 | 解决方式 |
|------|----------|----------|
| `safetensors` numpy backend 无法加载 bfloat16 | safetensors Python 包 numpy 后端不支持 bfloat16 dtype | 改用 `mx.load` + `mx.save_safetensors`（MLX 原生支持）|
| `mx.fast.gelu` 不存在 | MLX 0.30 无此 API | 手动实现 GELU 近似 |
| transformers 5.3.0 禁用 PyTorch | torch 版本 < 要求的 2.4 | 完全重写 encode_audio.py 为纯 MLX，移除 transformers 依赖 |
| 梯度除数错误 | 单文件训练时 `avg_grad = grad / batch_size` 等效 lr/4 | 改为 `actual_count = sample_idx % batch_size + 1` |

### 训练结果评估

训练流程本身跑通，`spk_emb_final.npz` 保存成功。但实际推理时暴露出两个严重问题：

**问题 1：生成时长异常**  
- 输入 25 个 text token，模型生成了 **3534 帧（282 秒）**，而正常应为 30–60 帧（3–5 秒）。  
- 根本原因：embedding 未收敛到模型能正确理解的 speaker space，导致模型无法识别 EOS 位置，持续生成直至接近 max_frames 限制。

**问题 2：解码 OOM crash**  
- 3534 帧经 SEANet decoder 上采样（1920×）后，中间张量峰值约 13 GB，超过 Metal buffer 上限 8.88 GB。  
- 已通过分块解码（每块 200 帧）修复 crash，但这只是绕开症状，根因是 embedding 质量差。

### 根本局限

22 秒音频切成 7 个 3 秒片段，信息量远不足以让冻结的 28 层 transformer 正确理解新 speaker 的特征。官方内置音色（vivian、serena 等）由 Qwen 团队用大量数据训练，speaker embedding 落在模型熟悉的"说话人流形"上。从零用 7 个片段训练一行新 embedding，极难收敛到正确位置。

**结论：数据量不足时，fine-tuning 方案不可行。**

---

## 方案二：ICL（In-Context Learning）语音克隆

### 原理

Qwen3-TTS Base 模型支持 ICL 语音克隆（即零样本克隆）：将参考音频的 codec frames 作为上下文前缀注入推理序列，模型自然模仿参考说话人的音色生成目标文本。

**Prefill 布局（ICL 模式）**：
```
[role tokens] → [codec control] → [ICL extension block: ref codec frames + ref text] → [target text] → [generation]
```

推理路径：`Base 模型 + synthesize_voice_clone_icl(text, ref_audio, ref_text, lang)`

### 优势

- **无需训练**：直接用参考音频做推理，22 秒完全够用
- **音色质量高**：实测与手动通过 Express Mode 克隆的效果一致，已验证满足需求
- **EOS 行为正常**：Base 模型在 ICL 模式下正确理解生成长度，不会出现无限生成

### 实施方案

将参考音频和文字稿随 app bundle 打包，定义新的 `VoiceSource::BundledIcl` 类型：

```
node-hub/dora-qwen3-tts-mlx/voices/
  baiyang/
    ref.wav        # 参考音频（22s）
    ref_text.txt   # 音频对应文字稿
  yangyang/
    ref.wav
    ref_text.txt
```

打包时（`build_macos_app.sh`）将 `voices/` 目录复制到 `$APP_BUNDLE/Contents/Resources/qwen3-voices/`。

推理时，选中 BundledIcl 音色后，`screen.rs` 通过 `resolve_bundled_icl_ref_path()` 定位参考音频（bundle → dev repo 两级查找），发送 `VOICE:CUSTOM|<ref_path>|<ref_text>|<lang>|<text>` 到 qwen-tts-node，走与 Express Mode 完全相同的 ICL 路径。

### UI 行为

- BundledIcl 音色在 UI 中显示为"内置"（与 Builtin 相同标签）
- 不显示删除按钮
- 点击预览直接播放 `ref.wav`
- 不出现在"自定义"过滤 tab 中

---

## 方案对比

| 维度 | Fine-tuning | ICL（BundledIcl）|
|------|------------|-----------------|
| 数据需求 | 数分钟以上 | 10–30 秒即可 |
| 音色质量（22s 数据）| 差（EOS 失控） | 好（与 Express Mode 一致）|
| 推理速度 | 与内置音色相同（直接 embedding lookup）| 略慢（需处理 ICL 前缀帧）|
| 模型修改 | 需要改写 model.safetensors | 不改动模型 |
| 分发复杂度 | 需分发修改后的模型文件（~GB 级）| 只需分发参考音频（几百 KB）|
| 可维护性 | 每个音色需要重新训练 | 直接替换 ref.wav 即可更新音色 |

---

## 最终决策

**选择 ICL（BundledIcl）方案**，理由：

1. 当前数据量（22 秒）下，fine-tuning 无法产生可用质量
2. ICL 效果已通过 Express Mode 验证满足需求
3. 参考音频只有几百 KB，分发成本远低于修改 model.safetensors
4. 未来新增音色只需提供 `ref.wav` + `ref_text.txt`，无需重新训练

如未来有充足数据（数分钟高质量录音），可重新评估 fine-tuning 方案，两者并不互斥。

---

## 已落地的内置 ICL 音色

| 音色 ID | 显示名 | 性别 | 语言 |
|---------|--------|------|------|
| baiyang | 白杨 (Baiyang) | 女 | 中文 |
| yangyang | 杨阳 (Yangyang) | 男 | 中文 |

