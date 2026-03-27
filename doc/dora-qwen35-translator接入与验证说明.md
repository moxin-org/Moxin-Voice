# dora-qwen35-translator 接入与验证说明

## 1. 本次更新概要

本次更新新增了一个独立的 Dora 翻译节点：

- `node-hub/dora-qwen35-translator`

它的目标是：

- 包装 `C:\Users\FPG_123\Documents\projects\OminiX-MLX\qwen3.5-35B-mlx`
- 默认加载 `mlx-community/Qwen3.5-2B-MLX-4bit`
- 在不影响现有 `node-hub/dora-qwen3-translator` 的前提下，提供一条可单独验证的 Qwen3.5 翻译后端

本次改动同时覆盖了分发链路，使新节点具备被打包、被 bootstrap 下载模型、被 preflight 检查的基础条件。

## 2. 目前更新后的现状

### 2.1 新增的节点与数据流

新增节点：

- [node-hub/dora-qwen35-translator/Cargo.toml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen35-translator\Cargo.toml)
- [node-hub/dora-qwen35-translator/src/main.rs](C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen35-translator\src\main.rs)

新增数据流模板：

- 开发用模板：[apps/moxin-voice/dataflow/translation_qwen35.yml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\apps\moxin-voice\dataflow\translation_qwen35.yml)
- 打包用模板：[scripts/dataflow/translation_qwen35.bundle.yml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\scripts\dataflow\translation_qwen35.bundle.yml)

### 2.2 新节点的默认行为

新节点当前实现特征：

- 使用 `qwen3-5-35b-mlx` crate，而不是 `qwen3-mlx`
- 默认模型路径：
  - `~/.OminiX/models/Qwen3.5-2B-MLX-4bit`
- 覆盖环境变量：
  - `QWEN35_TRANSLATOR_MODEL_PATH`
- 生成逻辑：
  - 使用 `Generate::new(model, temperature, &prompt_tokens)`
- 停止条件：
  - 从 `config.json` 中读取 `eos_token_id`
  - 不再使用旧节点里的硬编码 EOS

### 2.3 分发相关改动

已更新的分发文件：

- Workspace 注册新 crate：
  - [Cargo.toml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\Cargo.toml)
- 打包脚本新增 build/copy/chmod 与 bundle YAML：
  - [scripts/build_macos_app.sh](C:\Users\FPG_123\Documents\projects\Moxin-Voice\scripts\build_macos_app.sh)
- bootstrap 新增 Qwen3.5-2B 模型下载环境变量透传：
  - [scripts/macos_bootstrap.sh](C:\Users\FPG_123\Documents\projects\Moxin-Voice\scripts\macos_bootstrap.sh)
- `moxin-init` 新增 Qwen3.5-2B 下载步骤：
  - [moxin-init/src/main.rs](C:\Users\FPG_123\Documents\projects\Moxin-Voice\moxin-init\src\main.rs)
- preflight 新增新节点 binary 与模型检查：
  - [scripts/macos_preflight.sh](C:\Users\FPG_123\Documents\projects\Moxin-Voice\scripts\macos_preflight.sh)

### 2.4 当前仍保持不变的部分

现有翻译后端没有被替换：

- [node-hub/dora-qwen3-translator/Cargo.toml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen3-translator\Cargo.toml)
- [node-hub/dora-qwen3-translator/src/main.rs](C:\Users\FPG_123\Documents\projects\Moxin-Voice\node-hub\dora-qwen3-translator\src\main.rs)
- [apps/moxin-voice/dataflow/translation.yml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\apps\moxin-voice\dataflow\translation.yml)

当前 UI 代码默认仍然解析并启动旧的 `translation.yml`，不会自动切到新节点。

这意味着：

- 旧翻译链路仍可继续作为对照组存在
- 新节点需要你在 Mac 上手动单独验证

### 2.5 当前已知限制

本次在 Windows 上完成，未进行以下验证：

- 未编译 `dora-qwen35-translator`
- 未在 Apple Silicon 上实际加载 `Qwen3.5-2B-MLX-4bit`
- 未跑 Dora dataflow
- 未执行 shell 脚本语法校验，因为本机 WSL `bash` 不可用

因此，本次结果应视为：

- 代码接入和分发链路已完成
- 运行正确性仍需在 Mac mini 上验证

## 3. 验证 dora-qwen35-translator 的步骤清单

建议按“先最小验证，再接 Dora”的顺序执行，不要一开始就直接接入 UI。

### 3.1 准备阶段

1. 确认当前代码已经拉到包含本次改动的提交。
2. 确认 Mac 上本地 `OminiX-MLX` 依赖可用于编译本项目。
3. 确认 `Qwen3.5-2B-MLX-4bit` 已下载到默认目录，或准备设置：
   - `QWEN35_TRANSLATOR_MODEL_PATH`

建议先检查模型目录至少存在：

- `config.json`
- `tokenizer.json`
- `tokenizer_config.json`
- `model.safetensors` 或 `model.safetensors.index.json`

### 3.2 编译新节点

在项目根目录执行：

```bash
cargo build -p dora-qwen35-translator --release
```

成功标志：

- 生成二进制：
  - `target/release/dora-qwen35-translator`

如果失败，优先看两类问题：

- `qwen3-5-35b-mlx` 依赖是否能正确解析
- Apple Silicon / MLX 相关 crate 是否能在本机工具链下编译

### 3.3 最小加载验证

在不经过 Dora 的情况下，先只验证：

1. 模型能加载
2. prompt 能编码
3. 能输出 token
4. 能按 EOS 正常停止

如果你愿意，可以临时写一个最小测试程序；如果不想额外写测试，下一步直接做 Dora 节点验证也可以，但排障成本会更高。

### 3.4 Dora 节点单独验证

先启动 Dora：

```bash
dora up
```

然后使用新的数据流模板：

- [translation_qwen35.yml](C:\Users\FPG_123\Documents\projects\Moxin-Voice\apps\moxin-voice\dataflow\translation_qwen35.yml)

建议先把模板中的 `__ASR_BIN_PATH__` 和 `__TRANSLATOR_BIN_PATH__` 替换成实际路径，或者复用应用里已有的模板替换逻辑。

如果手动验证，可先生成一份 runtime YAML，再执行：

```bash
dora start <runtime-translation-qwen35.yml>
```

重点观察：

- `asr` 节点是否启动
- `translator` 节点是否启动
- `translator` 是否在启动时成功加载模型
- 是否能输出 `source_text`
- 是否能输出 `translation`

### 3.5 日志验证要点

重点看新节点日志里是否出现：

- `dora-qwen35-translator starting`
- `Loading Qwen3.5 model from: ...`
- `Qwen3.5 model loaded`

如果模型加载失败，优先检查：

1. `QWEN35_TRANSLATOR_MODEL_PATH` 是否指向正确目录
2. 模型目录是否完整
3. 本机实际下载的模型是否就是 `mlx-community/Qwen3.5-2B-MLX-4bit`

### 3.6 运行时功能验证

建议至少验证以下场景：

1. 单句短输入能翻译
2. 连续两三句输入仍然持续工作
3. `source_text` 和 `translation` 都有输出
4. 长一点的句子不会立即卡死

如果要验证这次换模型是否缓解了“2-3 句后停止”，建议同时观察：

- Activity Monitor 内存压力
- translator 进程是否仍存活
- 是否还出现内存在黄区后节点停止的现象

### 3.7 分发链验证

如果要连分发一起验证，再额外执行：

```bash
bash -n scripts/build_macos_app.sh
bash -n scripts/macos_bootstrap.sh
bash -n scripts/macos_preflight.sh
```

然后：

```bash
./scripts/build_macos_app.sh
```

验证项：

1. app bundle 中存在：
   - `Contents/MacOS/dora-qwen35-translator`
2. bundle 中存在：
   - `Contents/Resources/dataflow/translation_qwen35.yml`
3. bootstrap 能识别：
   - `QWEN35_TRANSLATOR_MODEL_PATH`
   - `QWEN35_TRANSLATOR_REPO`
4. preflight 对缺失模型给出 warning，而不是破坏旧链路

## 4. 建议的验证顺序

建议严格按下面顺序执行：

1. 编译 `dora-qwen35-translator`
2. 验证模型目录完整
3. 做最小加载/生成验证
4. 用 `translation_qwen35.yml` 做 Dora 验证
5. 观察内存与稳定性
6. 最后再决定是否替换现有 `dora-qwen3-translator`

这个顺序的目的很简单：

- 先确认新节点本身成立
- 再确认它是否真的比旧后端更适合你的实时翻译场景

