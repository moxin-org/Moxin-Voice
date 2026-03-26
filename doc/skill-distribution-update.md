---
name: distribution-update
description: Use when merging a feature branch that adds, removes, or renames Rust nodes, models, or dataflow YAML — guides which distribution files must be updated and in what order.
---

# Skill: 分发脚本更新

## 触发条件

特性分支含以下任一变动：新增/删除/重命名 Rust 节点、新增模型需下载、新增/删除数据流 YAML。

---

## 变动分类表

| 变动类型 | 典型路径 |
|---------|---------|
| 新 Rust binary | `node-hub/dora-xxx/` 或 `moxin-xxx/` |
| 新模型（需下载） | `moxin-init/src/main.rs` |
| 新数据流 YAML | `scripts/dataflow/*.yml` |
| 删除/重命名 | 以上路径的反向操作 |

---

## A. 新增 Rust binary

以下五处缺一不可：

```
① Cargo.toml（根）       → members 加入 crate 路径
② build_macos_app.sh    → run_cargo_build -p <crate>
③ build_macos_app.sh    → NEW_BIN_PATH="$ROOT_DIR/target/$PROFILE/<binary>"
④ build_macos_app.sh    → [[ ! -f "$NEW_BIN_PATH" ]] 存在性检查
⑤ build_macos_app.sh    → cp + chmod +x "$MACOS_DIR/<binary>"
```

数据流 YAML 中节点 `path` 改为 `../MacOS/<binary>`（bundle 相对路径）。

---

## B. 新增模型（首次下载）

| 文件 | 改动 |
|------|------|
| `moxin-init/src/main.rs` | 新增 `model_ready_xxx()` + 下载步骤 + 更新 `total` 步骤数 |
| `scripts/macos_preflight.sh` | 必须项 → `errors+=`；可选项 → `warnings+=` |
| `scripts/macos_bootstrap.sh` | `exec env` 传递新模型路径变量 |
| `scripts/build_macos_app.sh` | launcher 段 `export NEW_MODEL_PATH=...` |

---

## C. 新增数据流 YAML

```bash
# 1. 确认文件存在
ls scripts/dataflow/<new>.yml

# 2. build_macos_app.sh 加入复制步骤
cp "$ROOT_DIR/scripts/dataflow/<new>.yml" "$DATAFLOW_DIR/<new>.yml"

# 3. 若需动态路径，在 launcher 段加对应 cat > ... <<YAML
```

---

## D. 删除/重命名

执行 A/B/C 的逆操作，从所有涉及文件中删除对应条目。

---

## 自检清单

- [ ] 每个被 build 的 crate 都在 `Cargo.toml` members 中
- [ ] 每个被 `cp` 到 bundle 的 binary 有对应 build 步骤和存在性检查
- [ ] 每个数据流 YAML 中非 dynamic 节点的 `path` 指向 bundle 中存在的 binary
- [ ] `moxin-init` 下载的每个模型，`preflight` 都有对应就绪检查
- [ ] launcher 导出的环境变量覆盖所有 bootstrap/preflight/runtime 所需路径

---

## 验证命令

```bash
bash -n scripts/macos_bootstrap.sh
bash -n scripts/macos_preflight.sh
bash -n scripts/build_macos_app.sh
grep -r "__" scripts/dataflow/ scripts/build_macos_app.sh   # 应无输出
git diff --stat
