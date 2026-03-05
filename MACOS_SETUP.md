# macOS 设置指南

本指南专门针对 macOS 用户运行 Moxin TTS 项目。

## 前置要求

### 1. 安装 Homebrew

如果还没有安装 Homebrew，运行：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

安装完成后，根据提示将 Homebrew 添加到 PATH（通常是添加到 `~/.zprofile`）。

### 2. 安装系统依赖

```bash
brew install portaudio ffmpeg git-lfs openblas libomp
```

这些包的作用：

- `portaudio`: 音频输入/输出库（pyaudio 需要）
- `ffmpeg`: 音频/视频处理
- `git-lfs`: 大文件支持
- `openblas`: 线性代数库
- `libomp`: OpenMP 支持

### 3. 安装 Conda

推荐使用 Miniconda（轻量级）：

```bash
# 下载 Miniconda 安装器（Apple Silicon）
curl -O https://repo.anaconda.com/miniconda/Miniconda3-latest-MacOSX-arm64.sh

# 或者 Intel Mac
curl -O https://repo.anaconda.com/miniconda/Miniconda3-latest-MacOSX-x86_64.sh

# 运行安装器
bash Miniconda3-latest-MacOSX-*.sh

# 重启终端或运行
source ~/.zshrc
```

### 4. 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## 快速开始

### 方式 1: 一键自动设置（推荐）

```bash
cd models/setup-local-models
./quick_setup_macos.sh
```

这个脚本会自动：

- 检查所有依赖
- 创建 conda 环境
- 安装所有包
- 验证安装
- 可选：下载模型

### 方式 2: 手动分步设置

### 步骤 1: 环境设置

```bash
cd models/setup-local-models
./setup_isolated_env.sh
```

这个脚本会：

- 自动检测并安装 macOS 所需的 Homebrew 包
- 创建 `moxin-studio` conda 环境（Python 3.12）
- 安装所有 Python 依赖
- 安装 Dora CLI

### 步骤 2: 安装所有包

```bash
conda activate moxin-studio
./install_all_packages.sh
```

这个脚本会：

- 验证系统依赖已安装
- 安装所有 Python 节点（editable 模式）
- 构建 Rust 组件

验证安装：

```bash
python test_dependencies.py
```

### 步骤 3: 下载模型

```bash
cd ../model-manager

# 下载 ASR 模型
python download_models.py --download funasr

# 下载 PrimeSpeech TTS 模型
python download_models.py --download primespeech

# 查看可用语音
python download_models.py --list-voices

# 下载特定语音（可选）
python download_models.py --voice "Luo Xiang"
```

### 步骤 3.5: 下载 Pro Mode 预训练模型（可选，但强烈推荐）

如果计划使用 Pro Mode (Few-Shot) 训练自定义语音，必须下载 GPT-SoVITS 预训练基础模型。**不使用预训练模型的训练会产生噪音/空白音频**。

```bash
# 创建预训练模型目录
mkdir -p ~/.dora/models/primespeech/moyoyo/gsv-v2final-pretrained

cd ~/.dora/models/primespeech/moyoyo/gsv-v2final-pretrained

# 下载 GPT 预训练模型 (155 MB)
curl -L -o s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s1bert25hz-5kh-longer-epoch%3D12-step%3D369668.ckpt

# 下载 SoVITS 生成器模型 (~170 MB)
curl -L -o s2G2333k.pth \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s2G2333k.pth

# 下载 SoVITS 判别器模型 (~85 MB)
curl -L -o s2D2333k.pth \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s2D2333k.pth
```

验证下载：

```bash
ls -lh ~/.dora/models/primespeech/moyoyo/gsv-v2final-pretrained/
# 应该看到三个文件：
# s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt (155 MB)
# s2G2333k.pth (~170 MB)
# s2D2333k.pth (~85 MB)
```

### 步骤 4: 运行应用

返回项目根目录：

```bash
cd ../..

cargo run -p moxin-voice
```

## macOS TTS 已知问题及修复

### TTS 挂起/卡住不动（Apple Silicon）

在 macOS Apple Silicon 上，TTS 可能会在推理时完全挂起。这通常由以下多个因素共同导致：

**问题 1: G2PW 模型文件缺失**

G2PW（中文多音字标注模型）的 `char_bopomofo_dict.json` 文件可能未正确部署到模型目录。

检查方法：

```bash
ls ~/.dora/models/primespeech/G2PWModel/char_bopomofo_dict.json
```

如果文件缺失，从源码复制：

```bash
cp node-hub/dora-primespeech/dora_primespeech/moyoyo_tts/text/G2PWModel/char_bopomofo_dict.json \
   ~/.dora/models/primespeech/G2PWModel/
```

**问题 2: torch.jit.script 在 ARM64 上的兼容性**

GPT-SoVITS 的 AR 模型使用了 `@torch.jit.script` 装饰器，在 macOS ARM64 上可能导致 JIT 编译挂起。

已在 `moyoyo_tts/AR/models/t2s_model.py` 中禁用了以下三个类的 JIT 编译：

- `T2SMLP`
- `T2SBlock`
- `T2STransformer`

同时将 `torch_sdpa` 默认值改为 `False`，使用手动注意力实现代替 `F.scaled_dot_product_attention`。

**问题 3: BLAS 线程冲突**

PyTorch 在 macOS 上使用 Apple Accelerate framework（`BLAS_INFO=accelerate`），与 OpenMP/MKL 线程设置冲突。

`tts.yml` 中不应设置 `OMP_NUM_THREADS` 或 `MKL_NUM_THREADS`，改为使用：

```yaml
VECLIB_MAXIMUM_THREADS: "1" # Accelerate framework 专用线程控制
```

**问题 4: SDPA 注意力机制死锁**

HuBERT 和 BERT 模型默认使用 Scaled Dot-Product Attention (SDPA)，在 macOS 上会与 Accelerate 线程管理冲突导致死锁。

已在代码中修复：加载 HuBERT 和 BERT 模型时使用 `attn_implementation="eager"` 参数，强制使用非 SDPA 注意力实现。

**问题 5: PyTorch 多线程死锁**

PyTorch 默认使用多线程进行 CPU 推理，与 Apple Accelerate BLAS 库冲突。

已在代码中修复：

- 在 macOS 上自动调用 `torch.set_num_threads(1)` 限制 PyTorch 线程数
- 设置 `OPENBLAS_NUM_THREADS=1` 环境变量

**问题 6: 参考音频过长导致硬错误**

原始代码对超过 10 秒的参考音频直接报错。已修改为自动截断到 10 秒并发出警告，而非终止处理。

**问题 7: Dora 日志阻塞导致推理挂起**

在模型重新加载后（如切换到 Express Mode 自定义语音），Dora 节点的 `send_log()` 方法可能永久阻塞，导致 TTS 生成卡住。

已在代码中修复：

- `moyoyo_tts_wrapper_streaming_fix.py` 的 `log()` 方法优先使用 `sys.stderr`
- 所有 Dora 日志调用包裹在 try/except 中，避免因阻塞导致整个推理流程卡死

**问题 8: Pro Mode 训练依赖冲突**

Pro Mode (Few-Shot) 训练需要 `modelscope` 进行音频降噪，而 `modelscope 1.34.0` 与 `datasets>=3.0.0` 不兼容。

解决方案：安装时固定 `datasets<3.0.0`，并安装 modelscope 和训练流程的所有隐式依赖。

```bash
pip install "datasets<3.0.0" simplejson sortedcontainers tensorboard matplotlib
```

**问题 9: Pro Mode 训练 MPS 加速限制**

- **GPT 模型训练**：✅ 支持 MPS 加速（Apple Silicon GPU）
- **SoVITS 模型训练**：❌ 不支持 MPS，使用 CPU

原因：SoVITS 训练使用 `torch.stft` 进行频谱图计算，产生复数梯度（ComplexFloat）。PyTorch MPS 后端不支持复数类型的反向传播。

影响：SoVITS 训练阶段（15 epochs，约 30-90 分钟）会比较慢，但 GPT 训练阶段可以享受 MPS 加速。

**问题 10: Pro Mode 训练缺少预训练模型产生噪音**

如果不使用预训练基础模型，从零开始训练的模型只会产生噪音或空白音频。**必须下载 GPT-SoVITS 预训练模型**。

解决方案：参见上文"步骤 3.5: 下载 Pro Mode 预训练模型"部分，下载三个预训练文件：

- `s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt` (GPT)
- `s2G2333k.pth` (SoVITS 生成器)
- `s2D2333k.pth` (SoVITS 判别器)

**重要提示**: 修改 Python 代码后，必须重启 Dora 进程才能生效：

```bash
dora destroy && dora up
```

### 诊断工具

如果 TTS 仍有问题，可以使用诊断脚本逐步测试 TTS 管道的每个环节：

```bash
conda activate moxin-studio
python diagnose_tts.py
```

该脚本会测试：PyTorch 基础操作、模型加载、参考音频处理、BERT 特征提取、AR 模型推理、VITS 语音合成，每步都有独立超时控制。

## 常见问题

### Q: pyaudio 安装失败，提示找不到 portaudio.h

**A:** 确保已安装 portaudio：

```bash
brew install portaudio
```

如果已安装但仍然失败，可能需要设置环境变量：

```bash
export CFLAGS="-I$(brew --prefix portaudio)/include"
export LDFLAGS="-L$(brew --prefix portaudio)/lib"
pip install pyaudio
```

### Q: 编译 Rust 代码时出错

**A:** 确保已安装 Xcode Command Line Tools：

```bash
xcode-select --install
```

### Q: conda 命令找不到

**A:** 重启终端或手动激活：

```bash
source ~/miniconda3/bin/activate
# 或
source ~/anaconda3/bin/activate
```

### Q: MLX 后端是什么？

**A:** MLX 是 Apple 为 Apple Silicon 优化的机器学习框架。目前 MLX 后端仅在 `dora-kokoro-tts`（Kokoro TTS 引擎）中实现，`dora-primespeech`（GPT-SoVITS 引擎）尚不支持 MLX。

如需使用 MLX 加速，可以切换到 Kokoro TTS 引擎。

### Q: 如何切换 TTS 后端？

**A:** `BACKEND` 环境变量仅适用于 `dora-kokoro-tts` 节点：

```bash
# 使用 CPU 后端
export BACKEND=cpu

# 使用 MLX 后端（仅 Apple Silicon，需安装 mlx-audio）
export BACKEND=mlx

# 自动检测（默认）
export BACKEND=auto
```

注意：默认的 `dora-primespeech` 节点不使用此变量，始终使用 PyTorch CPU 模式。

## 性能优化

### Apple Silicon (M1/M2/M3/M4)

- 确保使用 ARM64 原生 Python（不是 Rosetta）
- 检查 Python 架构：`python -c "import platform; print(platform.machine())"`
  - 应该显示 `arm64`，而不是 `x86_64`
- 确认 PyTorch 使用 Accelerate framework：
  ```bash
  python -c "import torch; print(torch.__config__.show())" | grep BLAS
  # 应显示: BLAS_INFO=accelerate
  ```
- 不要设置 `OMP_NUM_THREADS` 或 `MKL_NUM_THREADS`（会与 Accelerate 冲突）

### Intel Mac

- 使用 CPU 后端
- 考虑使用 Docker 以获得更好的隔离性

## 目录结构

模型存储位置：

```
~/.dora/models/
├── asr/
│   └── funasr/                    # FunASR ASR 模型
└── primespeech/
    ├── G2PWModel/                 # G2PW 多音字标注模型（必须包含 char_bopomofo_dict.json）
    └── moyoyo/                    # GPT-SoVITS TTS 模型
        ├── GPT_weights/           # GPT 模型权重
        ├── SoVITS_weights/        # SoVITS 模型权重
        ├── ref_audios/            # 参考音频文件
        ├── chinese-hubert-base/   # HuBERT 特征提取模型
        └── chinese-roberta-wwm-ext-large/  # BERT 文本特征模型
```

## 下一步

- 查看 [README.md](README.md) 了解项目架构
- 查看 [doc/MOXIN_UI_IMPLEMENTATION.md](doc/MOXIN_UI_IMPLEMENTATION.md) 了解 UI 设计
- 查看 [CONTRIBUTING.md](CONTRIBUTING.md) 了解如何贡献

## 获取帮助

如果遇到问题：

1. 运行依赖检查：`./check_macos_deps.sh`
2. 查看 [TROUBLESHOOTING_MACOS.md](TROUBLESHOOTING_MACOS.md) 故障排除指南
3. 检查 [Issues](https://github.com/alan0x/moxin-voice/issues)
4. 运行 `python test_dependencies.py` 检查依赖
5. 查看 Dora 日志：`dora logs <dataflow-id>`
6. 提交新 Issue 并附上错误信息

---

**提示**: 首次运行可能需要下载大量模型文件，请确保网络连接稳定。
