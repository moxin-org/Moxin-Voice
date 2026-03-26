---
name: release-preflight-check
description: Use before running build_macos_app.sh to package a DMG — verifies workspace members, build steps, bundle contents, YAML node paths, model downloads, preflight checks, and launcher env vars are all consistent.
---

# Skill: 发包前分发脚本检查

## 执行原则

逐层与**实际代码/文件**核对，不凭记忆或文档推断。

---

## Layer 1：Workspace 成员完整性

读 `Cargo.toml` → `[workspace] members`

- 每个 `members` 条目的目录存在
- 含 `[[bin]]` 的 crate：是否需要打包？
- `build_macos_app.sh` 中 `run_cargo_build -p <name>` 与需打包的 crate 一一对应

---

## Layer 2：Build 步骤覆盖

对每条 `run_cargo_build` 行逐项检查：

| 检查项 | 方法 |
|-------|------|
| `-p <crate>` 在 workspace members 中 | 读 `Cargo.toml` |
| `--profile` 与 `$PROFILE` 一致 | 目视 |
| 有对应 `XXX_BIN_PATH=.../$PROFILE/<binary>` | 目视 |
| 有 `[[ ! -f "$XXX_BIN_PATH" ]]` 存在性检查 | 目视 |
| 有 `cp "$XXX_BIN_PATH" "$MACOS_DIR/<name>"` | 目视 |
| 有 `chmod +x "$MACOS_DIR/<name>"` | 目视 |

---

## Layer 3：Bundle 内容完整性

列出所有 `cp ... "$MACOS_DIR/..."` 行，逐文件核对：

| 文件类型 | 验证方法 |
|---------|---------|
| Rust binary | Layer 2 中已 build 并复制 |
| Shell 脚本 | 源文件在 repo 中存在 |
| 数据流 YAML | 源文件存在；节点路径在 bundle 内可达 |
| 资源文件 | 源目录在 repo 中存在 |

额外确认：bundle 中是否有从未被引用的幽灵文件？

---

## Layer 4：数据流 YAML 一致性

读所有将被打包的 `.yml` 文件，对每个非 `dynamic` 节点：

| 检查项 | 方法 |
|-------|------|
| `path` 指向的 binary 在 bundle 中存在 | 与 Layer 3 列表对照 |
| 节点 `id` 与 dynamic 节点的 `inputs` 引用一致 | 读 YAML |
| 无未替换占位符（`__XXX__` 形式） | `grep "__"` |
| `env` 中路径变量与 launcher 中 `export` 变量名一致 | 对照 launcher |

```bash
grep -r "__" scripts/dataflow/ scripts/build_macos_app.sh
```

---

## Layer 5：moxin-init 模型下载完整性

读 `moxin-init/src/main.rs`，对每个 `download_repo(...)` 调用：

| 检查项 | 方法 |
|-------|------|
| 运行时节点确实需要该模型 | 读对应节点源码 |
| `model_ready_xxx()` 文件列表与节点实际需要一致 | 对比节点源码 |
| 模型路径由环境变量控制，有合理默认值 | 读 `resolve_config()` |
| 该变量在 `macos_bootstrap.sh` 的 `exec env` 中传递 | 读 bootstrap |
| 该变量在 launcher 中被 `export` | 读 launcher |

---

## Layer 6：Preflight 检查完整性

读 `scripts/macos_preflight.sh`：

| 检查项 | 期望 |
|-------|------|
| 每个 bundle binary（Layer 3）有对应存在性检查 | `errors+=` 或 resolve 函数 |
| 每个 moxin-init 下载的模型有就绪检查 | 必须项 → `errors+=`；可选项 → `warnings+=` |
| preflight 的 `model_ready()` 文件列表与 moxin-init 一致 | 两处同步 |
| 无 conda/Python 相关检查 | `grep "conda\|python\|pip"` 无输出 |

---

## Layer 7：Launcher 脚本一致性

读 `build_macos_app.sh` 中 `cat > "$MACOS_DIR/$BIN_NAME" <<'EOF' ... EOF` 段：

| 检查项 | 方法 |
|-------|------|
| `export PATH=` 包含 `$MACOS_DIR` | 目视 |
| 所有 bootstrap/preflight/runtime 所需环境变量均被 `export` | 与 Layer 5 对照 |
| `MOXIN_APP_RESOURCES` 指向 `$RES_DIR` | 目视 |
| `MOXIN_DATAFLOW_PATH` 正确设置或由 preflight 自动推断 | 目视 |
| 内嵌 YAML 与 `scripts/dataflow/*.bundle.yml` 节点 id/path/env 一致 | 逐字段对比 |

---

## 最终签核

以下全部通过后执行打包：

- [ ] Layer 1：workspace members 与 build 步骤对齐
- [ ] Layer 2：每个 binary 有完整 build → 检查 → cp → chmod 链
- [ ] Layer 3：bundle 内容无遗漏、无幽灵文件
- [ ] Layer 4：YAML 节点路径可达，无占位符，id 引用自洽
- [ ] Layer 5：moxin-init 覆盖所有运行时所需模型，model_ready 与节点一致
- [ ] Layer 6：preflight 覆盖所有 binary 和模型，严重程度分级正确
- [ ] Layer 7：launcher 环境变量完整，内嵌 YAML 与 bundle YAML 一致
- [ ] `bash -n` 语法检查三个 shell 脚本无报错
- [ ] `grep "__" scripts/dataflow/ scripts/build_macos_app.sh` 无输出

---

## 历史问题速查

| 现象 | 根因 | 对应 Layer |
|------|------|-----------|
| 卡在"初始化环境" | bootstrap 崩溃，state file 停在中间 | Layer 5：每个 download 步骤有错误处理 |
| TTS 节点无法连接 | YAML 中 node id 与 dynamic 节点 inputs 不一致 | Layer 4：id 引用自洽 |
| 模型存在但 preflight 失败 | model_ready 文件列表与实际下载不一致 | Layer 5 + Layer 6 交叉对比 |
| 打开 app 找不到 binary | build 了但未 cp，或路径变量引用旧名称 | Layer 2：完整链检查 |
| 节点收到字面量 `__XXX__` | YAML 占位符未替换 | Layer 4：`grep "__"` |
