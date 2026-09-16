# Compiler Scripting Charter

> **页型**: 决策页 ｜ **状态**: 📋 设计已定 / **未实施**（charter，不进 roadmap minor 表）｜ **代码**: —
> **相关**: [架构总览](architecture.md) ｜ **对齐**: 2026-09-16

> **Status**: charter / not-scheduled — 长期目标，不进 roadmap minor 表
>
> **Created**: 2026-05-22
>
> **Driving question**: 如何让 z42 在全平台支持 Roslyn-style 动态编译 API（`Compile(source)` / `Eval(source)`），同时不让 mobile 包体爆炸？
>
> **Strategic decision**: 路径 2b —— pre-1.0 host-only；1.0 自举完成后 z42-written compiler 作为 zpkg 自然随 VM 全平台分发
>
> **Related**: memory project_mobile_no_compiler · [`stdlib/organization.md`](../../../design/stdlib/organization.md) · [`runtime/embedding.md`](../runtime/embedding.md) · [`runtime/hot-reload.md`](../runtime/hot-reload.md)

---

## 1. 动机

z42 设计目标是"全栈系统语言"，host-equivalent 动态编译能力（运行时 `Eval(source)` / `Compile(source)`）应该全平台可用。问题是：当前 compiler 是 C# bootstrap，**移植到 mobile/WASM 不优雅**（NativeAOT 矩阵脆 + 包体 15–30 MB）。

观察：z42 在 1.0 自举完成后，compiler 自身 = 一组 `.zpkg`，跟随 VM 走，零额外 toolchain。这意味着 **"全平台 compiler" 在自举完成后是白送的** —— 只要 compiler 已经被组织成 stdlib 包形态。

本 charter 定义这件事的**目标形态**，让自举工作 + scripting 工作向同一方向收敛。

---

## 2. 策略选择回顾

| 路径 | 内容 | 决策 |
|------|------|------|
| **2a** | 把 C# bootstrap compiler 用 .NET NativeAOT 编译到 iOS/Android/WASM | ❌ 不选 |
| **2b** | 等 1.0 自举完成，z42-written compiler 作为 zpkg 分发到全平台 | ✅ 选定 2026-05-22 |

**2a 弃用理由**：
- mobile 包体 +15–30 MB
- NativeAOT 在 iOS（.NET 9+ 才转正）/ Android（.NET 9+ limited）/ WASM（走 mono-wasm，不同 toolchain）矩阵脆
- 需维护双 codebase：C# bootstrap × 4 mobile 平台 + z42 自举后 × 4 mobile 平台
- Roslyn 自己也没有真正在生产 ship 到 mobile

**2b 选择理由**：
- z42-written compiler 体积估 2–5 MB（z42 字节码 + 元数据），跟随 VM 走
- 不引入额外 toolchain（zpkg 是 VM 已经能加载的产物）
- 自举本就是 [roadmap 1.0 必经里程碑](../../../roadmap.md#长期-semver-路线05--10)，全平台 compiler 是顺带
- 与 project_supported_platforms "只支持厂商官方维护的架构" 兼容

---

## 3. 目标模块拆分

把当前 C# 7 模块（[src/compiler/README.md](../../../../src/compiler/README.md)）映射到 8 个 stdlib 包：

| 标准库包 | 层级 | 对应 C# 模块 | 当前 C# 行数 | 主要类型 |
|---------|:---:|------|:---:|------|
| `z42.compiler.diagnostics` | L1 | `z42.Core` | 1.1K | `Diagnostic` / `Span` / `DiagnosticBag` / `LanguageFeatures` |
| `z42.compiler.ir` | L1 | `z42.IR` | 4.2K | `IrModule` / `Op` / `ZbcReader` / `ZbcWriter` / `BinaryFormat` |
| `z42.compiler.syntax` | L1 | `z42.Syntax` | 5.2K | `Lexer` / `Parser` / AST 节点 |
| `z42.compiler.semantics` | L2 | `z42.Semantics` | 14.7K | `TypeChecker` / `Bound*` / `IrGen` / `SymbolCollector` |
| `z42.compiler.project` | L2 | `z42.Project` | 4.6K | manifest 解析 / source discovery / `ZpkgBuilder` |
| `z42.compiler.pipeline` | L2 | `z42.Pipeline` | 3.3K | `SingleFileCompiler` / `PackageCompiler` / `WorkspaceBuildOrchestrator` |
| `z42.scripting` | L3 | **NEW** | ~500（估）| `Script.Eval` / `Script.Compile` / `ScriptState` / `ScriptOptions` |
| `z42.compiler.driver` | L3 | `z42.Driver` | 1.6K | CLI 命令路由（build / check / disasm / explain / test）|

**合计**：~35K 行 C# 源 → 8 个 zpkg。

层级分配理由：
- **L1**（`diagnostics` / `ir` / `syntax`）：纯数据结构 + 字节流；任何独立工具可单独消费（fmt / lsp / disasm 等）
- **L2**（`semantics` / `project` / `pipeline`）：需要 `z42.io` 文件能力（加载 zpkg / 写 zbc）
- **L3**（`scripting` / `driver`）：消费 pipeline 的 API 层 + CLI

---

## 4. 依赖关系

```
z42.core (prelude)
  ↓
z42.compiler.diagnostics ────┐
                              ↓
z42.compiler.ir ─────────┐    │
                          ↓   ↓
                    z42.compiler.syntax
                          ↓
                    z42.compiler.semantics ←── z42.compiler.project
                          ↓                          ↓
                    z42.compiler.pipeline ←─────────┘
                          ↓
              ┌───────────┴───────────┐
              ↓                       ↓
        z42.scripting          z42.compiler.driver
        (eval/script API)        (CLI 命令)
```

严格遵守 [`stdlib/organization.md` 规则 #4](../../../design/stdlib/organization.md)：上层依赖下层，禁止反向。

---

## 5. 关键设计点（待裁决，spec 阶段决定）

这些问题在本 charter 阶段不强制定下，但接近实施时必须先回答。

### P1. `z42.compiler.diagnostics` 独立 vs 并入 `syntax`

**问题**：仅 1.1K 行的 Diagnostic 体系是否值得单独成包？

**选项**：
- A：独立包 —— diagnostic 是跨阶段共享契约（lsp / fmt / lint / scripting 都消费）
- B：并入 `syntax` —— 减少包数量

**当前倾向**：A（保持独立）—— 跨工具共享的契约不应绑死在 syntax 上

### P2. `z42.compiler.semantics` 14.7K 单包 vs 拆分

**问题**：semantics 内部已分 `TypeCheck/` / `Codegen/` / `Bound/` / `Symbols/` / `Synthesis/` 5 子目录，是否拆为多个 zpkg？

**选项**：
- A：保持单包，内部用 namespace 切分（对照 C# `System.Private.CoreLib` 单 assembly 多 namespace）
- B：拆 `z42.compiler.semantics.typecheck` + `z42.compiler.semantics.codegen` —— fmt / lint 工具可只取 typecheck

**当前倾向**：A —— 跨包内部 API 会变成公开 API，对内部演化是负担

### P3. `z42.compiler.driver` 是否进 mobile 分发？

**问题**：CLI 入口在移动端有意义吗？

**当前倾向**：**不进** —— driver = CLI；mobile 分发仅含 `diagnostics → pipeline + scripting`（6 个 zpkg）。driver 保留 host-only。

### P4. `z42.scripting` API 形态

**问题**：Roslyn-style async / 同步 state-passing / 编译执行分离，哪个为主？

**当前倾向**：**三者并存**：
- 形态 C（底层）：`Compiler.Compile(source, opts) → CompileResult { Bytes, Diagnostics }` + `Vm.Load(bytes)` + `Vm.Invoke(name)`
- 形态 B（状态承载）：`ScriptState state = Script.Create(); state = state.Eval(snippet); ...`
- 形态 A（sugar）：`var result = Script.Eval(source)` / `await Script.EvalAsync(source)`

底层 C 是 pipeline 直接产物；B 在其上加 binding 持久化；A 是单次调用 sugar。

### P5. 自举循环验证（roadmap 1.0 "byte-identical" 要求）

**问题**：如何验证 z42-written compiler 自举固定点？

**当前方案**：
1. **Stage 1**：C# bootstrap 编译 z42-written compiler `.z42` 源 → `z42.compiler.*.zpkg.v1`
2. **Stage 2**：用 v1 重新编译同一份源 → `v2`
3. **Stage 3**：`v1 ≡ v2` 字节相同 → 自举固定点

复用 [`.zbc` strict-pin](../../../spec/archive/2026-05-14-freeze-zbc-v1) byte-golden 基础设施。

### P6. iOS / WASM interp-only 限制如何在 API 表达

**问题**：W^X 约束下，iOS / WASM 不能运行时 JIT，API 层如何表达？

**当前方案**：
```z42
ScriptOptions opts = ScriptOptions.Default;
// opts.AllowJit 默认值：iOS / WASM = false（不可改）；其他平台 = true
```

- 平台 facade 在 `Vm.Create()` 时注入该默认值
- 用户强制 `AllowJit = true` on iOS / WASM 时抛 `PlatformNotSupportedException`
- 文档明示行为差异

---

## 6. 长期实施顺序（charter，不进 roadmap 表）

| 阶段 | 动作 | 估算触发点 | 依赖 |
|------|------|---------|------|
| **C1** | C# 端把 7 模块改造为"可作为库被嵌入"（`ICompiler` / `IPipeline` interface） | 0.5.x 间隙（与 LSP Q13 共用基础）| 现有 C# 模块成熟 |
| **C2** | `z42.scripting` v0 上线 host 5 平台，底层调 C# library | 0.6.x – 0.7.x | C1 |
| **C3** | L3 全 feature 就绪（lambda / generic / async / Result / 反射） | 0.5 – 0.9 主线 | roadmap 主线 |
| **C4** | z42 重写 7 个 compiler 模块的 `.z42` 源（每模块独立 spec） | 1.0-α – 1.0-rc | C3 |
| **C5** | byte-identical fixed-point 验证 | 1.0.0 | C4 |
| **C6** | 7 个 zpkg + scripting 进 mobile / WASM 分发；移除 `PlatformNotSupportedException` | 1.1.x – 1.2.x | C5 + Q15 (WASM GC) |

**关键依赖链**：
- C1 → C2（host scripting 落地）
- C3 → C4（自举语言能力前置）
- C4 → C5 → C6（自举 → byte-identical → mobile 落地）
- C1 也为 LSP（Q13）铺路 —— compiler library API 是两者共享前提

---

## 7. 与现有 design doc 的关系

| Doc | 关系 |
|-----|------|
| [`compiler-architecture.md`](../formats/zpkg.md) | 当前 C# bootstrap 形态；C4 完成后此 doc 转为"过渡阶段历史记录"，新 SoT 是 z42-written 源 + 本 charter |
| `compilation.md` | 编译产物粒度策略；自举后维持不变（z42 compiler 产出同一种 .zbc / .zpkg）|
| [`project.md`](../../../reference/src/toolchain/z42-toml.md) | manifest schema；自举后 `z42.compiler.project` 实现这套 schema |
| [`runtime/embedding.md`](../runtime/embedding.md) | VM 嵌入 API；scripting 在其上加 in-memory module 加载（C2 引入）|
| [`runtime/hot-reload.md`](../runtime/hot-reload.md) | runtime 加载模块；scripting 与 hot-reload 共享 `Vm.LoadInMemoryModule(bytes)` 接口 |
| [`stdlib/organization.md`](../../../design/stdlib/organization.md) | L0–L3 分层规则；本拆分严格遵守 |

---

## 8. 触发本 charter 进入实施的条件

**必要条件**（任一不满足 → 本 charter 保持冰冻，不开 spec）：

- ✅ L2 测试体系 + 标准库基础完成（M6 / M7，当前焦点）
- ✅ L3 主要特性就绪（lambda / generic / async / Result / 反射，0.5 – 0.8.x 主线）
- ✅ 用户明确呼声 / 应用场景出现

**建议触发节点**：

- **0.5.x 中期**：C1 可启动（compiler library API 抽象）—— 与 LSP 共用
- **0.6.x – 0.7.x**：C2 启动（host scripting v0）
- **1.0-α**：C4 启动（z42 重写）

---

## 9. 与 path 2a 重新评估的触发条件

本 charter 锁定 2b 不代表永久排除 2a。以下任一发生 → 重新评估是否插入 2a 作为 stopgap：

1. App Store / Play Store 政策对 dynamic code execution 收紧 —— 2a 也无解，本条不触发
2. NativeAOT iOS / Android matured 到可行 + 出现可复用社区方案
3. 用户在 1.0 前明确需要 mobile dynamic eval（非 host-only 开发期工具）

重新评估时回到 memory project_mobile_no_compiler 的"How to apply"部分调整。

---

## 10. Deferred / 未来工作

本 charter 自身即处于 deferred 状态，所有事项均归属"未排期"。无独立 deferred 子项。

未来 spec 阶段（C1 启动时）需补的细化：
- `z42.scripting` 具体 API surface 设计（与 stdlib 其他 L3 包对齐风格）
- 自举 byte-identical 验证脚本与 CI 集成
- mobile 分发包体增量预算（C6 启动前测量）
- 多平台 scripting 性能基线（interp-only 模式 vs JIT，philosophy §9 五指标对齐）
