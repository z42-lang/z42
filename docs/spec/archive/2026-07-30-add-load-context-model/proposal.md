# Proposal: 加载上下文模型（AssemblyLoadContext / ALC 地基）

## Why

z42 运行时**没有任何"代码边界"概念**：`metadata::merge::merge_modules` 在加载时把所有 zpkg
塌成**一个扁平 `Module`**（一份 `Vec<Function>` + 一份 `type_registry` + 一份串池），跨包依赖走
第二条同样扁平、加载后**从不卸载**的 `LazyLoader`。没有边界 = 没有可卸载的单元、没有可问
"被谁引用"的对象、没有可界定"重载范围"的粒度。

这是 hot-reload / zpkg 卸载 / 保留根诊断三件事共同的前置地基（见
[load-context.md](../../../design/runtime/load-context.md) /
[tiered-execution.md](../../../design/runtime/tiered-execution.md)，均为 DESIGN 未实施）。本 change
只做**地基**：引入 dotnet `AssemblyLoadContext` 对标的 `AssemblyLoadContext` 模型，把 zpkg 载入
**可界定边界的上下文**，并让 zpkg 身份在运行时以 `Assembly` 反射投影**保留下来**。

**本 change 不含卸载、不含 hot-reload**——只建"边界 + 身份标记"。卸载（惰性/强制回收）与细粒度
patch 是后续独立 change。

## What Changes

- **运行时 `AssemblyLoadContext` 模型**：`root`（core/stdlib/主程序所在，永驻不可回收）+ 可创建的
  `collectible` 上下文（各自独立 arena）。root **保持现有扁平 merge 路径不变**（热路径零回归）；
  collectible 走独立 arena、不并入 root。
- **加载路径分叉**：`load into root`（现有 `merge_modules`，不动）vs `load into collectible`
  （独立 arena，解析 + 建元数据，反射可见）。
- **`Std.Runtime.AssemblyLoadContext`（新 z42 类）**：`Default` / `Name` / `IsCollectible` /
  `CreateCollectible(name)` / `Load(zpkgPath) -> Assembly` / `GetAssemblies()` /
  `Unload()`（**Phase 1 声明但抛 `NotSupportedException`**——回收机制下一 change 落）。
- **`Std.Reflection.Assembly`（新 z42 类）**：zpkg 的运行时反射投影，native 句柄背书（仿 `Type`）。
  `Name` / `IsCollectible` / `AssemblyLoadContext` / `GetTypes()`。
- **`Std.Type` 追加两成员**：`IsCollectible`（== `this.Assembly.IsCollectible`）+ `Assembly`
  （定义此类型的 assembly，镜像 .NET `Type.Assembly`）。
- **Rust 侧**：新 `corelib/assemblyloadcontext.rs`（`__lctx_*` / `__asm_*` / `__type_is_collectible` /
  `__type_assembly` builtins）+ `VmCore` 上下文注册表 + `TypeDesc` 的 context/assembly 回指 +
  `Value` 的 native handle 变体（AssemblyLoadContext / Assembly 句柄）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42` | NEW | `Std.Runtime.AssemblyLoadContext` 类 |
| `src/libraries/z42.core/src/Reflection/Assembly.z42` | NEW | `Std.Reflection.Assembly` 类 |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | 加 `IsCollectible` + `Assembly` 两个 extern 属性 |
| `src/runtime/src/corelib/assemblyloadcontext.rs` | NEW | AssemblyLoadContext / Assembly / Type-collectible builtins |
| `src/runtime/src/corelib/mod.rs` | MODIFY | `pub mod loadcontext;` + BUILTINS 表注册新 builtins |
| `src/runtime/src/metadata/context.rs` | NEW | `AssemblyLoadContext` / `ContextId` / `ContextRegistry` 运行时模型 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `TypeDesc` 加 context/assembly 回指字段；`NativeData` 加句柄变体 |
| `src/runtime/src/vm_context.rs` | MODIFY | `VmCore` 挂 `ContextRegistry`；root 上下文初始化 |
| `src/runtime/src/metadata/loader.rs` | MODIFY | 加载路径分叉：root(merge) vs collectible(独立 arena) |
| `src/runtime/src/metadata/mod.rs` | MODIFY | `pub mod context;` 声明 |
| `docs/design/runtime/load-context.md` | MODIFY | 页头加"Phase 1 地基已落地（本 change）"对齐；记录决策修订（强制清理轴、粒度可调） |
| `docs/book/src/runtime/load-context.md` | NEW | book 机制页：AssemblyLoadContext 模型 + root/collectible + Assembly 反射（知识上浮） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新 book 页 |
| `src/tests/reflection/load_context/source.z42` | NEW | golden e2e：Default/CreateCollectible/Name/IsCollectible/typeof.IsCollectible/Unload 抛异常 |
| `src/tests/reflection/load_context/expected_output.txt` | NEW | 期望输出 |
| `src/runtime/src/corelib/assemblyloadcontext_tests.rs` | NEW | Rust 单测：注册表 root vs collectible / load_into / assembly_types 排序 / 可回收性传播 |

> **实施期 Scope 校正**：① 测试改为 golden e2e（`src/tests/reflection/load_context/`，reflection/ 确定被 e2e 发现）+ Rust 注册表单测——`Environment` 类不存在，z42 golden 拿不到可加载 zpkg 路径，故 collectible-Load→GetTypes→IsCollectible==true 由 Rust 单测覆盖，不再需要 `dep/` 测试 zpkg。② `src/libraries/z42.core/src/Reflection/` 无 README（sibling 无先例）。③ `src/runtime/src/corelib/` 与 `src/runtime/src/metadata/` **均无 README**（第 4 层，超出 3 层 README 要求）——删除这两个 MODIFY 行。
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记本 change 占用 runtime+stdlib（worktree 预抢） |

**只读引用**（理解上下文必须读，不修改）：

- `src/libraries/z42.core/src/GC/GC.z42` — `[Native]` 绑定范式参考
- `src/runtime/src/corelib/gc.rs` — Rust builtin 实现范式参考
- `src/runtime/src/metadata/merge.rs` — 现有 merge 逻辑（root 路径保持不变）
- `src/runtime/src/metadata/lazy_loader.rs` — 现有跨包加载（Phase 1 不改，理解 arena 交互）
- `src/libraries/z42.core/src/Runtime.z42` — 既有 `Std.Runtime` 命名空间（LoadZpkg/CallStatic stub，不动）

## Out of Scope

- **卸载 / 内存回收机制**（惰性 + 强制 tombstone/trap）—— 下一 change（`whyRetained` 诊断随之）。
  Phase 1 的 `Unload()` 声明但抛 `NotSupportedException`。
- **细粒度 hot-reload / patch**（函数体替换 / 增删 / 类型版本共存）—— 后续 change。
- **跨 context 执行**（root ↔ collectible 互相调用函数）—— 本 change 只做**加载 + 反射可见**；
  collectible zpkg 的函数**能否被调用**不在 Phase 1（下一步）。跨 context **类型元数据引用**
  （collectible 类型的 base = root 的 `Std.Object`）在加载时解析，仅限元数据层。
- **动 `Std.Runtime.Runtime.LoadZpkg` / `CallStatic` stub**（FC3）：保留不动，`AssemblyLoadContext.Load`
  是其正确设计归宿，等 Phase 1 落地后单开小 change 删 stub。
- **de-merge root**：root 永远保持扁平 merge + O(1) MethodId dispatch，不拆。

## Open Questions

- [ ] （无——三处设计岔口 FC1=A / FC2=(ii) / FC3=保留 已与 User 敲定）
