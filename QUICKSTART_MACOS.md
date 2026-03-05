# macOS 快速开始指南

> 5 分钟快速设置 Moxin TTS

## 当前状态

根据检查，你的系统：

- ✅ Homebrew 已安装
- ✅ Conda 已安装（moxin-studio 环境已存在）
- ✅ Rust 已安装
- ✅ Git 已安装
- ✅ Python 是 ARM64 原生（最佳性能）
- ❌ 缺少 5 个 Homebrew 包

## 立即开始

### 1. 安装缺失的依赖（1 分钟）

```bash
./install_macos_deps.sh
```

或手动安装：

```bash
brew install portaudio ffmpeg git-lfs openblas libomp
```

### 2. 设置环境（3-5 分钟）

```bash
cd models/setup-local-models
./setup_isolated_env.sh
```

### 3. 安装包（2-3 分钟）

```bash
conda activate moxin-studio
./install_all_packages.sh
```

### 4. 验证安装

```bash
# 快速验证
./verify_setup.sh

# 或手动验证
conda activate moxin-studio
python test_dependencies.py
```

### 5. 下载模型（10-30 分钟）

```bash
cd ../model-manager

# ASR 模型（语音识别）
python download_models.py --download funasr

# TTS 模型（文本转语音）
python download_models.py --download primespeech
```

### 5.5. 下载 Pro Mode 预训练模型（可选，但强烈推荐）

如果计划使用 Pro Mode (Few-Shot) 训练自定义语音，**必须**下载预训练模型。不使用预训练模型的训练会产生噪音/空白音频。

```bash
# 创建目录
mkdir -p ~/.dora/models/primespeech/moyoyo/gsv-v2final-pretrained
cd ~/.dora/models/primespeech/moyoyo/gsv-v2final-pretrained

# 下载三个预训练文件 (总计约 410 MB)
curl -L -o s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s1bert25hz-5kh-longer-epoch%3D12-step%3D369668.ckpt

curl -L -o s2G2333k.pth \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s2G2333k.pth

curl -L -o s2D2333k.pth \
  https://huggingface.co/lj1995/GPT-SoVITS/resolve/main/gsv-v2final-pretrained/s2D2333k.pth
```

### 6. 运行应用

```bash
cd ../../..
cargo run -p moxin-voice
```

## 一键设置（推荐）

如果你想自动完成所有步骤：

```bash
cd models/setup-local-models
./quick_setup_macos.sh
```

## Pro Mode (Few-Shot) 使用须知

### 训练依赖

最新版本的 `install_all_packages.sh` 和 `setup_isolated_env.sh` 已包含所有必需的训练依赖：

```bash
# 如果手动安装，需要以下包：
pip install "datasets<3.0.0" simplejson sortedcontainers tensorboard matplotlib
```

### 预训练模型（必需）

**重要**: 使用 Pro Mode 训练前，必须下载 GPT-SoVITS 预训练模型（见上文步骤 5.5）。不使用预训练模型会导致：

- 训练后的音色产生噪音或空白音频
- 无法生成可用的语音

### MPS 加速（Apple Silicon）

- **GPT 训练**: ✅ 自动使用 MPS (GPU) 加速
- **SoVITS 训练**: ⚠️ 使用 CPU（MPS 不支持复数梯度）

预计训练时间：

- 3 分钟音频：约 1-2 小时（取决于 CPU 性能）
- 10 分钟音频：约 3-5 小时

## 遇到问题？

- 查看 [TROUBLESHOOTING_MACOS.md](TROUBLESHOOTING_MACOS.md)
- 运行 `./check_macos_deps.sh` 诊断问题

## 下一步

- 下载更多语音：`python download_models.py --list-voices`
- 阅读完整文档：[MACOS_SETUP.md](MACOS_SETUP.md)

---

**预计总时间**: 15-40 分钟（取决于网络速度）
