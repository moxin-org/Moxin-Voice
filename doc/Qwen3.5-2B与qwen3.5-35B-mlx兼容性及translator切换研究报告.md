# Qwen3.5-2B 与 `qwen3.5-35B-mlx` 兼容性及 translator 切换研究报告

## 1. 研究目标

本报告聚焦两个问题：

1. `mlx-community/Qwen3.5-2B-MLX-4bit` 是否与本地 `OminiX-MLX/qwen3.5-35B-mlx` crate 兼容。
2. 当前 `node-hub/dora-qwen3-translator` 若要切换到该 crate，需要做哪些代码改动。

本次研究基于源码和模型元数据分析完成，当前环境是 Windows，无法实际在 Apple Silicon 上运行模型，因此报告会明确区分：

- 已有源码和模型文件支持的高置信结论
- 仍需在 Mac mini 上做运行验证的边界点

## 2. 研究材料

### 本地源码

- `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx\src\config.rs`
- `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx\src\cache.rs`
- `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx\src\model.rs`
- `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx\src\lib.rs`
- `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx\examples\generate.rs`
- `C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen3-translator\Cargo.toml`
- `C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen3-translator\src\main.rs`
- `C:\Users\FPG_123\Documents\projects\Moxin-Voice\Cargo.toml`

### 外部模型文件

- [`mlx-community/Qwen3.5-2B-MLX-4bit/config.json`](https://huggingface.co/mlx-community/Qwen3.5-2B-MLX-4bit/raw/main/config.json)
- [`mlx-community/Qwen3.5-2B-MLX-4bit/model.safetensors.index.json`](https://huggingface.co/mlx-community/Qwen3.5-2B-MLX-4bit/raw/main/model.safetensors.index.json)

## 3. 结论摘要

### 3.1 兼容性结论

`mlx-community/Qwen3.5-2B-MLX-4bit` 与 `qwen3.5-35B-mlx` 的文本 loader，基于现有证据判断为“高概率兼容”。

原因不是“它也是 MLX 模型”，而是更具体的三点：

1. 2B 模型的 `text_config` 结构与 `qwen3.5-35B-mlx` 期望的 Qwen3.5 hybrid 架构字段一致。
2. 2B 模型的文本权重键名与该 crate 在 `load_model()` 中硬编码查找的键名一致。
3. 2B 模型虽然是 `Qwen3_5ForConditionalGeneration`，包含 `vision_config` 和 `vision_tower.*` 权重，但该 crate 已实现 `language_model.model.*` 与 `language_model.lm_head.*` 前缀自动识别，因此额外视觉权重本身不会阻止其加载文本骨干。

### 3.2 不能在本机最终确认的点

当前不能在 Windows 上跑模型，因此还无法最终确认以下运行时问题：

- `load_model()` 在 Apple Silicon 上是否一次性成功完成加载
- 文本生成时是否存在某个未覆盖的特殊张量形状问题
- 2B 模型的 `tokenizer_config.json` / chat template 在现有 translator 提示词路径下是否 100% 正常

这意味着本次结论是“源码级高置信兼容”，不是“已经在 Mac 上跑通的最终兼容”。

### 3.3 对 translator 改造的结论

如果要切换到 `qwen3.5-35B-mlx`：

- 不是只改模型目录。
- 需要替换 Rust 依赖、模型类型、生成器调用方式和 EOS 处理逻辑。
- 当前 tokenizer / chat template 处理逻辑大概率可以保留。

## 4. 兼容性证据

### 4.1 `qwen3.5-35B-mlx` 实际要求的模型结构

`qwen3.5-35B-mlx` 不是“固定只支持 35B 参数量”的 crate，它本质上是一个 Qwen3.5 hybrid 架构 loader。

证据：

- `TextConfig` 明确要求 `layer_types`、`linear_num_key_heads`、`linear_num_value_heads`、`linear_key_head_dim`、`linear_value_head_dim`、`linear_conv_kernel_dim`、`rope_parameters`、`attn_output_gate`。见 `qwen3.5-35B-mlx/src/config.rs:35-66`。
- `load_model()` 会校验 `layer_types.len() == num_hidden_layers`，然后逐层根据 `layer_type` 决定加载 `full_attention` 还是 `linear_attn`。见 `qwen3.5-35B-mlx/src/model.rs:266-315`。
- `Model::forward()` 会按层类型初始化两类缓存：
  - `HybridCache::KV(KVCache)` 用于 full attention
  - `HybridCache::Recurrent(RecurrentState)` 用于 DeltaNet/linear attention
  见 `qwen3.5-35B-mlx/src/model.rs:95-129` 和 `qwen3.5-35B-mlx/src/cache.rs:34-64`。

这说明 crate 的兼容条件是“架构匹配”，而不是“参数量刚好等于 35B”。

### 4.2 2B 模型的 `config.json` 与 crate 期望匹配

从 Hugging Face 原始 `config.json` 可以确认，`mlx-community/Qwen3.5-2B-MLX-4bit` 的文本骨干就是 Qwen3.5 hybrid 架构：

```json
"architectures": ["Qwen3_5ForConditionalGeneration"],
"model_type": "qwen3_5",
"text_config": {
  "attn_output_gate": true,
  "head_dim": 256,
  "layer_types": [
    "linear_attention",
    "linear_attention",
    "linear_attention",
    "full_attention",
    ...
  ],
  "linear_conv_kernel_dim": 4,
  "linear_key_head_dim": 128,
  "linear_num_key_heads": 16,
  "linear_num_value_heads": 16,
  "linear_value_head_dim": 128,
  "num_hidden_layers": 24,
  "rope_parameters": {
    "partial_rotary_factor": 0.25
  }
}
```

这与本地 crate 的字段要求逐项对应：

| crate 期望字段 | 2B 模型实际字段 | 结论 |
| --- | --- | --- |
| `layer_types` | 存在，长度 24 | 匹配 |
| `num_hidden_layers` | `24` | 匹配 |
| `linear_num_key_heads` | `16` | 匹配 |
| `linear_num_value_heads` | `16` | 匹配 |
| `linear_key_head_dim` | `128` | 匹配 |
| `linear_value_head_dim` | `128` | 匹配 |
| `linear_conv_kernel_dim` | `4` | 匹配 |
| `rope_parameters.partial_rotary_factor` | `0.25` | 匹配 |
| `attn_output_gate` | `true` | 匹配 |

需要注意的一点是，模型文件里的 layer 类型值是 `"linear_attention"`，而本地 crate 对非 `"full_attention"` 的分支统一走 `load_gated_deltanet()`，见 `qwen3.5-35B-mlx/src/model.rs:297-315`。因此 `"linear_attention"` 对它来说是可接受的。

### 4.3 2B 模型的权重键名与 crate 查找逻辑匹配

`qwen3.5-35B-mlx/src/model.rs` 中，loader 会按以下模式拼接和查找权重键：

- 文本前缀自动检测：`language_model.model` 或 `model`。见 `qwen3.5-35B-mlx/src/model.rs:249-255`。
- LM head 前缀自动检测：`language_model.lm_head` 或 `lm_head`。见 `qwen3.5-35B-mlx/src/model.rs:258-263`。
- 每层 full attention 需要 `self_attn.q_proj/k_proj/v_proj/o_proj/q_norm.weight/k_norm.weight`
- 每层 linear attention 需要 `linear_attn.in_proj_qkv/in_proj_z/in_proj_a/in_proj_b/conv1d.weight/A_log/dt_bias/norm.weight/out_proj`
- 还需要 `embed_tokens`、`input_layernorm`、`post_attention_layernorm`、`mlp.*`、`norm.weight`、`lm_head`

2B 模型的 `model.safetensors.index.json` 中，实际存在以下键名：

```json
"language_model.model.embed_tokens.weight"
"language_model.model.layers.0.linear_attn.A_log"
"language_model.model.layers.0.linear_attn.conv1d.weight"
"language_model.model.layers.0.linear_attn.dt_bias"
"language_model.model.layers.0.linear_attn.in_proj_a.weight"
"language_model.model.layers.0.linear_attn.in_proj_b.weight"
"language_model.model.layers.0.linear_attn.in_proj_qkv.weight"
"language_model.model.layers.0.linear_attn.in_proj_z.weight"
"language_model.model.layers.0.linear_attn.norm.weight"
"language_model.model.layers.0.linear_attn.out_proj.weight"
"language_model.model.layers.11.self_attn.q_proj.weight"
"language_model.model.layers.11.self_attn.k_proj.weight"
"language_model.model.layers.11.self_attn.v_proj.weight"
"language_model.model.layers.11.self_attn.o_proj.weight"
"language_model.model.layers.11.self_attn.q_norm.weight"
"language_model.model.layers.11.self_attn.k_norm.weight"
```

这与 crate 的查找规则是对齐的。

### 4.4 多模态前缀不是阻塞点

`Qwen3.5-2B-MLX-4bit` 的 `config.json` 是 `Qwen3_5ForConditionalGeneration`，同时存在 `vision_config`。`model.safetensors.index.json` 里也存在大量 `vision_tower.*` 键。

这本来是一个风险点，但本地 crate 对这个问题已经做了适配：

- `detect_prefix()` 会检测 `language_model.` 前缀并切换到 `language_model.model`。见 `qwen3.5-35B-mlx/src/model.rs:249-255`。
- `detect_lm_head_prefix()` 会检测 `language_model.lm_head.weight`。见 `qwen3.5-35B-mlx/src/model.rs:258-263`。

因此，对文本推理来说，这个模型“额外带了视觉塔”不是本次兼容性的主要障碍。

### 4.5 兼容性结论的边界

当前仍然保留一个必要的谨慎点：

- 代码和元数据层面，高概率兼容。
- 运行时层面，仍然需要在 Mac mini 上实际执行一次最小化 `load_model + generate` 验证。

建议把兼容性定性为：

> 证据充分支持“该 2B 模型可以作为 `qwen3.5-35B-mlx` 的目标模型尝试接入”，但最终结论仍需一次真实加载验证收口。

## 5. 当前 translator 的实现现状

当前 `dora-qwen3-translator` 是围绕 `qwen3-mlx` 写的，关键耦合点如下：

- 依赖 `qwen3-mlx`，见 `node-hub/dora-qwen3-translator/Cargo.toml:8-13`。
- 代码直接导入 `qwen3_mlx::{load_model, Generate, KVCache}`，见 `node-hub/dora-qwen3-translator/src/main.rs:39-41`。
- `translate_and_emit()` 的模型类型写死为 `qwen3_mlx::Model`，见 `node-hub/dora-qwen3-translator/src/main.rs:308-320`。
- 当前生成器调用方式是外部持有 cache：

```rust
let mut cache = Vec::new();
let generator = Generate::<KVCache>::new(model, &mut cache, temperature, &prompt_tokens);
```

见 `node-hub/dora-qwen3-translator/src/main.rs:343-345`。

- 当前 EOS 判断写死为 `151643 || 151645`，见 `node-hub/dora-qwen3-translator/src/main.rs:372-375`。
- tokenizer 与 chat template 处理逻辑目前独立于 `qwen3-mlx`，使用的是 `mlx-lm-utils::tokenizer` 和 `tokenizer_config.json`，见 `node-hub/dora-qwen3-translator/src/main.rs:39`, `560-564`。

## 6. 切换到 `qwen3.5-35B-mlx` 需要的代码改动

### 6.1 Cargo 依赖改动

当前 translator 只声明了 `qwen3-mlx`，没有 `qwen3-5-35b-mlx`。见 `node-hub/dora-qwen3-translator/Cargo.toml:8-13`。

需要改动：

1. 在 `node-hub/dora-qwen3-translator/Cargo.toml` 中移除或并存替换 `qwen3-mlx` 依赖。
2. 新增 `qwen3-5-35b-mlx` 依赖。
3. 如果希望继续走本地 OminiX checkout 的 patch 方式，需要在 workspace 根 `Cargo.toml` 的 `[patch."https://github.com/OminiX-ai/OminiX-MLX.git"]` 中新增 `qwen3-5-35b-mlx` 对应 patch。

原因：

- 当前 root patch 只覆盖了 `qwen3-mlx`、`mlx-rs`、`mlx-rs-core`、`mlx-sys`、`mlx-lm-utils`，没有覆盖 `qwen3-5-35b-mlx`。见 `Cargo.toml:53-59`。
- 如果不补 patch，依赖会走 git 仓库中的 crate，而不是本地你正在调的 `OminiX-MLX/qwen3.5-35B-mlx`。

### 6.2 import 与模型类型改动

当前：

```rust
use qwen3_mlx::{load_model, Generate, KVCache};
```

需要改为类似：

```rust
use qwen3_5_35b_mlx::{load_model, Generate};
```

同时：

- `translate_and_emit()` 参数中的 `model: &mut qwen3_mlx::Model`
- `finalize_pending_session()` 参数中的 `model: &mut qwen3_mlx::Model`

都要替换为 `qwen3_5_35b_mlx::Model`。

### 6.3 生成器调用方式改动

这是最核心的代码变更。

当前 `qwen3-mlx` 路线：

```rust
let mut cache = Vec::new();
let generator = Generate::<KVCache>::new(model, &mut cache, temperature, &prompt_tokens);
```

`qwen3.5-35B-mlx` 路线：

```rust
let generator = Generate::new(model, temperature, &prompt_tokens);
```

原因：

- `qwen3-mlx` 由调用方显式持有 `Vec<KVCache>`。
- `qwen3.5-35B-mlx` 的 `Generate` 自己内部持有 `Vec<HybridCache>`，见 `qwen3.5-35B-mlx/src/lib.rs:46-76`。
- `Model::forward()` 会在首次调用时自己初始化 `HybridCache::KV` 或 `HybridCache::Recurrent`，见 `qwen3.5-35B-mlx/src/model.rs:120-129`。

这意味着 translator 里当前的外部 `cache` 变量应当完全移除，而不是简单替换类型。

### 6.4 EOS 处理必须改

当前 translator 的 EOS 终止条件是：

```rust
if token_id == 151643 || token_id == 151645 {
    break;
}
```

见 `node-hub/dora-qwen3-translator/src/main.rs:372-375`。

这不适合 Qwen3.5-2B。

从 `Qwen3.5-2B-MLX-4bit/config.json` 可以看到：

```json
"text_config": {
  "eos_token_id": 248044
}
```

而 `qwen3.5-35B-mlx/examples/generate.rs` 也明确采用“从 `config.json` 读取 EOS token 集合”的方式，而不是硬编码。见 `qwen3.5-35B-mlx/examples/generate.rs`。

因此，切换 loader 时，translator 最好同步做一个正确性改进：

- 新增 `load_eos_tokens(model_path)` helper
- 从 `config.json` 读取 `eos_token_id`
- 支持 `number` 和 `array` 两种形式
- 生成循环里改用 `eos_tokens.contains(&token_id)`

这是“必须改”的项，不建议继续硬编码。

### 6.5 tokenizer / chat template 逻辑大概率可保留

当前 prompt 构造链路是：

1. `Tokenizer::from_file(tokenizer.json)`
2. `load_model_chat_template_from_file(tokenizer_config.json)`
3. `build_prompt_token_ids()`
4. 特殊处理 `enable_thinking=false`

见 `node-hub/dora-qwen3-translator/src/main.rs:210-305` 与 `560-564`。

这部分目前没有直接依赖 `qwen3-mlx::Model` 类型，因此大概率可以保留。

保留理由：

- 2B 模型仍然是 Qwen3.5 家族，chat template 机制并未换成完全不同的体系。
- 当前 translator 的 prompt 渲染是独立做的，不依赖 `qwen3-mlx` 的 tokenizer API。
- 这条链路只负责把 prompt 编成 token id，不负责模型结构加载。

需要注意的风险：

- 由于 2B 模型是 `Qwen3_5ForConditionalGeneration`，其 tokenizer config 可能包含额外多模态相关字段。
- 但只要 text-only 聊天模板对纯文本消息可正常渲染，这不是阻塞点。

因此，这一项结论是：

- 初次迁移时可以先保留。
- 如果实际运行出现 prompt 渲染异常，再单独处理 tokenizer/chat template。

### 6.6 默认模型路径与日志文案需要同步

当前默认路径解析函数：

```rust
QWEN3_TRANSLATOR_MODEL_PATH
~/.OminiX/models/qwen3-8b-4bit
```

见 `node-hub/dora-qwen3-translator/src/main.rs:172-182`。

如果切换为 2B Qwen3.5，需要同步改以下内容：

- 默认目录名
- 启动日志里的 “Loading Qwen3 model”
- 成功日志里的 “Qwen3 model loaded”
- `model_id` 默认值和相关日志

这些不是技术阻塞，但会影响排查和运维。

### 6.7 `force_disable_thinking` 逻辑需要重新确认

当前逻辑：

```rust
let force_disable_thinking = model_id.to_lowercase().contains("qwen3");
```

见 `node-hub/dora-qwen3-translator/src/main.rs:573-583`。

由于 `qwen3.5` 同样包含 `qwen3`，这段逻辑大概率仍会生效。

但建议在迁移时做一次显式确认：

- 如果 2B 模型的 chat template 仍支持 `enable_thinking`
- 那么该逻辑可继续沿用
- 否则应回退到默认模板路径

这不是第一优先级改动，但应该纳入迁移检查清单。

## 7. 推荐的最小改造方案

建议采用“最小修改、先跑通 loader”的策略，不要在第一轮同时大改 prompt 和会话状态机。

### 第一阶段：只替换推理后端

改动范围：

1. Cargo 依赖从 `qwen3-mlx` 切到 `qwen3-5-35b-mlx`
2. 替换 import
3. 替换模型类型
4. 把 `Generate::<KVCache>::new(...)` 改成 `Generate::new(...)`
5. 删除外部 `cache` 变量
6. 把 EOS 逻辑改为读 `config.json`
7. 把默认模型路径改到 Qwen3.5-2B

这一阶段不建议改：

- Dora session 状态机
- `translate_and_emit()` 的 streaming 粒度
- chat template 渲染逻辑
- `mlx_clear_cache()` 位置

### 第二阶段：Mac mini 上做最小验证

按以下顺序验证：

1. 仅验证 `load_model()` 是否成功
2. 仅验证一条固定 prompt 能否输出 token
3. 再接回 Dora translator 节点
4. 再观察实时翻译是否还会出现“2-3 句后停止”

## 8. 风险与收益判断

### 收益

- 2B 模型显著小于当前 8B translator 模型，内存压力预计明显下降。
- `qwen3.5-35B-mlx` 的 hybrid 架构里，大量层使用固定大小 recurrent state，而不是全量增长型 KV cache。见 `qwen3.5-35B-mlx/src/cache.rs:4-39`。
- 这对 16GB Mac mini 的持续运行更有利。

### 风险

- 2B 模型是 `ForConditionalGeneration` 变体，虽然文本骨干兼容证据充分，但首次加载仍需实际验证。
- 翻译质量可能弱于现有 8B 模型。
- 如果内存问题的另一半来自 ASR，而不只是 translator，那么更换 2B translator 只能缓解，不一定一次性根治。

## 9. 最终判断

本次研究的最终判断如下：

1. `mlx-community/Qwen3.5-2B-MLX-4bit` 与 `OminiX-MLX/qwen3.5-35B-mlx` 在架构字段、缓存模型和权重命名上均高度匹配，具备明确的源码级兼容证据。
2. 当前 `dora-qwen3-translator` 不能通过“只换模型目录”直接切换到该 crate，至少需要替换依赖、模型类型、生成器调用方式和 EOS 逻辑。
3. tokenizer / chat template 路线大概率可以保留，不需要第一轮就改。
4. 最合理的推进方式是在 Mac mini 上先做“只替换 loader 的最小改造”，验证加载和生成成功后，再评估是否继续把 8B translator 切成 2B translator。

## 10. 基于 `qwen3.5-35B-mlx` 的推荐模型结论

在重新以 `qwen3.5-35B-mlx` 为基准审视 Hugging Face 上的 `mlx-community` 模型后，当前最合适的候选是：

- 首选模型：[mlx-community/Qwen3.5-2B-MLX-4bit](https://huggingface.co/mlx-community/Qwen3.5-2B-MLX-4bit)

选择理由：

1. 它满足“Qwen3.5 + 参数量小”的前提。
2. 它的 `config.json` 和 `model.safetensors.index.json` 已经被本次研究逐项核对，和 `qwen3.5-35B-mlx` 的 hybrid loader 期望一致。
3. 它使用的是标准 MLX affine 4bit 路线，风险低于 OptiQ 这种额外量化变体。
4. 相比 0.8B，它在内存和翻译质量之间更平衡，更适合作为当前 translator 的第一候选。

### 10.1 候选模型比较

| 模型 | 参数量 | 直接替换把握 | 说明 | 推荐级别 |
| --- | --- | --- | --- | --- |
| [mlx-community/Qwen3.5-2B-MLX-4bit](https://huggingface.co/mlx-community/Qwen3.5-2B-MLX-4bit) | 2B | 高 | 已核对过 hybrid `text_config`、`language_model.model.*` 权重前缀、`linear_attn`/`self_attn` 键名 | 首选 |
| [mlx-community/Qwen3.5-0.8B-4bit](https://huggingface.co/mlx-community/Qwen3.5-0.8B-4bit) | 0.8B | 高 | 结构上同样兼容 `qwen3.5-35B-mlx`，但翻译质量风险更高 | 次选 |
| [mlx-community/Qwen3.5-2B-OptiQ-4bit](https://huggingface.co/mlx-community/Qwen3.5-2B-OptiQ-4bit) | 2B | 中 | 参数量合适，但量化形式不是本次已验证的标准 affine 4bit 路线 | 不优先 |

### 10.2 为什么首选 2B，而不是 0.8B

`mlx-community/Qwen3.5-0.8B-4bit` 也具备直接接入 `qwen3.5-35B-mlx` 的结构条件：

- `model_type` 仍是 `qwen3_5`
- `text_config.layer_types` 仍是 24 层 hybrid 结构
- 权重前缀仍是 `language_model.model.*`
- 同样存在 `linear_attn.*`、`self_attn.*`、`embed_tokens`、`norm.weight`

因此，从“能不能被 loader 直接加载”的角度看，0.8B 并不是不兼容。

但从实时翻译场景看，0.8B 的主要问题不是兼容性，而是质量：

- 它当然会进一步减轻内存压力
- 但也更可能在会议口语翻译、碎片句翻译、抗噪场景下明显掉质量

因此，如果当前目标是：

- 先解决内存和稳定性
- 同时保住一个基本可用的翻译体验

那么 2B 比 0.8B 更合理。

### 10.3 为什么不优先推荐 2B OptiQ

`mlx-community/Qwen3.5-2B-OptiQ-4bit` 的规模和任务类型都看起来合适，但本次研究并没有像标准 `Qwen3.5-2B-MLX-4bit` 那样，对它的配置和权重命名做过同等强度的逐项核对。

更关键的是：

- 当前 `qwen3.5-35B-mlx` 已明确按标准 MLX quantized weight 方式加载
- 本次已验证的 2B 标准版模型使用的是标准 affine 4bit 配置
- OptiQ 属于额外的量化变体，虽然未必不兼容，但不适合被定义为“最稳妥的直接替换方案”

因此，OptiQ 更像后续优化选项，而不是当前第一轮切换的推荐目标。

## 11. Mac mini 上的建议验证清单

1. 先写一个最小独立测试，只做 `load_model + one prompt + 32 tokens`。
2. 验证 EOS 是否按 `config.json` 正常停止。
3. 验证 tokenizer/chat template 是否能正常生成 text-only prompt。
4. 再把相同改动接回 `dora-qwen3-translator`。
5. 运行实时翻译，观察内存、`cache_memory`、translator 是否仍在 2-3 句后停止。
