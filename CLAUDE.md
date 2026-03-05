# CLAUDE.md - Moxin TTS 项目上下文

> 本文档用于 Claude 在新对话中快速恢复项目上下文

**创建日期**: 2026-02-14
**文档版本**: 1.0
**项目仓库**: https://github.com/alan0x/moxin-voice

---

## 📋 项目概述

### 项目简介

**Moxin TTS** 是一个现代化的、GPU 加速的桌面应用程序，专注于文本转语音（TTS）和语音克隆功能。它完全使用 Rust 构建，采用 Makepad UI 框架，由 GPT-SoVITS v2 驱动，提供先进的语音克隆和合成能力。

### 项目起源

- **原始项目**: moxin-studio - 包含多个应用的 AI 桌面平台
- **演化**: 从 moxin-studio 中提取 TTS 功能，创建独立的专用应用
- **目标**: 专注于 TTS 和语音克隆，提供最佳用户体验

### 核心功能

1. **文本转语音 (TTS)** - 自然流畅的语音合成，14+ 预置语音
2. **零样本语音克隆 (Express Mode)** - 仅需 5-10 秒音频即可克隆声音
3. **Few-Shot 训练 (Pro Mode)** - 使用 3-10 分钟音频进行高质量语音克隆
4. **音频录制** - 内置实时可视化音频录制功能
5. **语音识别 (ASR)** - 从音频自动识别文本
6. **音频导出** - 保存生成的语音为 WAV 文件

### 技术亮点

- 🎨 GPU 加速渲染，流畅动画
- 🌓 原生暗色主题支持
- 🚀 Rust 原生性能
- 🔧 模块化架构设计
- 🎯 Dora 数据流集成

---

## 🏗️ 核心技术栈

### 前端技术

| 技术        | 版本/信息         | 用途             |
| ----------- | ----------------- | ---------------- |
| **Rust**    | 2021 edition      | 主要编程语言     |
| **Makepad** | git rev 53b2e5c84 | GPU 加速 UI 框架 |
| **CPAL**    | 0.15              | 跨平台音频 I/O   |
| **Tokio**   | 1.x               | 异步运行时       |
| **Serde**   | 1.0               | 序列化框架       |

### 后端技术

| 技术                  | 用途                       |
| --------------------- | -------------------------- |
| **Python**            | 3.8+                       |
| **GPT-SoVITS v2**     | 语音克隆和 TTS 引擎        |
| **Dora**              | 机器人数据流框架 (v0.3.12) |
| **FunASR Paraformer** | 语音识别 (ASR)             |
| **PyTorch**           | 2.2.0 (深度学习)           |

---

## 📂 项目结构

### 目录树

```
moxin-voice/
├── moxin-voice-shell/          # ⭐ 独立应用入口点
│   ├── src/
│   │   ├── main.rs           # CLI 参数解析和启动 (47 行)
│   │   └── app.rs            # 主应用逻辑 (147 行)
│   ├── Cargo.toml
│   ├── README.md
│   ├── BUILDING.md
│   └── IMPLEMENTATION_SUMMARY.md
│
├── apps/
│   └── moxin-voice/             # TTS 应用逻辑库
│       ├── src/
│       │   ├── lib.rs                # 库入口
│       │   ├── screen.rs             # 主界面
│       │   ├── voice_selector.rs     # 语音选择器组件
│       │   ├── voice_clone_modal.rs  # 语音克隆模态框
│       │   ├── voice_data.rs         # 语音数据结构
│       │   ├── voice_persistence.rs  # 语音数据持久化
│       │   ├── audio_player.rs       # 音频播放器
│       │   ├── training_manager.rs   # 训练管理器
│       │   └── dora_integration.rs   # Dora 集成
│       ├── dataflow/
│       │   └── tts.yml               # Dora TTS 数据流配置
│       └── Cargo.toml
│
├── moxin-widgets/             # 🎨 共享 UI 组件
│   ├── src/
│   │   ├── theme.rs          # MoxinTheme 定义
│   │   ├── audio_recorder.rs # 音频录制组件
│   │   └── ...
│   └── Cargo.toml
│
├── moxin-ui/                  # 🔧 应用基础设施
│   ├── src/
│   │   ├── app_data.rs       # MoxinAppData
│   │   └── ...
│   └── Cargo.toml
│
├── moxin-dora-bridge/         # 🌉 Dora 数据流集成桥接
│   ├── src/
│   │   ├── shared_state.rs   # SharedDoraState
│   │   └── ...
│   └── Cargo.toml
│
├── node-hub/                 # 🐍 Python Dora 节点
│   ├── dora-primespeech/     # GPT-SoVITS TTS 引擎节点
│   │   ├── dora_primespeech/
│   │   │   ├── main.py       # TTS 节点主程序
│   │   │   ├── training_service.py  # 训练服务
│   │   │   └── moxin_tts/   # GPT-SoVITS 核心
│   │   ├── setup.py
│   │   └── pyproject.toml
│   │
│   └── dora-asr/             # FunASR 语音识别节点
│       ├── dora_asr/
│       │   └── main.py
│       ├── setup.py
│       └── pyproject.toml
│
├── models/
│   ├── setup-local-models/   # 模型设置脚本
│   │   ├── setup_isolated_env.sh       # Conda 环境设置
│   │   ├── install_all_packages.sh     # 安装所有包
│   │   ├── quick_setup_macos.sh        # macOS 快速设置
│   │   └── test_dependencies.py        # 依赖测试
│   └── model-manager/        # 模型下载管理器
│       └── download_models.py
│
├── doc/                      # 📚 项目文档
│   ├── CONTEXT_RESUME.md     # 详细上下文恢复文档
│   ├── DEBUG_LOG.md          # 调试日志记录
│   ├── TTS项目对比分析报告.md
│   ├── moxin-voice-fewshot决策分析.md
│   └── ...
│
├── Cargo.toml                # Workspace 配置
├── README.md                 # 项目主 README
├── QUICKSTART_MACOS.md       # macOS 快速开始
├── MACOS_SETUP.md            # macOS 详细设置
└── CLAUDE.md                 # 本文件

```

### Workspace 成员

根据 `Cargo.toml`:

```toml
members = [
    "moxin-voice-shell",     # 主应用入口
    "moxin-widgets",         # UI 组件库
    "moxin-dora-bridge",     # Dora 桥接
    "moxin-ui",              # 应用基础设施
    "apps/moxin-voice",        # TTS 应用逻辑
]
```

---

## 🔑 关键文件说明

### 应用入口层

| 文件                            | 说明       | 关键点                           |
| ------------------------------- | ---------- | -------------------------------- |
| `moxin-voice-shell/src/main.rs` | CLI 入口点 | 解析命令行参数，初始化日志       |
| `moxin-voice-shell/src/app.rs`  | 主应用逻辑 | Makepad 应用结构，live_design 宏 |
| `moxin-voice-shell/Cargo.toml`  | 应用包配置 | 依赖关系，二进制定义             |

### TTS 核心层

| 文件                                        | 说明           | 关键点               |
| ------------------------------------------- | -------------- | -------------------- |
| `apps/moxin-voice/src/screen.rs`            | 主界面         | 现代侧边栏布局       |
| `apps/moxin-voice/src/voice_clone_modal.rs` | 语音克隆对话框 | Express/Pro 模式切换 |
| `apps/moxin-voice/src/training_manager.rs`  | 训练管理器     | 异步训练编排         |
| `apps/moxin-voice/src/dora_integration.rs`  | Dora 集成      | 与 Python 节点通信   |

### Python 节点层

| 文件                                                             | 说明           | 关键点                  |
| ---------------------------------------------------------------- | -------------- | ----------------------- |
| `node-hub/dora-primespeech/dora_primespeech/main.py`             | TTS 节点主程序 | 处理 TTS 请求，模型加载 |
| `node-hub/dora-primespeech/dora_primespeech/training_service.py` | 训练服务       | GPT + SoVITS 训练流程   |
| `node-hub/dora-asr/dora_asr/main.py`                             | ASR 节点       | 实时语音识别            |

### 数据流配置

| 文件                                | 说明           | 关键点                 |
| ----------------------------------- | -------------- | ---------------------- |
| `apps/moxin-voice/dataflow/tts.yml` | TTS 数据流定义 | 节点连接，输入输出定义 |

### 文档

| 文件                    | 说明           | 用途                               |
| ----------------------- | -------------- | ---------------------------------- |
| `doc/CONTEXT_RESUME.md` | 详细上下文文档 | 项目历史、已完成工作、问题修复记录 |
| `doc/DEBUG_LOG.md`      | 调试日志       | Error 1-27 修复记录                |
| `README.md`             | 项目主文档     | 功能介绍、快速开始、架构说明       |
| `MACOS_SETUP.md`        | macOS 设置指南 | macOS 特定设置步骤                 |
| `QUICKSTART_MACOS.md`   | macOS 快速开始 | 5 分钟快速设置                     |

---

## 🚀 开发工作流

### 环境准备

#### 系统要求

- **macOS**: Darwin 25.1.0 (当前开发环境)
- **Rust**: 1.70+ (2021 edition)
- **Python**: 3.8+ (推荐 3.12)
- **Conda**: 用于隔离 Python 环境

#### 安装步骤

```bash
# 1. macOS 系统依赖
./install_macos_deps.sh

# 2. Python 环境设置
cd models/setup-local-models
./setup_isolated_env.sh        # 创建 moxin-studio conda 环境

# 3. 安装 Python 包
conda activate moxin-studio
./install_all_packages.sh

# 4. 验证依赖
python test_dependencies.py

# 5. 下载模型
cd ../model-manager
python download_models.py --download funasr        # ASR 模型
python download_models.py --download primespeech   # TTS 模型
python download_models.py --list-voices            # 查看可用语音
```

模型存储位置：

- ASR: `~/.dora/models/asr/funasr/`
- TTS: `~/.dora/models/primespeech/`

### 构建和运行

```bash
# 开发构建（快速，带调试符号）
cargo build -p moxin-voice

# Release 构建（优化，生产用）
cargo build -p moxin-voice --release

# 运行
cargo run -p moxin-voice

# 运行带详细日志
cargo run -p moxin-voice -- --log-level debug

# 清理构建产物
cargo clean
```

### Dora 数据流管理

```bash
# 启动 Dora 守护进程
dora up

# 进入数据流目录
cd apps/moxin-voice/dataflow

# 启动 TTS 数据流
dora start tts.yml

# 查看运行状态
dora list

# 停止数据流
dora stop <dataflow-id>

# 停止守护进程
dora down
```

### Git 工作流

```bash
# 当前远程仓库
git remote -v
# origin: https://github.com/alan0x/moxin-voice.git

# 当前分支
git branch
# * main

# 查看状态
git status

# 提交更改
git add .
git commit -m "feat: description"
git push origin main
```

---

## 🎯 项目当前状态

### 开发阶段

| 阶段                 | 状态      | 说明                         |
| -------------------- | --------- | ---------------------------- |
| Phase 1: 基础搭建    | ✅ 100%   | 创建独立 Shell，工作区集成   |
| Phase 2: Shell 修复  | ✅ 100%   | Makepad 初始化，编译错误修复 |
| Phase 3: Few-Shot UI | ✅ 100%   | Express/Pro 模式 UI 实现     |
| Phase 4: 代码库清理  | ✅ 100%   | 删除 24K 行未使用代码        |
| Phase 5: 功能测试    | 🔧 进行中 | TTS 生成、语音克隆测试       |
| Phase 6: 文档和发布  | 📋 待开始 | 完善文档，准备发布           |

### Git 状态快照

```
Current branch: main
Main branch: main

Modified:
  M README.md
  M models/setup-local-models/install_all_packages.sh
  M models/setup-local-models/setup_isolated_env.sh
  M models/setup-local-models/test_dependencies.py

Untracked files:
  ?? CURRENT_STATUS.md
  ?? DORA_MACOS_ISSUE.md
  ?? MACOS_CHECKLIST.md
  ?? (多个 macOS 相关文档)
  ?? debug_dora_macos.sh
  ?? install_macos_deps.sh
  ?? models/setup-local-models/check_macos_deps.sh
  ?? models/setup-local-models/quick_setup_macos.sh
  ?? models/setup-local-models/verify_setup.sh

Recent commits:
  92ac28f - fix: resolve Pro Mode Few-Shot training root cause
  3f87c7b - docs: investigate and document Pro Mode Few-Shot training issue
  61e9d40 - fix: resolve trained voice model loading and UI refresh issues
```

### 关键里程碑

✅ **已完成**:

- 独立应用 Shell 创建
- TTS 屏幕实现（两种布局）
- 零样本语音克隆 UI (Express Mode)
- Few-Shot 训练 UI (Pro Mode)
- 音频录制和播放
- Dora 数据流集成
- Pro Mode 训练问题修复（GPT 预训练模型 + 架构修复）
- 训练音色加载和 UI 刷新问题修复

🚧 **进行中**:

- macOS 平台适配和优化
- 功能测试和验证
- 性能优化

📋 **待完成**:

- 完整端到端测试
- 用户文档完善
- 发布准备

---

## 🔧 关键决策和设计

### 决策 1: 独立 Shell vs Feature Flags

**选择**: 创建独立的 `moxin-voice-shell` (方案 A)
**替代方案**: 使用 feature flags 在 moxin-studio-shell 中切换 (方案 B)

**理由**:

- ✅ 代码独立性强
- ✅ 简洁清晰（~200 行 vs 复杂条件编译）
- ✅ 未来可独立演进
- ✅ 打包体积更小

**评分**: 方案 A 29/30 vs 方案 B 17/30

### 决策 2: Few-Shot 使用 dora-primespeech

**选择**: 使用现有 dora-primespeech 节点

**理由**:

- ✅ dora-primespeech 已包含完整 GPT-SoVITS 工具链
- ✅ 避免重复依赖
- ✅ 架构一致（都是 Dora 节点）

**参考**: `doc/moxin-voice-fewshot决策分析.md`

### 决策 3: ASR 节点选择

**选择**: 使用 dora-asr 进行实时识别
**替代方案**: 使用 dora-primespeech 内置 ASR

**理由**:

- ✅ dora-asr 专为实时识别优化
- ✅ dora-primespeech ASR 是批处理工具（用于训练数据准备）
- ✅ 两者互补，各司其职

---

## 🐛 已知问题和解决方案

### 已解决的关键问题

#### 1. Pro Mode 训练输出空白音频 (Error 27)

**问题**: 训练后的语音模型只生成 ~1.5 秒空白音频

**根本原因**:

1. 缺少 GPT 预训练模型 (`s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt`)
2. `training_service.py` 硬编码了错误的模型架构：
   - `n_layer`: 12 → 应为 24
   - `head`: 8 → 应为 16
   - `phoneme_vocab_size`: 512 → 应为 732

**解决方案**:

1. 下载 GPT 预训练模型 (155 MB)
2. 修正 `training_service.py:211-262` 的模型配置
3. 添加预训练模型强制检查

**状态**: ✅ 已修复并验证

#### 2. 训练音色未加载自定义模型 (Error 22-23)

**问题**: Pro Mode 训练的音色听起来和 doubao 预置音色完全一样

**根本原因**:

1. Rust 代码只识别 `VoiceSource::Custom`，不识别 `VoiceSource::Trained`
2. Python 代码没有传递自定义模型路径的机制

**解决方案**:

1. 新增 `VOICE:TRAINED|<gpt>|<sovits>|<ref>|<prompt>|<lang>|<text>` 协议
2. 更新 Rust 代码支持 Trained voices (`screen.rs:2483-2514`)
3. 更新 Python 代码解析并加载自定义模型 (`main.py:28, 298-323`)

**状态**: ✅ 已修复

#### 3. Voice Library 不刷新 (Error 未编号)

**问题**: Pro Mode 训练完成后新音色不显示

**根本原因**: `on_training_completed` 保存音色后未发送 `VoiceCreated` action

**解决方案**:

1. 修改调用链传递 `scope` 参数
2. 在 `on_training_completed` 中发送 `VoiceCreated(new_voice)` action

**文件**: `voice_clone_modal.rs:1905, 3428, 3442, 3487, 3520, 3566`

**状态**: ✅ 已修复

### 编译警告（可忽略）

```rust
warning: function `get_cli_args` is never used
  --> moxin-voice-shell/src/app.rs:26
// 原因: 预留用于未来功能

warning: struct `App` is never constructed
  --> moxin-voice-shell/src/app.rs:68
// 原因: Makepad 宏系统会使用，编译器检测不到
```

### macOS 特定问题

#### ⚠️ Dora Dynamic Node 连接失败 (CRITICAL)

**问题**: 在 macOS 上点击 "Start Dataflow" 按钮时，dynamic 节点无法初始化

**错误信息**:

```
[ERROR moxin_tts::dora_integration] Failed to start dataflow:
moxin-asr-listener: Failed to init dora node
moxin-audio-input: Failed to initialize Dora node
```

**状态**: ✅ **已修复** (2026-02-14)

**根本原因**:

1. macOS 的 Unix domain socket 初始化比 Windows 慢
2. 原始 2-5 秒超时在 macOS 上不足够
3. Dynamic 节点（Rust 端创建）需要更多时间连接到 Dora daemon

**修复内容**:

1. **Dispatcher 初始化延迟**: 2s → 5s (macOS only)
   - 文件: `moxin-dora-bridge/src/dispatcher.rs:241-252`
2. **AudioInputBridge 连接超时**: 5s → 10s (macOS only)
   - 文件: `moxin-dora-bridge/src/widgets/audio_input.rs:130-146`
3. **AsrListenerBridge 连接超时**: 5s → 10s (macOS only)
   - 文件: `moxin-dora-bridge/src/widgets/asr_listener.rs:194-206`

**测试方法**:

```bash
# 1. 重新编译
cargo clean && cargo build -p moxin-voice --release

# 2. 运行测试脚本
./test_dora_fix.sh

# 3. 运行应用并测试
RUST_LOG=debug cargo run -p moxin-voice
# 点击 "Start Dataflow" 并观察日志
```

**成功标志**:

```
[INFO] Waiting for dataflow to initialize...
[INFO] Initialization delay completed (5s)  ← macOS 特有
[INFO] [AudioInputBridge] Connection verified in 1.2s
[INFO] [AsrListener] Connection verified after 1500 ms
[INFO] All bridges connected successfully
```

**详细文档**: `MACOS_DORA_FIX.md`

#### ⚠️ TTS 挂起 (CRITICAL - Apple Silicon)

**问题**: 在 Apple Silicon (M1/M2/M3/M4) 上，TTS 在推理时完全挂起（卡在 `self.tts.run(inputs)`）

**根本原因**: **BLAS 线程库冲突**

- PyTorch 编译时使用 Apple 的 **Accelerate framework** (`BLAS_INFO=accelerate`)
- 但 `tts.yml` 设置了 `OMP_NUM_THREADS` 和 `MKL_NUM_THREADS`
- 这些 OpenMP/MKL 线程设置与 Accelerate 的内部线程管理冲突
- 导致在 GPT-SoVITS 推理时发生**死锁**

**状态**: ✅ **已修复** (2026-02-14)

**修复内容** (`apps/moxin-voice/dataflow/tts.yml`):

1. **移除冲突的线程设置**:
   - 删除 `OMP_NUM_THREADS`
   - 删除 `MKL_NUM_THREADS`
   - 删除 `NUM_THREADS`
2. **使用正确的 Accelerate 控制**:
   - 添加 `VECLIB_MAXIMUM_THREADS: "1"`
3. **让系统自动检测**: 不强制线程数，让 Accelerate 优化

**技术细节**:

- 检查 PyTorch BLAS: `python -c "import torch; print(torch.__config__.show())" | grep BLAS`
- macOS 上应显示: `BLAS_INFO=accelerate`
- `VECLIB_MAXIMUM_THREADS` 是控制 Accelerate 的正确方式

**验证方法**:

```bash
# 运行测试脚本（60秒超时）
./test_tts_fix.sh

# 应该在几秒内完成，不再挂起
```

**重要**: 此问题仅影响使用 Accelerate 的 macOS PyTorch。Linux/Windows 使用 OpenBLAS/MKL 不受影响。

#### 其他 macOS 问题

详见:

- `MACOS_MPS_FIX.md` - TTS 性能修复指南（MPS 加速）(新增)
- `MACOS_DORA_FIX.md` - Dora 连接修复指南
- `MACOS_SETUP.md` - 完整设置指南
- `TROUBLESHOOTING_MACOS.md` - 故障排查
- `DORA_MACOS_ISSUE.md` - Dora 问题诊断

---

## 🧪 测试和验证

### 功能测试清单

#### TTS 核心功能

- [ ] 语音选择（14+ 预置语音）
- [ ] 文本输入和编辑
- [ ] TTS 生成（各种语音）
- [ ] 音频播放
- [ ] 音频导出（WAV 格式）

#### Express Mode (零样本克隆)

- [ ] 短音频录制（5-10 秒）
- [ ] 音频文件上传
- [ ] ASR 自动文本识别
- [ ] 克隆语音生成
- [ ] 保存自定义语音

#### Pro Mode (Few-Shot 训练)

- [ ] 长音频录制（3-10 分钟）
- [ ] 音频文件上传
- [ ] 训练进度显示
- [ ] 训练完成通知
- [ ] 训练模型保存和加载
- [ ] 使用训练语音生成 TTS

#### 性能和稳定性

- [ ] 长时间运行（>1 小时）
- [ ] 内存使用监控
- [ ] Dora dataflow 连接稳定性
- [ ] 错误处理和恢复

### 调试技巧

```bash
# 查看详细日志
RUST_LOG=debug cargo run -p moxin-voice

# 只看特定模块
RUST_LOG=moxin_tts=debug,moxin_tts=debug cargo run -p moxin-voice

# 保存日志
cargo run -p moxin-voice 2>&1 | tee moxin-voice.log

# 检查 Dora 节点状态
dora list

# 查看 Python 节点输出
# (通常在 Dora 日志中)
```

### 常见问题排查

#### 应用无法启动

1. 检查 Rust 版本：`rustc --version`
2. 检查 Python 环境：`conda activate moxin-studio`
3. 检查 Dora：`dora --version`
4. 查看日志输出

#### TTS 不生成音频

1. 检查 dora-primespeech 节点：`dora list`
2. 验证模型文件：`ls ~/.dora/models/primespeech/`
3. 查看 Python 节点日志
4. 检查 GPU/CPU 配置

#### ASR 无法识别

1. 检查 dora-asr 节点状态
2. 验证麦克风权限（macOS System Preferences）
3. 检查音频设备：`dora list`
4. 查看 ASR 模型：`ls ~/.dora/models/asr/funasr/`

#### Pro Mode 训练失败

1. 检查音频长度（需要 3-10 分钟）
2. 验证预训练模型存在：
   - GPT: `s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt`
   - SoVITS: `s2G2333k.pth`, `s2D2333k.pth`
3. 查看训练日志
4. 检查磁盘空间

---

## 📚 重要参考文档

### 项目核心文档

| 文档                            | 说明           | 何时查看                 |
| ------------------------------- | -------------- | ------------------------ |
| `README.md`                     | 项目主文档     | 了解功能、快速开始       |
| `doc/CONTEXT_RESUME.md`         | 详细上下文恢复 | 了解项目历史、已完成工作 |
| `MACOS_SETUP.md`                | macOS 设置指南 | macOS 环境设置           |
| `QUICKSTART_MACOS.md`           | macOS 快速开始 | 5 分钟快速设置           |
| `moxin-voice-shell/BUILDING.md` | 构建指南       | 详细构建说明             |

### 技术决策文档

| 文档                                 | 说明                          |
| ------------------------------------ | ----------------------------- |
| `doc/moxin-voice-fewshot决策分析.md` | Few-Shot 实施方案对比         |
| `doc/moxin-voice-fewshot疑问解答.md` | 技术疑问解答                  |
| `doc/TTS项目对比分析报告.md`         | Moxin.tts vs dora-primespeech |
| `doc/moxin-voice独立应用实施方案.md` | 独立应用设计方案              |

### 实现细节文档

| 文档                                         | 说明                   |
| -------------------------------------------- | ---------------------- |
| `doc/DEBUG_LOG.md`                           | Error 1-27 修复记录    |
| `doc/MOXIN_UI_IMPLEMENTATION.md`             | Moxin UI 实现细节      |
| `FEW_SHOT_UI_IMPLEMENTATION_GUIDE.md`        | Few-Shot UI 实施指南   |
| `VOICE_CLONE_MODAL_MODIFICATIONS_SUMMARY.md` | 语音克隆模态框修改总结 |

### 外部参考

| 项目         | 链接                                      | 说明       |
| ------------ | ----------------------------------------- | ---------- |
| GPT-SoVITS   | https://github.com/RVC-Boss/GPT-SoVITS    | TTS 引擎   |
| Makepad      | https://github.com/makepad/makepad        | UI 框架    |
| Dora         | https://github.com/dora-rs/dora           | 数据流框架 |
| Moxin Studio | https://github.com/moxin-org/moxin-studio | 上游项目   |

---

## 🎯 快速恢复检查清单

在新对话中开始工作前，请确认：

- [ ] 已阅读本文档（CLAUDE.md）
- [ ] 理解项目目标和背景
- [ ] 知道当前进度（Phase 1-4 完成，Phase 5 进行中）
- [ ] 了解项目结构和关键文件位置
- [ ] 环境配置检查：
  - [ ] 工作目录：`/Users/alan0x/Documents/projects/moxin-voice`
  - [ ] Git 分支：`main`
  - [ ] Rust 可用：`cargo --version`
  - [ ] Python 环境：`conda activate moxin-studio`
- [ ] 代码可编译：`cargo build -p moxin-voice`
- [ ] 如需详细历史，查阅 `doc/CONTEXT_RESUME.md`

### 常用命令速查

```bash
# 项目位置
cd /Users/alan0x/Documents/projects/moxin-voice

# 编译
cargo build -p moxin-voice                    # Debug
cargo build -p moxin-voice --release          # Release

# 运行
cargo run -p moxin-voice
cargo run -p moxin-voice -- --log-level debug # 带日志

# Git
git status
git log --oneline -10
git diff

# Dora
dora up                   # 启动守护进程
dora list                 # 查看运行状态
dora start apps/moxin-voice/dataflow/tts.yml  # 启动数据流
dora stop <id>            # 停止数据流
dora down                 # 停止守护进程

# Python 环境
conda activate moxin-studio
cd models/setup-local-models
python test_dependencies.py
```

---

## 💡 开发提示

### Makepad 开发要点

1. **live_design! 宏** - 定义 UI 布局和样式（类似 CSS in Rust）
2. **Widget 系统** - 组件化 UI，通过 WidgetRef 访问
3. **Event 驱动** - 使用 `MatchEvent` 处理用户交互
4. **GPU 加速** - 所有渲染都是 GPU 加速的

### Dora 集成要点

1. **SharedDoraState** - 在 Rust 和 Python 之间共享状态
2. **DataflowExecution** - 管理数据流生命周期
3. **Arrow IPC** - 节点间通信使用 Apache Arrow
4. **异步通信** - 所有节点通信都是异步的

### 代码风格

- **Rust**: 遵循 Rust 2021 edition 标准
- **注释**: 关键逻辑添加注释，解释"为什么"而非"是什么"
- **错误处理**: 使用 `Result<T, E>` 和 `?` 操作符
- **日志**: 使用 `log::info!`, `log::debug!`, `log::error!` 等宏

### Git 提交规范

```
feat: 新功能
fix: Bug 修复
docs: 文档更新
refactor: 代码重构
test: 测试相关
chore: 构建/工具相关
```

示例:

```bash
git commit -m "feat: add voice cloning progress indicator"
git commit -m "fix: resolve Pro Mode training audio blank issue"
git commit -m "docs: update CLAUDE.md with current status"
```

---

## 🔗 相关链接

- **GitHub 仓库**: https://github.com/alan0x/moxin-voice
- **Issues**: https://github.com/alan0x/moxin-voice/issues
- **上游项目**: https://github.com/moxin-org/moxin-studio
- **开发者**: alan0x

---

**版本历史**:

- v1.0 (2026-02-14) - 初始版本，项目概览和上下文

**下次更新建议**:

- 添加 API 参考
- 添加架构图
- 添加性能优化指南
- 添加发布流程

---

_由 Claude Sonnet 4.5 创建，用于快速上下文恢复_
