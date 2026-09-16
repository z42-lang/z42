# `compiler-architecture.md`（1431 行）逐节核实结果

> 批 1 执行，User 2026-09-16 裁决「现在就做逐节核实」。
> 判据：**该节描述的机制，在自举的 z42c 里是否仍存在**（grep 当前 `src/`）+
> **现有 internals 页是否已覆盖**（`source-compile.md` 704 行 / `project-model.md`）。

该文档开篇自述：「记录 **C# bootstrap 编译器**的内部数据结构、算法、加载策略」。
C# 编译器已于 2026-06-26 移除（`find src -name '*.cs'` = 0）。

## 结论一览（18 节）

| 类别 | 节数 | 处置 |
|---|---|---|
| 现有 internals 页**已覆盖** | 8 | 删（内容已在 source-compile / project-model） |
| 机制**已不存在** | 4 | 删 |
| C# 结构细节，无可移植内容 | 5 | 删 |
| **仍有价值、且无人覆盖** | 1 | **IMPL 段传播 → 批 2 并入 `formats/zpkg.md`** |

## 已覆盖（8）—— 证据：现有页的命中数

`ManifestLoader`(project-model 2) · `TSIG`(5+7) · `ImportedSymbolLoader`/两阶段加载(9+6) ·
多 CU 包内 symbol 共享(2) · `DependencyIndex`(1+4) · 实例方法绑定 receiver-aware(8) ·
Pratt 表达式解析(1) · 方法重载决议 type-based mangling(16)

## 机制已不存在（4）—— 逐条证据

| 节 | 证据 |
|---|---|
| `BoundVisitor` 统一遍历框架 | 全 `src/` **0 命中** |
| pseudo-class 策略与迁移 | 11 处命中**全在测试夹具**（`rsa_vectors.z42` / `generic_list/source.z42` 等），不是编译器机制 |
| 泛型接口 dispatch —— `Z42InterfaceType.TypeParams` | 当前 `Z42InterfaceType`（`z42c.semantics/src/Z42Type.z42:330`）**无 `TypeParams` 字段**，只有 `Methods` / `IsPartial` / `Visibility` / `BaseNames` |
| Parameter Modifiers 的 `ModifierMangling` | z42 里修饰符是 `Param.IsRef`（**布尔**，非 `ParamModifier` 枚举）；`IsRef` 只出现在 `ForwardGenerator.z42:384`，**不参与 mangling** |

> 参数修饰符一节剩余的活事实（「调用方传地址、VM 处理间接、callee 寄存器类型不变」）
> **已经写在代码注释里**（`z42c.syntax/src/Decl.z42:12`）——再抄一份进文档只会多一处待漂移的副本。

## 唯一要带去批 2 的（1）

**跨 zpkg `impl` 块传播 — IMPL section + Phase 3 merge**（原文 439–517 行，79 行）

- **仍活着**：`z42.ir/src/ZpkgWriter.z42:371` 写 IMPL 段、`ZpkgReader.z42:484` 读，两处注释均自述
  「镜像 C# `BuildImplSection`」「布局 1:1」——**说明这份 C# 文档目前仍是该 wire 布局的最好描述**。
- **归属**：它是 **zpkg 格式**的一部分 ⇒ 批 2 的 `internals/src/formats/zpkg.md`，不是编译器页。
- ⇒ **本批不删 `compiler-architecture.md`**，留到批 2：那时 IMPL 内容落进 `formats/zpkg.md`，
  同一个 PR 内删除原文件。**现在删会让批 2 失去来源，或被迫把内容临时寄放在不自然的位置。**

## 方法论留存

「grep 标识符还在」**不等于**「描述还准」——本次 17/18 节的标识符都能在 `src/` 命中，
但逐个看下去，`Z42InterfaceType.TypeParams`、`ParamModifier` 枚举、`ModifierMangling` 全都不存在。
**核实必须落到字段 / 函数这一级，不能停在关键词。**
