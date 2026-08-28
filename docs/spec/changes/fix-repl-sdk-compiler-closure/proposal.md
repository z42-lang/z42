# Proposal: 修复打包 SDK 里 z42i REPL 全量求值加载不到编译器闭包

> 类型：`fix`（runtime loader bug）。与 `move-scripting-to-libraries` 同分支/PR 落地（User 定「一起修复」），
> 但**独立逻辑单元、独立 commit**。归属：`stdlib-interop-and-repl-split-program` 轴 2 收尾。

## Why（bug）

打包 SDK 里 `z42 repl -c '1 + 2'` 全量求值**长期报错**（`package-host` gate 自 add-z42-repl / fix-repl-sdk-layout
起就红，早于 PR-B）：

```
ModuleLoader: cannot read dep zpkg meta `z42c.semantics.zpkg`: not found in any search dir (2 candidates)
Error: undefined function `Z42.Semantics.IrDump.ParseAll...`
```

**根因**：REPL 经 `ReplCompilerHost` 运行期反射注入编译器——`ModuleLoader.Load(<sdk>/programs/z42c/z42c.pipeline.zpkg)`
按**绝对路径**加载 pipeline。但 pipeline 的传递依赖闭包（`z42c.semantics` + 兄弟）解析走的是 **VM 启动时固定的
`search_dirs`**（= app 自身 entry-dir `programs/z42i/` + `Z42_LIBS`），**不含 pipeline 自身所在的
`programs/z42c/`**——尽管 semantics 就躺在 pipeline 旁边。→ 依赖闭包解析不到 → `undefined`。

这正是 `xtask_package_desktop.z42` 里那条 KNOWN LIMITATION（「z42i shipped 全量求值缺 semantics/pipeline，正解=
z42b publish 通用传递闭包复制」）的**根因**——但真正的最小根治不在打包层，而在 loader：**「按路径加载一个
模块，应让它的同目录兄弟能被解析」**。

## What Changes（最小根治，runtime）

`LazyLoader::load_module_from_path`（`src/runtime/src/metadata/lazy_loader.rs`）在注册被加载 artifact **前**，把
**被加载 zpkg 自身所在目录**并入 `self.search_dirs`（VFS-aware `is_dir` 判定 + 去重 + **append 到末尾**=最低
优先级，绝不 shadow app dir / `Z42_LIBS`；无同目录依赖时无害）。镜像 `app::run` 的「entry-zpkg dir first」规则。

- 通用：惠及**所有** `ModuleLoader.Load` 调用方（REPL 注入 + z42b 动态注入 + `Std.Test` 加载外部 test 模块）。
- 无 zpkg 复制、不改打包布局、不改 `ReplCompilerHost`（探测逻辑不变）。
- 零格式 bump、非新 builtin（纯内部 loader 行为、更宽松→不可能破坏既有解析）。

## Scope

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | `load_module_from_path`：并入被加载 zpkg 目录到 search_dirs |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | 删除已过时的 KNOWN LIMITATION 注释，改为「已根治」说明 |
| `src/libraries/z42.scripting/src/ReplCompilerHost.z42` | MODIFY | 注释：闭包解析现含被加载 zpkg 目录 |

## Out of Scope

- z42b publish 的「通用传递闭包复制」（原注释设想的另一条正解路径）：不再需要——loader 根治后闭包在原目录即可解析。
- REPL 限定名 `Std.IO.Console.WriteLine`（`undefined: Std`）既有限制：无关，不动。

## 验证（CI 权威）

本地 `cargo check` 通过（仅 pre-existing warnings）。端到端真验 = **`package-host` gate 的 `z42 repl --config -c '1 + 2' → 3`
smoke 转绿**（4 平台）。runtime 改动另由 CI `test-host`（含 `cargo test --lib`）覆盖。零格式 bump。
