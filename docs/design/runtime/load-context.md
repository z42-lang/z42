# 加载上下文（Load Context）：zpkg 重载 / 卸载 / 内存回收

> **状态：Phase 1 地基 + Phase 2 惰性卸载已落地；强制清理 / 诊断仍 DESIGN** · 创建 2026-06-21
>
> **已落地**：Phase 1（`add-load-context-model`，2026-07-30）`AssemblyLoadContext` 运行时模型 + zpkg 身份
> `Std.Reflection.Assembly` + `Std.Type.IsCollectible`/`.Assembly`；Phase 2（`add-lazy-context-unload`，
> 2026-08-05）**惰性卸载**——`Unload()` 标 Unloading + GC major 回收无引用 context 的 arena（Erlang 等自然死、
> 反射对象计入保留边）。用户视角机制页见 [`docs/book/src/runtime/load-context.md`](../../book/src/runtime/load-context.md)。
> **未落地**：**强制卸载**（惰性 §8 已落，强制/tombstone 见决策修订）、`whyRetained` 诊断、跨 context 执行、hot-reload。
>
> **决策修订（2026-07-27，与 User 讨论）**：§9 决策 (b)"不做确定性强制卸载"已翻转——目标新增**强制内存清理**
> 轴（tombstone/trap 模型：STW → 间接槽改陷阱 + 活对象类型降 tombstone → 确定性 free 码/元数据大头，
> 类型身份 tombstone 随活实例惰性收尾）。§3 "整体卸载"放宽为**粒度可调**（context 单元大小由用户决定，
> 可细至单 zpkg / 单类型的 micro-context）。这两条在后续 change 落地。
>
> 本文设计 z42 的 **AssemblyLoadContext（ALC）式机制**：把 zpkg 加载进可卸载的上下文，实现代码重载更新，并对不再使用的已加载 zpkg **卸载、回收内存**（含运行时内部 metadata / 缓存池）。
>
> 复用 [tiered-execution.md](tiered-execution.md) 的回收+诊断基建（安全点 + 可达性 + 保留边注册 + `inspect_artifacts`），同一地基服务 **tier 回收 / hot-reload / zpkg 卸载** 三件事；组件框架见 [componentized-runtime.md](componentized-runtime.md)；落实 [vm-architecture.md](vm-architecture.md) "多实例 / ALC-like → 全局态 per-handle 化" 的延后项。

---

## 1. 目标
- 把 zpkg 加载进**上下文（context）**；同一逻辑 zpkg 可有新旧多版本上下文并存（重载更新）。
- 不再使用的上下文可**卸载并回收内存**——包括用户堆对象 **和运行时内部态（类型 metadata、method table、interned 串池、反射缓存、JIT 码 / interp tier 产物）**。
- 卸载失败时**能诊断"被谁引用了"**（.NET ALC 最大痛点的根治）。

---

## 2. .NET ALC 分析：哪些接受、哪些必修

| # | .NET ALC 问题 | z42 取舍 |
|---|---|---|
| 1 | 卸载非确定 / 合作式（GC 驱动，不能强制） | **接受**：z42 也走 GC 惰性卸载（不追求确定性强制卸载） |
| 2 | 引用泄漏静默阻止卸载 | **靠诊断治**（见 §5），不靠强制卸载 |
| 3 | **无法诊断"谁钉住"** | **必修，核心差异化**（§5）：z42 自有 GC → 可报告保留根 |
| 4 | 全有全无 + 跨 ALC 类型身份割裂 | **接受/特性**：context 整体卸载即可；`type identity = (context, type)`，新旧 context 类型分离是**重载所需的刻意特性** |
| 5 | native 资源不自动回收 | **修**：context 持有的 native 句柄 teardown 时显式释放（§6） |
| 6 | 静态状态丢失 | **接受 + 可选迁移钩子**（§7） |
| 7 | 执行中不能卸载 | **接受**：安全点 + 无活跃帧才回收（复用 tiered §6） |
| 8 | **内部运行时缓存泄漏**（反射缓存/Type 缓存钉住 ALC） | **修，重点**（§4）：内部缓存按 context 分区或弱引用，绝不自钉 |

> 一句话：z42 ALC ≈ .NET ALC 形态（惰性 collectible 卸载 + per-context 类型身份 + 整体卸载），**唯一也是关键的加法 = 内建保留根诊断 + 内部缓存不自钉铁律**。

---

## 3. 上下文模型
- **context = 加载/卸载单元**，粒度 = per-zpkg-version（或一组）；**整体卸载**（不追求单 zpkg 子卸载）。
- **父链 + root 共享契约**：core/stdlib 等不变契约放 **root context（永驻）**；可重载代码放各自 **collectible context**。
- **类型身份 = (context, type)**：新旧版本 context 的同名类型是**不同类型**（刻意，支撑版本并存）；跨 context 仅经 root 共享契约交互；重载时新版 context supersede 旧版（Erlang current/old 模型）。
- **per-context 全局态分区**（落实 vm-architecture 延后项）：GC heap 视图 / JIT code cache / type registry 按 context 分区，使"某 context 单独回收"可行。

---

## 4. 内部态与缓存池回收（重点）

### 两类内部态
1. **纯内部、不外露**（ClassDesc/TypeDesc、method table/vtable、字段布局、`Module.interned_strings` 串池、反射结果缓存、`Length/CharAt` 元数据缓存、method token/签名缓存）→ 放 **context-owned arena**。**非 GC 对象**，生命周期绑 context；context 判定可回收的那刻**随 arena 确定性 free**，不另走 GC。
2. **外露成 GC 对象**（`Type`/`MemberInfo`/`typeof` 结果）→ GC 对象，走 GC-lazy + 诊断（同用户堆）。

### 铁律：缓存自己不能变成那条泄漏边
.NET 最阴的内部泄漏 = 全局缓存强引用了 context 的类型数据 → 永久钉住。z42 强制：

> **任何 root/global 缓存，若按 context 数据建条目 → 要么 context-keyed 且 unload 时 purge，要么只持弱引用。绝不在全局缓存里强引用 context 的 metadata / Type。**

context 自己 arena 内的缓存随 arena 整丢，无所谓；跨 context / 全局缓存必须 context-aware。

### z42 现有内部缓存定性（均需 context-aware）
| 内部缓存/池 | 处置 |
|---|---|
| 类型 metadata（ClassDesc/TypeDesc）、method table、字段布局 | context arena，teardown 整丢 |
| `Module.interned_strings` 串池 | per-context arena，整丢 |
| 反射缓存（GetFields/Methods/Properties、attribute、MemberInfo） | context arena；全局二级缓存 → context-keyed purge |
| `Length/CharAt` 元数据缓存 | 绑类型 metadata，随之走 |
| cross-zpkg 调用缓存 `OnceLock<Arc<Function>>` | 注册表按 context 清 |
| `typeof`→Type 对象缓存 | 外露 GC 对象 → weak 或 context-keyed |
| method token / 签名解析缓存 | context arena |
| JIT 码 / interp tier 产物 | tiered §7 可回收 arena，按 context 标记 |

---

## 5. 保留根诊断（核心差异化）

### 为何 z42 能、.NET 不能
z42 **自有 GC**：可达性扫描时能记录"引用从哪来"；.NET GC 不暴露 root 来源，用户只能外部 profiler 硬挖。z42 做成内建、context-aware。

### 第 1 层：框架边报告（便宜，随时可查）
结构性保留边**有限可枚举**（都经组件框架注册槽），按 context 列出：cross-zpkg 调用缓存、类型注册表/方法表、observer/handler/lazy_loader 注册、泛型单态实例、JIT/tier 产物、**内部缓存边（类型 metadata 被全局强引用 / 反射缓存条目 / interned 串表持有者 / typeof-Type 缓存）**、native（dlopen 句柄 / FFI 回调 / 未解 pinned）。= Erlang `check_process_code` 的泛化。

### 第 2 层：堆保留路径报告（较重，按需经 GC walk）
抓普通用户代码持引用那类（.NET 式 static/event/闭包泄漏）：对 context 内仍可达对象，计算**从 context 外部到它的保留路径**，报出"哪个外部对象/线程栈帧/根 持有它"。
- 实现：倾向**平时零开销、诊断时 on-demand 反向可达**（profiler 式 retaining paths）；必要时 GC mark 顺带 root-source-tagged 出"外部入边集合"做常态轻量版。

### 暴露面
- 运行时 API：`Std.Diagnostics.whyRetained(ctx)` / `retainers(ctx)`（挂 `Std.Runtime` 族）。
- dev 工具：`z42vm --diagnose-context <id>`；或经 observer 在卸载迟迟不发生时自动 dump。
- 与 tiered-execution 的 `inspect_artifacts()` **统一同一保留边模型**。

---

## 6. native 资源（修 .NET #5）
context 持有的 native 扩展库句柄（ext.rs dlopen 的 `libz42_*`）teardown 时**显式 dlclose**；FFI/pinned 边界（`feedback_interop_pinned`：String/Array pinned 零拷贝给 native）卸载前**确保无在途 native 调用 + 解除 pinned**；native 侧持有的回调指针登记为保留边，未释放则诊断报出。

---

## 7. 静态状态迁移（.NET #6）
默认"卸载即弃状态"（明确，不静默）；hot-reload 需保状态时提供**显式迁移钩子**（旧 ctx → 新 ctx 搬迁），不像 .NET 完全甩给用户。

---

## 8. 卸载流程（惰性 GC + 诊断兜底）
1. `unload(ctx)`：标 collectible；新查找走新 ctx；清掉**本 ctx 主动持有的**结构边（自己的缓存/注册）。
2. GC 判定 context 可回收（无外部引用其 用户对象 ∪ 外露 Type/MemberInfo）→ **teardown**：① GC 回收用户堆对象；② **确定性 free context arena**（全部内部 metadata + 缓存池）；③ 释放 native 句柄；④ 清本 ctx 残留注册边。
3. **迟迟没回收？** → `whyRetained(ctx)` 列保留根（堆路径 + 结构边 + 内部缓存边）→ 定位清理。**不静默、不靠猜。**

铁律（§4）保证内部缓存不自钉 → 正常情况内部态随 context 干净回收；异常情况诊断兜底。

---

## 9. 决策记录（2026-06-21，与 User 讨论确定）
| # | 决策 |
|---|---|
| a | 接受 all-or-nothing 整体卸载；`type identity = (context,type)` 新旧分离是**特性**，不规避 |
| b | 接受 GC 惰性卸载（不做确定性强制卸载）；**但必须有保留根诊断**（§5） |
| c | **内部态/缓存池同等回收**（§4）：context arena 确定性 free + 缓存不自钉铁律 |

---

## 10. 交叉引用
- 回收 / 诊断 / 帧迁移 / hot-reload 共用基建：[tiered-execution.md](tiered-execution.md)
- 组件框架（observer / 注册槽 → 边可按 ctx 摘除）：[componentized-runtime.md](componentized-runtime.md)
- per-context 全局态 / ALC 延后项出处：[vm-architecture.md](vm-architecture.md)
- 诊断事件总线（`whyRetained` / context 生命周期事件 emit 于此）：[diagnostics.md](diagnostics.md)
- 卸载所依赖的统一 safepoint 协议 + 精确 GC（可靠卸载的前提）：[safepoint.md](safepoint.md)
