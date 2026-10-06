# Compiler Scripting Charter

> **页型**: 决策页 ｜ **代码**: —
> **相关**: [架构总览](architecture.md)
> **待办**: 设计已定，尚未实施（charter，不进 roadmap minor 表）

> **Status**: charter / not-scheduled — 长期目标，不进 roadmap minor 表
>
> **Driving question**: 如何让 z42 在全平台支持 Roslyn-style 动态编译 API（`Compile(source)` / `Eval(source)`），同时不让 mobile 包体爆炸？
>
> **Strategic decision**: 路径 2b —— pre-1.0 host-only；1.0 自举完成后 z42-written compiler 作为 zpkg 自然随 VM 全平台分发
>
> **Related**: memory project_mobile_no_compiler · [`stdlib/organization.md`](../stdlib/organization.md) · [`runtime/embedding.md`](../runtime/embedding.md) · [`runtime/hot-reload.md`](../runtime/hot-reload.md)

---

## 1. 动机

z42 设计目标是"全栈系统语言"，host-equivalent 动态编译能力（运行时 `Eval(source)` / `Compile(source)`）应该全平台可用。问题是：mobile/WASM 不适合 ship 一个原生 toolchain（NativeAOT 矩阵脆 + 包体 15–30 MB）。

观察：z42c 全部由 z42 编写，compiler 自身 = 一组 `.zpkg`，跟随 VM 走，零额外 toolchain。这意味着 **"全平台 compiler" 是白送的** —— 只要 compiler 已经被组织成包形态。

本 charter 定义这件事的**目标形态**，让自举工作 + scripting 工作向同一方向收敛。

---

## 2. 策略选择回顾

| 路径 | 内容 | 决策 |
|------|------|------|
| **2a** | 把原生 compiler 用 NativeAOT 编译到 iOS/Android/WASM | ❌ 不选 |
| **2b** | z42-written compiler 作为 zpkg 分发到全平台 | ✅ 选定 |

**2a 弃用理由**：
- mobile 包体 +15–30 MB
- NativeAOT 在 iOS（.NET 9+ 才转正）/ Android（.NET 9+ limited）/ WASM（走 mono-wasm，不同 toolchain）矩阵脆
- 需为 4 个 mobile 平台各维护一套原生 compiler 构建
- Roslyn 自己也没有真正在生产 ship 到 mobile

**2b 选择理由**：
- z42-written compiler 体积估 2–5 MB（z42 字节码 + 元数据），跟随 VM 走
- 不引入额外 toolchain（zpkg 是 VM 已经能加载的产物）
- 自举本就是 [roadmap 1.0 必经里程碑](https://github.com/z42-lang/z42/blob/main/docs/roadmap.md#长期-semver-路线05--10)，全平台 compiler 是顺带
- 与 project_supported_platforms "只支持厂商官方维护的架构" 兼容

---

## 3. 目标模块拆分

compiler 域的包都在 [`src/compiler/`](https://github.com/z42-lang/z42/blob/main/src/compiler/README.md) 这一个 workspace 里：

| 包 | 层级 | 命名空间 | 主要类型 |
|---|:---:|------|------|
| `z42c.core` | L1 | `Z42.Core` | `Diagnostic` / `Span` / `DiagnosticBag` / `LanguageFeatures` |
| `z42.package` | L1 | `Z42.IR` / `Z42.Package` | IR 模型 / `ZbcReader` / `ZbcWriter` / `ZpkgBuilder` / `BinaryFormat` |
| `z42c.syntax` | L1 | `Z42.Syntax` | `Lexer` / `Parser` / AST 节点 |
| `z42c.optimization` | L2 | `Z42.Optimization` | `IrOptPipeline` / `Opt` / 各优化 pass |
| `z42c.semantics` | L2 | `Z42.Semantics` | `TypeChecker` / `Bound*` / `SymbolCollector` / `FrontEnd` |
| `z42c.emission` | L2 | `Z42.Emission` | `IrGen` / 各 `*Emitter` / `IrDump` / `CompiledModuleZ` |
| `z42.project` | L2 | — | manifest 解析 / source discovery |
| `z42c.pipeline` | L2 | `Z42.Pipeline` | `Z42cCompiler` / `PackageCompile` / `WorkspaceBuild` |
| `z42.scripting` | L3 | — | `Script.Eval` / `ScriptState` / `Engine` |
| `z42c.driver` | L3 | `Z42.Driver` | CLI 命令路由（build / check / disasm / explain / test）|

层级分配理由：
- **L1**（`z42c.core` / `z42.package` / `z42c.syntax`）：纯数据结构 + 字节流；任何独立工具可单独消费（fmt / lsp / disasm 等）
- **L2**（`z42c.optimization` / `z42c.semantics` / `z42c.emission` / `z42.project` / `z42c.pipeline`）：需要 `z42.io` 文件能力（加载 zpkg / 写 zbc）
- **L3**（`z42.scripting` / `z42c.driver`）：消费 pipeline 的 API 层 + CLI

---

## 4. 依赖关系

```
z42.core (prelude)
  ↓
z42c.core ──────────────┐
                         ↓
z42.package ────────┐    │
                     ↓   ↓
                z42c.syntax
                     ↓
                z42c.semantics
                     ↓
z42c.optimization → z42c.emission      z42.project
                     ↓                  ↓
                z42c.pipeline ←────────┘
                     ↓
         ┌───────────┴───────────┐
         ↓                       ↓
   z42.scripting            z42c.driver
   (eval/script API)        (CLI 命令)
```

严格遵守 [`stdlib/organization.md` §2](../stdlib/organization.md)：依赖图必须无环，`z42.core` 在所有库之下。

---

## 5. 关键设计点（待裁决，spec 阶段决定）

这些问题在本 charter 阶段不强制定下，但接近实施时必须先回答。

### P1. `z42c.core` 独立 vs 并入 `syntax`

**问题**：小体量的 Diagnostic 体系是否值得单独成包？

**选项**：
- A：独立包 —— diagnostic 是跨阶段共享契约（lsp / fmt / lint / scripting 都消费）
- B：并入 `syntax` —— 减少包数量

**当前倾向**：A（保持独立）—— 跨工具共享的契约不应绑死在 syntax 上

### P2. 语义分析与代码生成的包边界

**已定**：拆为两包——`z42c.semantics`（符号收集 / 绑定 / 类型检查 / 校验 / TSIG 导出面提取）与
`z42c.emission`（Bound → IR + 单文件 / 包编译编排）。依赖单向 `z42c.emission → z42c.semantics`，语义层零引用
代码生成层；fmt / lint / lsp 类工具只取 `z42c.semantics`，不连带 IR 优化与代码生成。代价是两包之间的内部
API 变成跨包公开面（`SymbolTable` / `Bound*` / `SemanticModel` 等），演化时要按跨包接口对待。

### P3. `z42c.driver` 是否进 mobile 分发？

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
1. **Stage 1**：已有 z42c 编译 compiler `.z42` 源 → gen1 zpkg
2. **Stage 2**：用 v1 重新编译同一份源 → `v2`
3. **Stage 3**：`v1 ≡ v2` 字节相同 → 自举固定点

复用 `.zbc` strict-pin byte-golden 基础设施。

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
| **C1** | 把 compiler 模块组织为"可作为库被嵌入"（`ICompiler` / `IPipeline` interface） | 与 LSP Q13 共用基础 | 现有模块成熟 |
| **C2** | `z42.scripting` v0 上线 host 5 平台，底层调 compiler 库 | 0.6.x – 0.7.x | C1 |
| **C3** | L3 全 feature 就绪（lambda / generic / async / Result / 反射） | 0.5 – 0.9 主线 | roadmap 主线 |
| **C4** | compiler 模块全部以 `.z42` 源维护（每模块独立 spec） | 1.0-α – 1.0-rc | C3 |
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
| [`compiler-architecture.md`](../formats/zpkg.md) | compiler 形态以 z42-written 源（`src/compiler/`）+ 本 charter 为 SoT |
| `compilation.md` | 编译产物粒度策略；自举后维持不变（z42 compiler 产出同一种 .zbc / .zpkg）|
| [`project.md`](https://z42-lang.github.io/z42/reference/toolchain/z42-toml.html) | manifest schema；自举后 `z42.project` 实现这套 schema |
| [`runtime/embedding.md`](../runtime/embedding.md) | VM 嵌入 API；scripting 在其上加 in-memory module 加载（C2 引入）|
| [`runtime/hot-reload.md`](../runtime/hot-reload.md) | runtime 加载模块；scripting 与 hot-reload 共享 `Vm.LoadInMemoryModule(bytes)` 接口 |
| [`stdlib/organization.md`](../stdlib/organization.md) | 包划分与依赖无环规则；本拆分严格遵守 |

---

## 8. 触发本 charter 进入实施的条件

**必要条件**（任一不满足 → 本 charter 保持冰冻，不开 spec）：

- L2 测试体系 + 标准库基础完成（M6 / M7，当前焦点）
- L3 主要特性就绪（lambda / generic / async / Result / 反射，0.5 – 0.8.x 主线）
- 用户明确呼声 / 应用场景出现

**建议触发节点**：

- **0.5.x 中期**：C1 可启动（compiler library API 抽象）—— 与 LSP 共用
- **0.6.x – 0.7.x**：C2 启动（host scripting v0）

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
- 多平台 scripting 性能基线（interp-only 模式 vs JIT，对齐 `docs/roadmap.md` 的五条性能基线）
