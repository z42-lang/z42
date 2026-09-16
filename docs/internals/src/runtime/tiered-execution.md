# 分层执行（Tiered Execution）：interp/JIT 各自分层 + OSR/deopt + 回收 + hot-reload

> **状态：DESIGN（目标架构，未实施）** · 创建 2026-06-21
>
> 本文设计 z42 运行时的**分层执行优化**：**每个执行引擎（interp、JIT）各自内部分层**，低层产物在被取代且无人引用时回收，并提供引用诊断。叠在 [componentized-runtime.md](componentized-runtime.md) 的组件框架之上；IR/特化层面的优化见 [ir-specialization.md](ir-specialization-design.md)；当前单态架构见 [vm-architecture.md](vm-architecture.md)。
>
> **实施触发**：性能/内存成为约束时。当前作为目标设计存在。

---

## 1. 范围澄清（关键）

**tiering = 每个引擎各自的内部分层优化**（interp 自己的 t0→t1、JIT 自己的 t0→t1），**不是** interp→JIT 跨引擎提升。

**为什么这样切（iOS 约束是硬的）**：iOS 禁运行时 JIT（无 W^X，不允许把生成码 mmap 成可执行）→ iOS 只能 **interp-only + AOT**（对齐现有 `ios = ["interp-only", "aot", …]` feature）。所以**不能把 JIT 当优化地基**——否则 iOS/wasm 无优化。**interp 必须能独立分层优化**，是 first-class 需求。interp→JIT 只是 JIT 可用平台（desktop/android）的**可选叠加轴**。

---

## 2. 业界对标

| 运行时 | 层次 | 借鉴点 |
|---|---|---|
| HotSpot(JVM) | interp + C1(L1-3) + C2(L4) | OSR；deopt；**code cache sweeper** 分阶段回收 |
| WebKit JSC | LLInt → Baseline → DFG → FTL | **OSR entry+exit** 经典实现 |
| V8 | Ignition → Sparkplug → Maglev → TurboFan | **bytecode/code flushing** 回收冷码 |
| .NET | Tier0(快JIT) → Tier1(优化)；.NET7+ 有 OSR | tier0/tier1 + OSR |
| Mono on iOS | **解释器 + AOT（无 JIT）** | **z42 iOS 直接对标** |
| CPython 3.11（PEP 659） | **自适应特化解释器（无 JIT）** | **z42 "interp 内部分层" 最贴近范本**：quickening + inline cache + 特化 opcode + 失配 deopt |
| Erlang | 模块 current/old 双版本 | **无引用即 purge + `check_process_code` 引用诊断** = z42 回收+诊断金标准 |

---

## 3. interp 内部分层（iOS 安全，零 codegen）

tier1 手段均不生成机器码（CPython PEP 659 路线）：
- **quickening**：首次执行后把泛型 opcode 改写成特化变体（`Call`→`CallCached`、`LoadField`→`LoadFieldAtOffset`）。
- **superinstruction**：融合常见 opcode 序列。
- **inline cache**：单态调用/字段缓存内嵌指令流。
- **特化 opcode**：`String.Length`→`StrLen`（详见 [ir-specialization.md](ir-specialization-design.md)）。

**形态决策（已定）**：tier1 = **独立优化指令流**；tier0 原始流在被取代且无活跃帧引用后**回收**（不走原地改写，因要拿到"回收 tier0"的内存收益）。迁移活跃帧靠 OSR（见 §6）。

---

## 4. JIT 内部分层（已定：两档 + deopt）

- **t0 baseline**：快产出、少优化。
- **t1 optimizing**：cranelift 多 pass。
- **deopt**：支持 optimizing→baseline 去优化（投机优化假设破时回退）。
- 重编 t1 后**回收 t0 机器码块**（受 deopt 依赖约束：仍可能被回退目标引用的不回收）。

---

## 5. interp→JIT（已定：可选轴，仅 JIT 平台）

desktop/android 上可在 interp 之上叠加"热函数提升到 JIT"一档；iOS/wasm 不启用（靠 §3 interp 内部分层 + AOT）。走同一套帧迁移基建（§6）。

---

## 6. 共用基建：safepoint + stackmap + deopt/OSR 元数据 + 帧迁移

**这是地基：一次建成、四处复用**——interp t0↔t1 OSR、JIT baseline↔opt deopt、interp→JIT、以及 **hot-reload 的帧迁移**（§8）。

### OSR 安全性论证（回应"OSR 安全吗、有无副作用"）
**安全、语义无副作用——前提三件套**（HotSpot/JSC/V8/.NET 已工程验证）：
1. **safepoint + 精确 stack map**：只在能恢复抽象状态的点（循环头、调用点）做 OSR/deopt。
2. **OSR/deopt 元数据**：低层↔高层状态双向映射（局部、求值栈、PC）。
3. **OSR 点优化约束**：被消除的值（标量替换对象等）必须可**再物化**（rematerialize），否则 deopt 重建不出。

- OSR entry（低→高，热循环跳进优化码）与 OSR exit（=deopt，高→低）共用同一元数据。
- "副作用"风险**只来自实现 bug**（状态映射写错），**非设计层语义危险**；转移的是精确程序状态。代价 = 维护元数据 + OSR 点限制部分激进优化。
- **结论**：安全且无副作用 → **可一次性把帧迁移基建做到位**，不临时拼。

---

## 7. 内存回收（对应"低层不用即回收 + 诊断引用"）

三种成熟模型组合：
1. **HotSpot sweeper 状态机**：被取代产物走 `not_entrant`（不再新进入）→ 无活跃帧/引用 → `flush`（释放）。**z42 tier0/baseline 回收照此**。
2. **V8 flushing**：长期未跑的高层产物可降级丢弃（bytecode/code flushing），下次按需重建——**冷码也回收**省内存。
3. **Erlang 引用诊断（金标准）**：old 版"无任何引用"才 purge；`check_process_code` 报告谁还引用。

### 回收机制
- tier 产物放**独立 code/metadata arena**（按函数/层），与对象 GC 堆分开。
- 提升时标 `superseded` + 记 epoch；**安全点回收 pass** 扫保留边，全无 → 释放；epoch/代际保证"跨过全局安全点、无线程持有"才真放。
- 提升后**立即丢 profiling 计数器**。

### 诊断接口 `inspect_artifacts()`（= Erlang `check_process_code` 等价）
列出每个可回收产物 `{函数, tier, 字节, 是否 superseded, 保留边}`，保留边枚举 z42 实际来源：
- **活跃栈帧**（扫调用栈）
- **per-site 调用缓存** `OnceLock<Arc<Function>>`（`cache-cross-zpkg-call-target`——典型隐性保留边）
- **异常 handler 表 / catch 落点**
- **observer / 调试钩子**（[observer.rs](../../../../src/runtime/src/observer.rs)）
- **lazy_loader 注册项**

用法：某 tier0"本该死却活着" → 工具直接指出"被 N 个调用缓存钉住"或"还有 1 个活跃帧"。**与 `feedback_leak_via_diagnostics` 哲学一致**（生命周期问题靠 runtime 诊断，不在语言层加标注）。诊断 API + arena 登记归 **libz42 基座**；各引擎组件注册自己的可回收产物。

---

## 8. hot-reload（与回收同源）

- **Erlang**：两版共存 + 无引用即 purge（最干净，z42 借鉴）。
- **JVM**：HotSwap / DCEVM / JVMTI RedefineClasses（旧帧跑完旧码，新调用走新码）。
- **.NET**：EnC + Hot Reload（MetadataUpdate）。
- **z42 共性基建** = 版本共存 + "旧版无引用才回收"（§7）+ 活跃帧处理（跑完/迁移，§6）。**与分层回收、帧迁移是同一套**——故 §6 基建一次建成即支撑 hot-reload。

---

## 9. 决策记录（2026-06-21，与 User 讨论确定）

| # | 决策 | 选择 |
|---|---|---|
| 1 | interp tier1 形态 + tier0 回收 | 独立优化流 + OSR 迁移活跃帧 → 立即回收 tier0（OSR 安全性见 §6） |
| 2 | JIT 内部分层 + deopt | baseline + optimizing **两档 + deopt** |
| 3 | interp→JIT 提升 | **可选轴，仅 JIT 平台**；iOS/wasm 靠 interp 内部分层 + AOT |
| 4 | tier0 基线质量 | **编译期 IR 优化使 tier0 即高性能**，不靠后续 tier 兜底（见 [ir-specialization.md §1](ir-specialization-design.md)） |

---

## 10. 分阶段实施（post-ROI）

1. **帧迁移基建**（§6）：safepoint + stackmap + deopt/OSR 元数据 + 版本回收 arena —— 地基先行。
2. **interp 内部分层**（§3，PEP 659 路线）+ §7 回收 + `inspect_artifacts` 诊断 —— iOS/wasm 也受益，优先。
3. **JIT 两档 + deopt**（§4）。
4. **interp→JIT 可选轴**（§5）。
5. **hot-reload**（§8，复用 §6 基建）。

---

## 11. 交叉引用
- 组件框架（引擎作为组件、observer/注册基座）：[componentized-runtime.md](componentized-runtime.md)
- IR 优化 / 特化 / intrinsic / tier0 基线质量：[ir-specialization.md](ir-specialization-design.md)
- zpkg 加载上下文 / 重载 / 卸载回收 / 保留根诊断（复用本篇 §6/§7 基建）：[load-context.md](load-context.md)
- 诊断事件总线（`inspect_artifacts` / 编译·deopt·OSR·tier 事件 emit 于此）：[diagnostics.md](diagnostics.md)
- 统一 safepoint / STW 协议（OSR/回收所依赖的安全点）+ 精确 GC 契约：[safepoint.md](safepoint-design.md)
- AOT 后端（作 baseline 档与 JIT 共存；iOS/wasm 的优化档）：[aot.md](aot.md)
- 当前单态架构：[vm-architecture.md](vm-architecture.md)
