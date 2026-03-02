# 安装总结

## 安装日期
2026-03-02

## 安装状态
✅ 核心组件安装成功

## 已安装组件

### Python 包
- ✅ dora-common (共享库)
- ✅ dora-primespeech (TTS 引擎)
- ✅ dora-speechmonitor
- ✅ dora-text-segmenter
- ✅ Pro Mode 训练依赖 (datasets, matplotlib, tensorboard 等)

### Rust 组件
- ✅ Dora CLI 0.3.12
- ✅ Rust 1.93.1

### 核心依赖
- ✅ PyTorch 2.2.2
- ✅ NumPy 1.26.4
- ✅ Transformers 4.49.0
- ✅ Librosa 0.11.0

## 跳过的组件

### 由于依赖问题跳过
- ⏭️ dora-maas-client (outfox-openai 版本冲突)
- ⏭️ dora-conference-bridge (可选)
- ⏭️ dora-conference-controller (可选)

### 由于网络问题跳过
- ⏭️ dora-asr (pywhispercpp 克隆超时)

**注意**: 这些组件不是 TTS 核心功能所必需的。

## 环境配置

### Shell 配置 (~/.zshrc)
```bash
# Cargo (Rust) bin path
export PATH="$HOME/.cargo/bin:$PATH"

# Claude Code alias
alias claude='/Users/mac/Library/pnpm/claude --dangerously-skip-permissions'
```

### Conda 环境
- 环境名称: mofa-studio
- Python 版本: 3.12.12

## 下一步

1. **下载模型**:
   ```bash
   cd ../model-manager
   python download_models.py --download primespeech
   python download_models.py --list-voices
   ```

2. **测试 TTS**:
   ```bash
   cd /Users/mac/workspaces/Moxin-Voice
   cargo run -p moxin-tts
   ```

3. **启动 Dora**:
   ```bash
   dora up
   dora start apps/mofa-tts/dataflow/tts.yml
   ```

## 已知问题

1. **网络连接**: GitHub 访问超时（影响 pywhispercpp 克隆）
   - 解决方案: 配置代理或手动克隆

2. **可选依赖缺失**:
   - accelerate
   - pyaudio
   - webrtcvad
   - openai
   - websockets
   
   这些不影响核心 TTS 功能。

## 验证命令

```bash
# 激活环境
conda activate mofa-studio

# 验证 Python 包
python -c "import dora_primespeech; print('✓ dora-primespeech')"
python -c "import torch; print(f'✓ PyTorch {torch.__version__}')"

# 验证 Dora CLI
dora --version

# 验证 Rust
cargo --version
```

## 参考文档
- CLAUDE.md - 项目上下文
- MACOS_SETUP.md - macOS 详细设置
- QUICKSTART_MACOS.md - 快速开始指南
