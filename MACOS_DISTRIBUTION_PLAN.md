# macOS 分发方案与排障记录（MLX 分支）

> 适用分支：`codex-fix-tts-stutter-remote`

## 1. 当前分发基线

当前 app 为混合架构，不是纯 Rust 单二进制：

- TTS 推理：`moxin-tts-node`（Rust/MLX）
- ASR：`dora-asr`（Python）
- few-shot 训练：
  - Option A（默认）：Python
  - Option B（实验）：Rust/MLX

因此分发必须覆盖：

1. Python 运行时与节点依赖
2. 模型下载/初始化
3. Dora 运行环境

## 2. 分发目标

- 产出可安装的 `.app` / `.dmg`
- 首次启动可自动自检并提示初始化
- 默认功能可用：内置音色 TTS + ASR 回填 + Option A few-shot

## 3. 已落地内容（当前状态）

### 3.1 打包与安装产物

- `scripts/build_macos_app.sh`：构建 `Moxin Voice Dev.app`
- `scripts/build_macos_dmg.sh`：构建 `Moxin Voice Dev.dmg`
- app 启动器已内置：
  - preflight（快速检查）
  - bootstrap（缺依赖时初始化）
  - Dora 启动检查与失败提示

### 3.2 app 运行时关键机制

- 使用 `MOXIN_DATAFLOW_PATH` 指向 dataflow
- 使用 `scripts/macos_run_dora_asr.sh` 启动 ASR（避免 PATH 继承问题）
- few-shot trainer 路径可由 `MOXIN_FEWSHOT_TRAINER_BIN` 指定

## 4. 本次真实排障过程（重点）

### 4.1 症状 A：启动卡住 / Dock 一直跳

现象：app 启动长时间无响应。

根因：启动阶段 `dora up` 可能阻塞，且失败信息不可见。

处理：

- 启动器增加 Dora 健康检查与超时逻辑
- 启动失败时弹窗，并写日志到 `~/Library/Logs/MoxinVoice/dora_up.log`

### 4.2 症状 B：`Read-only file system`（`out` 目录创建失败）

现象日志：

- `failed to create out directory`
- `Read-only file system (os error 30)`

根因：Dora/dataflow 工作目录落在 app 只读路径（`/Applications/.../Resources/dataflow`）。

处理：

- Dora 运行目录固定到 `~/.dora/runtime`
- 启动时在可写目录生成 runtime dataflow（`~/.dora/runtime/dataflow/tts.yml`）
- runtime dataflow 使用绝对路径指向：
  - `/Applications/.../moxin-tts-node`
  - `/Applications/.../macos_run_dora_asr.sh`

验证点：

- `dora-coordinator` 日志中的 `local_working_dir` 应为 `~/.dora/runtime/dataflow`

### 4.3 症状 C：可启动但 TTS 一直“生成中”

现象：dataflow 显示 Running，但无音频返回。

根因：`moxin-tts-node` 崩溃，日志报：

- `MLX error: Failed to load the default metallib`

处理：

- 打包时将 `target/release/mlx.metallib` 一并拷贝到：
  - `Moxin Voice Dev.app/Contents/MacOS/mlx.metallib`

结果：TTS 恢复可生成。

## 5. 运行时日志位置（排障入口）

- 启动器日志：`~/Library/Logs/MoxinVoice/`
  - `dora_up.log`
  - `preflight.log`
  - `bootstrap.log`
- Dora 日志：`~/.dora/runtime/out/`
- dataflow 节点日志：`~/.dora/runtime/dataflow/out/<dataflow-id>/`
  - 例如：`log_primespeech-tts.txt`、`log_asr.txt`

## 6. 修改应用名称与图标

### 6.1 修改名称（Dock / Finder / 顶部菜单）

使用：

```bash
bash scripts/build_macos_app.sh --app-name "Moxin Voice Dev"
```

说明：`--app-name` 会写入 `Info.plist` 的 `CFBundleName` / `CFBundleDisplayName`。

### 6.2 修改图标

使用：

```bash
bash scripts/build_macos_app.sh --app-name "Moxin Voice Dev" --icon "/abs/path/icon.png"
```

当前实现说明：

- 传 `.icns`：直接拷贝到 `Contents/Resources/AppIcon.icns`
- 传 `.png`：直接写入 `Contents/Resources/AppIcon.png` 并在 `Info.plist` 引用
- 由于当前环境 `iconutil` 不稳定，PNG 路径使用“直接资源”策略

图标显示建议：

- 优先使用正方形素材（推荐 1024x1024）
- 若原图是横图/竖图，先做居中裁切再打包

### 6.3 覆盖安装与刷新

```bash
rm -rf "/Applications/Moxin Voice Dev.app"
cp -R "dist/Moxin Voice Dev.app" "/Applications/"
killall Dock
```

## 7. 其他开发者拿到提交后需要做什么

1. 拉取代码并重新构建 app：

```bash
bash scripts/build_macos_app.sh --app-name "Moxin Voice Dev"
```

2. 首次启动如果提示初始化，执行 bootstrap。

3. 核验关键点：

- `dora list` 可看到 dataflow Running
- `~/.dora/runtime/dataflow/tts.yml` 存在
- `~/.dora/runtime/dataflow/out/.../log_primespeech-tts.txt` 无 metallib 错误

## 8. 后续建议

- 增加一个 “一键收集诊断包” 脚本（汇总上述日志路径）
- 增加发布前 smoke test：
  - 启动 app
  - 发送固定文本
  - 验证 `log_primespeech-tts.txt` 出现 `TTS request`

## 9. 单机分发测试方法（强烈建议）

### 9.1 推荐：新建“干净用户”验证

步骤：

1. 系统设置 -> 用户与群组 -> 添加用户（标准用户即可）
2. 退出当前账号，登录新用户测试

意义：

- 这基本等价于一台新机器
- 不会被你现有 `~/.dora` / `~/.OminiX` / conda 状态污染

在新用户下执行安装测试：

1. 双击 `Moxin Voice Dev.dmg`，拖到 `Applications`
2. 启动 App，走首启初始化
3. 记录是否出现阻断、错误文案是否可理解

重点验证项：

- 初始化是否自动安装依赖
- 模型是否自动下载并转换
- App 能否正常出声（内置音色）
- ASR 是否可回填
- few-shot Option A 能否启动训练

### 9.2 次优方案：临时隔离 HOME

若不想新建系统用户，可在当前用户下隔离 HOME 做一轮验证：

```bash
mkdir -p /tmp/moxin-clean-home
HOME=/tmp/moxin-clean-home open "/Applications/Moxin Voice Dev.app"
```

说明：

- 这会把 `~/.dora`、`~/.OminiX`、conda 相关状态隔离到临时目录
- 缺点：GUI app 对 `HOME` 继承不如“新用户”方式稳定
