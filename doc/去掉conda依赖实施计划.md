# 去掉 Conda/Python 依赖实施计划

**分支**：`main`
**创建日期**：2026-03-25
**最后更新**：2026-03-25（计划创建）

---

## 背景

Moxin Voice 经历多次后端重构（Python FunASR → Python PrimeSpeech → OminiX-MLX → Qwen3-only），
运行时已全面切换为 Rust 原生。当前所有 Dora 节点均为 Rust 二进制，不再有任何 Python 节点。

然而 bootstrap 脚本仍保留了完整的 Conda 环境搭建流程——整套环境的唯一实际用途
是在首次启动时下载两个 HuggingFace 模型（TTS × 2 + ASR × 1）。

本次重构目标：**彻底去掉 Conda/Python 依赖，改用纯 Rust 模型下载器。**

---

## 进度总览

| # | 模块 | 描述 | 状态 | 备注 |
|---|------|------|------|------|
| 1 | 计划文档 | 本文件 | ✅ 完成 | |
| 2 | `moxin-init` crate | Rust 模型下载器二进制 | ⏳ 待开始 | 核心新增 |
| 3 | workspace Cargo.toml | 注册新成员，验证编译 | ⏳ 待开始 | |
| 4 | `macos_bootstrap.sh` | 大幅简化，去掉 conda | ⏳ 待开始 | 243 行 → ~30 行 |
| 5 | `macos_preflight.sh` | 删除 conda/Python 检查 | ⏳ 待开始 | |
| 6 | `build_macos_app.sh` | 打包 moxin-init，删 python-src | ⏳ 待开始 | |

**状态说明**：⏳ 待开始 · 🔄 进行中 · ✅ 完成 · 🔴 阻塞

---

## 方案架构

### 新增：`moxin-init/` workspace 成员

```
moxin-init/
├── Cargo.toml      # 依赖：reqwest(blocking), serde_json, anyhow, dirs
└── src/
    └── main.rs     # ~200 行，完整模型下载器
```

职责：
- 检查模型是否已就绪（跳过已下载的）
- 通过 HuggingFace HTTP API 列出 repo 文件列表
- 逐文件下载，支持断点续传（Range 请求）
- 实时写入 `bootstrap_state.txt`（格式与现有 screen.rs 兼容）
- 支持 `HF_ENDPOINT` 环境变量（国内镜像）

归属说明：下载器是**应用级基础设施**，不属于任何单个 Dora 节点（TTS/ASR 各有
自己的模型，未来还会增加更多节点），因此作为独立 workspace 成员而非放入某个节点。

### 模型下载逻辑

```
HF_ENDPOINT（默认 https://huggingface.co）
  ├── GET /api/models/{repo_id}          → 获取文件列表（siblings）
  └── GET /{repo_id}/resolve/main/{file} → 下载单个文件（自动跟随 LFS 302 重定向）
```

断点续传：检查目标文件已有字节数 → 发送 `Range: bytes={n}-` → HTTP 206 追加写入。

### State file 格式（与 screen.rs 兼容）

```
{current}/{total}|{标题}|{详情}
```

示例：
```
1/3|Download TTS CustomVoice|[3/48] model.safetensors
```

### 简化后的 bootstrap 流程

**现在（243 行，依赖 conda）：**
```
Step 1  安装/检查 Miniforge conda
Step 2  conda install git git-lfs
Step 3  pip install pip/setuptools/wheel
Step 4  pip install dora-common            ← 完全没用
Step 5  Skip (Python ASR 已移除)
Step 6  Skip (PrimeSpeech 已移除)
Step 7  huggingface-cli download ASR       ← 真正的目的
Step 8  Skip (模型转换已移除)
Step 9  python download_qwen3_tts_models.py ← 真正的目的
Step 10 Finalize
```

**重构后（~30 行，纯 Rust）：**
```
Step 1  定位 moxin-init 二进制
Step 2  exec moxin-init（内部写 state，下载所有模型）
```

---

## 关键设计决策

### 为什么用 `reqwest` 而不是 `hf-hub` crate？

- `reqwest` 更底层，可直接写到目标路径，无中间 cache 层
- `hf-hub` 强制下载到 `~/.cache/huggingface/hub/` 再 copy，两次 I/O
- HuggingFace 的 HTTP API 足够简单稳定（`/api/models/{repo}` + `/resolve/main/{file}`）
- LFS 文件通过 302 重定向到 S3，`reqwest` 的 `redirect::follow` 自动处理

### 为什么是独立 workspace 成员而不是 bin target in TTS 节点？

- 下载器需要管理所有模型（TTS × 2 + ASR，未来更多），不属于任何单个节点
- 独立 crate 职责单一，依赖关系清晰（只需 reqwest/serde_json/dirs/anyhow）
- 与已有 workspace 层次结构（shell/widgets/bridge/apps/node-hub）一致

---

## 影响文件清单

| 文件 | 变化类型 | 说明 |
|------|----------|------|
| `moxin-init/Cargo.toml` | 新增 | workspace 新成员 |
| `moxin-init/src/main.rs` | 新增 | ~200 行下载器实现 |
| `Cargo.toml`（根） | 修改 | members 列表加入 moxin-init |
| `scripts/macos_bootstrap.sh` | 大幅重写 | 243 行 → ~30 行，去掉 conda |
| `scripts/macos_preflight.sh` | 修改 | 删除 conda/Python 检查 |
| `scripts/build_macos_app.sh` | 修改 | 构建/打包 moxin-init，删除 python-src |

**不涉及**：`screen.rs`（init 流程不变）、任何 Dora 节点、UI 代码

---

## 模块详情

### 模块 2：moxin-init/src/main.rs

核心函数：

- `tts_model_ready(dir)` — 检查 TTS 模型完整性（7 个文件）
- `asr_model_ready(dir)` — 检查 ASR 模型完整性（config.json）
- `list_repo_files(client, repo_id)` — GET /api/models/{repo} → Vec<String>
- `download_file(client, repo_id, filename, dest)` — 单文件下载，支持 Range 续传
- `download_repo(client, repo_id, target_dir, ...)` — 完整 repo 下载
- `write_state(path, current, total, title, detail)` — 写 state file
- `main()` — 读环境变量配置，顺序下载 CustomVoice → Base → ASR（optional）

环境变量（全部可选，有合理默认值）：
- `MOXIN_BOOTSTRAP_STATE_PATH`
- `QWEN3_TTS_MODEL_ROOT`
- `QWEN3_TTS_CUSTOMVOICE_MODEL_DIR` / `QWEN3_TTS_CUSTOMVOICE_REPO`
- `QWEN3_TTS_BASE_MODEL_DIR` / `QWEN3_TTS_BASE_REPO`
- `QWEN3_ASR_MODEL_PATH` / `QWEN3_ASR_REPO`
- `HF_ENDPOINT`（国内镜像支持）

### 模块 4：macos_bootstrap.sh（重写）

```bash
#!/usr/bin/env bash
set -euo pipefail
# 1. 解析路径变量
# 2. 定位 moxin-init（bundle MacOS/ 或 dev target/）
# 3. exec moxin-init（传递环境变量）
```

### 模块 5：macos_preflight.sh（简化）

删除：
- conda 安装检查（`CONDA_BIN`、`CONDA_ENV_PREFIX` 相关）
- Python 环境检查
- `dora-common` 检查

保留：
- `dora` 命令检查
- `qwen-tts-node` 二进制检查
- `moxin-init` 二进制检查（新增）
- Qwen3 TTS 模型就绪检查
- Qwen3 ASR 模型就绪检查（warning 级别）
- dataflow 文件检查

---

## 提交计划

| 提交 | 内容 |
|------|------|
| ① | feat(init): add moxin-init Rust model downloader |
| ② | refactor(bootstrap): replace conda/Python with moxin-init |
| ③ | build(macos): bundle moxin-init, remove python-src |
