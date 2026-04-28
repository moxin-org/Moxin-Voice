# macOS 设置检查清单

使用此清单确保所有步骤都已完成。

## 📋 前置要求

- [ ] macOS 系统（已确认 ✅）
- [ ] Homebrew 已安装（已确认 ✅）
- [ ] Conda 已安装（已确认 ✅）
- [ ] Rust 已安装（已确认 ✅）
- [ ] Git 已安装（已确认 ✅）
- [ ] Xcode Command Line Tools 已安装（已确认 ✅）

## 🔧 系统依赖

手动安装以下依赖，或在需要兼容旧流程时运行 `scripts/install_macos_deps.deprecated.sh`：

- [ ] portaudio (`brew install portaudio`)
- [ ] ffmpeg (`brew install ffmpeg`)
- [ ] git-lfs (`brew install git-lfs`)
- [ ] openblas (`brew install openblas`)
- [ ] libomp (`brew install libomp`)

验证：

```bash
cd models/setup-local-models
./check_macos_deps.sh
```

## 🐍 Python 环境

- [ ] 创建 moxin-studio 环境

  ```bash
  cd models/setup-local-models
  ./setup_isolated_env.sh
  ```

- [ ] 激活环境

  ```bash
  conda activate moxin-studio
  ```

- [ ] 安装 Python 包

  ```bash
  ./install_all_packages.sh
  ```

- [ ] 验证安装
  ```bash
  python test_dependencies.py
  ```

## 📦 模型下载

- [ ] ASR 模型（语音识别）

  ```bash
  cd ../model-manager
  python download_models.py --download funasr
  ```

- [ ] TTS 模型（语音合成）

  ```bash
  python download_models.py --download primespeech
  ```

- [ ] 查看可用语音（可选）

  ```bash
  python download_models.py --list-voices
  ```

- [ ] 下载特定语音（可选）
  ```bash
  python download_models.py --voice "Voice Name"
  ```

## 🦀 Rust 组件

- [ ] 构建应用

  ```bash
  cd ../..
  cargo build -p moxin-voice-shell
  ```

- [ ] 运行应用

  ```bash
  cargo run -p moxin-voice-shell
  ```

## 🧪 测试验证

- [ ] Python 依赖测试

  ```bash
  conda activate moxin-studio
  cd models/setup-local-models
  python test_dependencies.py
  ```

- [ ] Dora CLI 测试

  ```bash
  dora --version
  ```

- [ ] 应用启动测试
  ```bash
  cargo run -p moxin-voice-shell
  ```

## ⚡ 性能优化（Apple Silicon）

- [ ] 确认 Python 是 ARM64 原生

  ```bash
  python -c "import platform; print(platform.machine())"
  # 应该输出: arm64
  ```

- [ ] 确认 MLX 已安装（GPU 加速）

  ```bash
  python -c "import mlx; print('MLX available')"
  ```

- [ ] 设置 TTS 后端（可选）
  ```bash
  export BACKEND=mlx    # GPU 加速
  export BACKEND=cpu    # CPU
  export BACKEND=auto   # 自动（默认）
  ```

## 📚 文档阅读

- [ ] [QUICKSTART_MACOS.md](./QUICKSTART_MACOS.md) - 快速开始
- [ ] [MACOS_SETUP.md](./MACOS_SETUP.md) - 完整设置指南
- [ ] [README.md](../../README.md) - 项目文档

## 🎯 快捷方式

### 一键设置（推荐）

如果你还没有开始，可以使用：

```bash
# 安装系统依赖
./scripts/install_macos_deps.deprecated.sh

# 一键完成所有设置
cd models/setup-local-models
./quick_setup_macos.sh
```

### 依赖检查

随时运行以检查状态：

```bash
cd models/setup-local-models
./check_macos_deps.sh
```

## ✅ 完成标志

当你完成所有步骤后，应该能够：

- [ ] 成功运行 `./check_macos_deps.sh` 无错误
- [ ] 成功运行 `python test_dependencies.py` 无错误
- [ ] 成功启动应用 `cargo run -p moxin-voice-shell`
- [ ] 看到应用窗口并能使用 TTS 功能

## 🐛 遇到问题？

如果任何步骤失败：

1. 查看 [TROUBLESHOOTING_MACOS.md](TROUBLESHOOTING_MACOS.md)
2. 运行 `./check_macos_deps.sh` 诊断
3. 查看错误日志
4. 搜索或提交 GitHub Issue

## 📊 预计时间

- 系统依赖安装: 1-2 分钟
- Python 环境设置: 3-5 分钟
- Python 包安装: 2-3 分钟
- 模型下载: 10-30 分钟（取决于网络）
- Rust 编译: 5-10 分钟

**总计**: 20-50 分钟

## 🎉 完成！

恭喜！你已经成功设置了 Moxin Voice。

下一步：

- 探索不同的 UI 风格
- 尝试语音克隆功能
- 下载更多语音模型
- 阅读项目文档

享受使用 Moxin TTS！🚀

---

**提示**: 保存此清单，以便将来重新安装或在其他 Mac 上设置时使用。
