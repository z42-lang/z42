# IR 优化与特化（IR Optimization & Specialization）

> **页型**: 决策页 ｜ **状态**: 📋 设计已定 / **未实施** ｜ **代码**: —
> **相关**: [optimization-pipeline.md](optimization-pipeline.md) ｜ **对齐**: 2026-09-17

> **状态：DESIGN（目标架构，未实施）** · 创建 2026-06-21
>
> 本文设计 z42 的**编译期 IR 优化**与 **intrinsic 特化**：让 tier0 基线本身就快，并提供一套可持续扩展的特化框架（常量折叠 + 引擎内联）。与分层执行 [tiered-execution.md](tiered-execution.md) 互补（那篇讲运行时分层/回收，本篇讲 IR 层优化）；当前架构见 [vm-architecture.md](vm-architecture.md)、IR 指令集见 [ir.md](../formats/ir.md)。

---

## 1. 核心原则：编译期就把 tier0 优化好（2026-06-21 定）

**编译生成 IR 时一定要对 tier0 做优化，使 tier0 基线性能本身就好——不依赖后续 tier 兜底。**

理由：
- **iOS/wasm 无 JIT**，很多时候 tier0（解释器基线）就是最终执行形态（再加 AOT），没有"后面有优化 JIT 会修"这回事。tier0 慢 = 这些平台一直慢。
- 即使 JIT 平台，tier0 也是冷代码与启动期的执行形态；tier0 好 = 启动快、提升前不拖累。
- 编译期优化是**一次性成本、跨 interp/JIT/AOT 三引擎都受益**（IR 是共享输入），比运行时反复优化更划算。

**做法**：IR 生成阶段（编译器，后随 z42c）就跑一组**确定性、与运行时反馈无关**的优化 pass，产出"已经不错"的 IR；运行时分层再做"需要反馈/投机"的那部分。

### 编译期可做的 tier0 优化（确定性，无需运行时反馈）
- **常量折叠 / 传播**（含 intrinsic 折叠，见 §2）
- **devirtualization**：sealed/primitive/已知具体类型 → 虚调用降直调/内联
- **死代码消除 / 无用存储消除**
- **公共子表达式 / 简单 GVN**
- **特化 opcode 降级**：把可静态识别的泛型操作降成专用 opcode（`s.Length`→`StrLen`），interp 直接走快路径
- **简单内联**：小的纯 intrinsic 内联
- **bounds-check 静态消除**（可证明的下标）

> 边界：需要**运行时类型反馈/投机**的优化（多态 inline cache 命中后特化、投机内联 + deopt）留给运行时分层（[tiered-execution.md](tiered-execution.md)），不在编译期硬做。

---

## 2. 特化 / intrinsic 框架（已定方案）

**决策（2026-06-21）**：编译器折常量 + 引擎内联非常量 + **纯度用硬编码 intrinsic 表**（对标 HotSpot intrinsics / .NET `[Intrinsic]`）。

### 2.1 两级特化
1. **编译期常量折叠**：intrinsic + 全常量输入 + 纯 → 折成字面量。跨 interp/jit/aot 受益（尤其 iOS）。
2. **引擎内联**（非常量）：降成专用 opcode → interp 快路径 + JIT 内联 emit。

### 2.2 intrinsic 注册表（避免 if-else 散落）
```
Intrinsic = (接收者类型, 成员) -> {
    pure: bool,                         // 硬编码
    const_fold: fn(const_args) -> Option<Const>,
    interp_op: Option<OpCode>,          // 特化 opcode（解释器快路径）
    jit_lower: Option<fn(emit_ctx)>,    // cranelift 内联 emit
}
```
IR codegen 发成员调用前查表：命中且常量+纯 → 折叠；命中非常量 → 发专用 opcode；未命中 → 泛型 Call 兜底。**统一表 + lowering 接口,不在泛型调用路径塞 special case**（对齐 philosophy 设计完整性）。

### 2.3 正确性 gate
- 只对**不可重写 / 已知具体类型**特化（`String`/`Array` sealed/primitive，`.Length` 不可 override → 安全）；虚成员/可覆盖的不折。
- `pure` 必须真纯（无副作用、确定性）才可折叠 / 跨调用消除。

### 2.4 案例：`"sss".Length`
- `String` sealed + `.Length` 纯 + 不可 override + 输入是**驻留字面量**（`Module.interned_strings`，`string literal interning` 已落地）→ 编译期直接折成 `3`，**零运行时调用**。
- `s.Length`（s 运行时才知）→ 降成 `StrLen` opcode → interp 复用已有 **"Length/CharAt 元数据缓存"** 快路径；JIT 内联 `load [str + len_off]`。

---

## 3. 业界对标

| 运行时 | 特化机制 |
|---|---|
| **HotSpot** | 数百 `@IntrinsicCandidate`（`String.length/charAt`、`Math.*`、`System.arraycopy`…），JIT 用表识别换手写 IR。**= z42 硬编码表方案** |
| **.NET JIT** | `[Intrinsic]` + 表；`Math`/`Vector<T>`/`Span`/`RuntimeHelpers` |
| **CPython PEP 659** | 运行时反馈选特化 opcode（`LOAD_ATTR_INSTANCE_VALUE`、`BINARY_OP_ADD_INT`…），失配 deopt |
| **V8/JSC** | builtins + inline cache + TurboFan reduction 内联 |
| **GraalVM Truffle** | `@Specialization` 自特化节点 + assumption + 偏特化（最通用框架） |
| **C2 / TurboFan** | sea-of-nodes：常量折叠/传播、devirtualization、逃逸分析 |

z42 起步取 **HotSpot/.NET 的"硬编码 intrinsic 表 + 编译期折叠 + JIT 内联"**；Truffle 式动态自特化（assumption 驱动）作为更远期参考。

---

## 4. 常见优化手段菜单（可逐项排期）

| 手段 | 作用 | 适用层 | iOS（无 JIT）可用 |
|---|---|---|---|
| inline cache（mono/poly/mega） | 消解动态派发 | interp + JIT | ✅（解释器内嵌 IC） |
| quickening / superinstruction / 特化 opcode | 解释器自身提速 | interp | ✅（PEP 659 路线） |
| 常量折叠 / 传播、偏特化 | 编译期算掉 | 编译器 | ✅ |
| 方法内联（intrinsic 是其一） | 去调用开销 | JIT/AOT；interp 有限 | 部分（AOT） |
| devirtualization（sealed/CHA） | 虚→直/可内联 | 编译器/JIT | ✅（编译期 sealed） |
| 逃逸分析 / 标量替换 | 免堆分配 | JIT/AOT | AOT |
| 边界检查消除 | 去 array bounds | 编译器/JIT/AOT | ✅（可证明的）/AOT |
| 循环优化（LICM/展开/OSR 进热循环） | 热循环提速 | JIT；OSR 跨层 | AOT 侧 |
| 类型反馈 + 投机优化 + deopt guard | 按观测特化 | JIT（+ deopt） | — |
| DCE / 存储消除 / 寄存器分配 | 通用 | 编译器/JIT/AOT（cranelift regalloc 已有） | ✅/AOT |
| code/bytecode flushing | 省内存 | 两引擎（见 tiered-execution §7） | ✅ |

> **分工原则**：确定性、无需反馈的（折叠/devirt/DCE/CSE/可证明 bounds 消除）→ **编译期做好 tier0**（§1）；需反馈/投机的（多态 IC 特化、投机内联）→ 运行时分层做。

---

## 5. 交叉引用
- 运行时分层 / OSR / 回收 / hot-reload：[tiered-execution.md](tiered-execution.md)
- 组件框架：[componentized-runtime.md](componentized-runtime.md)
- IR 指令集：[ir.md](../formats/ir.md) · 当前架构：[vm-architecture.md](vm-architecture.md)
