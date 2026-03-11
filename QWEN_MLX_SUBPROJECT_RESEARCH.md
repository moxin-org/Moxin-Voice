# OminiX-MLX 子项目调研报告（Qwen 系列）

- 调研时间：2026-03-11
- 调研范围：
  - `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3-asr-mlx`
  - `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3-mlx`
  - `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3-tts-mlx`
  - `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3.5-35B-mlx`
- 调研方法：阅读各子项目 `README.md`、`Cargo.toml`、`src/lib.rs`、`examples/*`、源码关键字扫描（train/training/loss/optimizer/finetune 等）

---

## 1. 这几个子项分别是什么

## 1.1 `qwen3-asr-mlx`
- 类型：**ASR（语音识别）推理库**
- 任务：音频 -> 文本（支持多语言）
- 技术形态：Rust + MLX，支持 0.6B/1.7B，配置驱动加载
- 公开接口（从 `src/lib.rs` 和 README）：
  - `Qwen3ASR::load(...)`
  - `transcribe(...)` / `transcribe_with_language(...)` / `transcribe_samples(...)`
- 结论：定位是**纯推理 ASR 引擎**，非训练框架

## 1.2 `qwen3-mlx`
- 类型：**Qwen3 通用 LLM 推理库**
- 任务：文本生成 / 聊天（token 生成）
- 技术形态：Rust + MLX，含 KV cache 与生成迭代器
- 公开接口（`src/lib.rs`）：
  - `load_model`、`load_tokenizer`
  - `Generate` 迭代生成
- 结论：定位是**文本 LLM 推理**，不是语音模型

## 1.3 `qwen3-tts-mlx`
- 类型：**TTS 推理库**
- 任务：文本 -> 音频（含预置音色、voice clone 推理、streaming）
- 技术形态：Rust + MLX，`Synthesizer` 高层 API
- 公开接口（`src/lib.rs` / README）：
  - `synthesize(...)`
  - `synthesize_voice_clone(...)`
  - `synthesize_voice_clone_icl(...)`（README 标注实验性）
  - `start_streaming(...)`
- 结论：定位是**TTS 推理引擎**，支持“参考音频条件推理”，但不是 few-shot 训练框架

## 1.4 `qwen3.5-35B-mlx`（crate 名 `qwen3-5-35b-mlx`）
- 类型：**Qwen3.5-27B 通用 LLM 推理库**
- 任务：文本生成（Hybrid DeltaNet + Attention）
- 技术形态：Rust + MLX，聚焦超大模型推理性能
- 公开接口（`src/lib.rs`）：
  - `load_model`、`load_tokenizer`
  - `Generate`
- 结论：定位是**大语言模型推理**，不直接面向语音链路

---

## 2. 关系与区别

## 2.1 共同点
- 全部是 Rust + MLX 路线（Apple Silicon 推理优先）
- 全部是“模型推理”导向的 crate 形态，提供 library API + examples
- 都依赖同一生态基座：`mlx-rs` / `mlx-rs-core`

## 2.2 关键区别（按任务域）

| 子项目 | 任务域 | 输入 | 输出 | 与你当前客户端链路的对应位置 |
|---|---|---|---|---|
| qwen3-asr-mlx | ASR | 音频 | 文本 | 可替代/并行 ASR 节点 |
| qwen3-tts-mlx | TTS | 文本（可带参考音频） | 音频 | 可替代 TTS 推理节点 |
| qwen3-mlx | 通用 LLM | 文本 | 文本 | 不对应主语音链路核心节点 |
| qwen3.5-35B-mlx | 通用 LLM | 文本 | 文本 | 不对应主语音链路核心节点 |

## 2.3 “是否包含训练能力”差异
- 结论：这四个子项目都没有看到完整可用的训练流水（数据集构建、loss 计算、优化器迭代、checkpoint 管理、导出）
- 说明：
  - 代码中虽可见 `training_mode(...)` 之类模块接口痕迹，但这是神经网络模块常见接口，不等同于“可执行训练系统”
  - 没有发现可运行的训练入口（examples/cli）和训练文档链路

---

## 3. 按“通用迁移流程”评估：哪些适合做新的推理 / 训练节点

你的通用流程核心是：
1) 能力矩阵确认
2) 接口契约固定
3) 适配节点化
4) 模型资产与初始化
5) 训练链路独立评估

基于这个框架，评估如下。

## 3.1 适合做“新的推理节点”的候选

## A. `qwen3-tts-mlx`：**适合做新的 TTS 推理节点（高优先级）**
- 原因：
  - 任务域匹配（TTS）
  - 有较完整推理 API（合成、流式、部分 voice clone 推理）
  - Rust/MLX 方向与当前迁移目标一致
- 迁移建议：
  - 新建 `qwen3-tts-node`，对齐你当前 node contract（text/control/audio/status/log）
  - 保持 UI/业务层协议不变，只换推理后端

## B. `qwen3-asr-mlx`：**适合做新的 ASR 推理节点（中高优先级）**
- 原因：
  - 任务域匹配（ASR）
  - 接口清晰（文件/样本识别，多语言）
- 迁移建议：
  - 新建 `qwen3-asr-node`，替代或并行 `dora-asr`
  - 与当前 ASR 回填逻辑（zero/few-shot 文本回填）做适配

## 3.2 不适合做语音主链路推理节点的项

## C. `qwen3-mlx`：**不适合直接做 TTS/ASR 节点**
- 原因：是通用文本 LLM，不是语音模型
- 可选用途：提示重写、文本预处理、对话辅助（非核心语音推理）

## D. `qwen3.5-35B-mlx`：**不适合直接做 TTS/ASR 节点**
- 原因：同上，且模型体积更大、资源成本更高
- 可选用途：高级文本理解/规划类能力（如果未来有需要）

## 3.3 适合做“新的训练节点”的候选

**结论：当前这四个子项目都不适合直接承担你要的 few-shot 训练节点。**

- 原因（共性）：
  - 未发现端到端训练流水入口
  - 未发现训练数据管线与训练任务编排
  - 未发现训练导出与部署闭环
- 对你当前项目的含义：
  - 可做“推理替换”
  - 不应直接替换现有 few-shot 训练链路

---

## 4. 对你项目的落地建议（按优先级）

1. 先落地 `qwen3-tts-mlx` 推理节点 PoC（文本->音频，先不碰训练）
2. 再做 `qwen3-asr-mlx` 节点 PoC（替换/并行当前 ASR）
3. 保持训练后端独立开关（沿用你已有 Option A/B 思路）
4. 明确将“Qwen 迁移”拆成两阶段：
   - 阶段1：推理后端替换（可交付）
   - 阶段2：训练体系重建（若上游补齐或自研）

---

## 5. 最终结论（TL;DR）

- **最适合做新推理节点**：`qwen3-tts-mlx`（TTS）、`qwen3-asr-mlx`（ASR）
- **不适合做语音推理节点**：`qwen3-mlx`、`qwen3.5-35B-mlx`（它们是文本 LLM）
- **这四个都不适合直接做 few-shot 训练节点**：当前看不到完整训练链路实现

