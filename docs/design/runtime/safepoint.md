# 统一 Safepoint / STW 协议（+ 精确 GC 契约）

> **状态：DESIGN（GC safepoint 已实施，泛化未实施）** · 创建 2026-06-21
>
> 把现有 **GC 专用**的协作轮询 safepoint 泛化成**统一安全点**：GC STW、tier OSR、load-context 卸载、hot-reload 帧迁移**共用一套**。并定义**精确 GC 对 safepoint / codegen 的契约**（GC map 只在安全点有效）。
>
> 消费方：[tiered-execution.md](tiered-execution.md)（OSR）、[load-context.md](load-context.md)（卸载）、[diagnostics.md](diagnostics.md)（safepoint/STW 事件）；对象布局/谁是 ref 见 #2 对象 ABI（待写）。

---

## 1. 现状（`gc/safepoint.rs`，add-gc-safepoint 2026-05-20，已实施）
- **协作轮询**：mutator 每次 `check_safepoint` 读 `gc_phase`，GC 要 STW 时 park（condvar + parked 计数）。
- **节流**：per-thread 计数器，每 N 次（默认 `safepoint_throttle=1024`，env 可调）才真做 Mutex poll → 热循环近零成本。计数器 fast path 是**普通 load/store 递减**（非原子 RMW）——`safepoint_skip` 每-mutator 单写（唯一跨线程写 `force_safepoint` 是 test/embedder-only），故 RMW 原子性对正确性不必要（inline-jit-safepoint-check 2026-08-01）。
- **两模式**：STW mark+sweep（默认）+ 并发（mutator 跑、写屏障、仅短 STW handshake）。
- **JIT 插桩**：fast-path 已**内联**（inline-jit-safepoint-check，2026-08-01）——`translate::emit_safepoint_check` 在 5 处 site（function entry / 后向 Br / BrCond / Call·CallIndirect 返回）emit 原生 `load + iadd_imm(-1) + store + brif`，替代 `jit_check_safepoint` helper call（~10ns→~1-2ns）；仅 counter 归零的 slow 分支调 `jit_check_safepoint_slow`。用普通 load/store（非 `atomic_rmw`）是关键：前一版 `atomic_rmw sub` 在 x86_64 Cranelift lowering panic 被 revert，load/store 形式从根因绕开。

→ 机制可用，但**硬绑 GC**（单一 `gc_phase`），且**无显式线程状态**。

## 2. 缺口
1. **只服务 GC**：OSR / context 卸载 / hot-reload 也要"停在安全点"，无通道。
2. **无线程状态**：线程卡在长 FFI 里无法 poll → **拖住 STW**；与 pinned（native 调用期对象不可移）交互未明确。

---

## 3. 泛化设计：统一 SafepointRequest
`gc_phase` → 通用请求：
```rust
struct SafepointRequest { kind: SafepointKind, target: Target }
enum SafepointKind { Gc(GcPhase), Osr(FuncId), ContextTeardown(CtxId), HotReload(..), Custom }
enum Target { All, Thread(ThreadId) }   // 全局 handshake / 单线程
```
- **复用** throttle + park + condvar + handshake；到达安全点按 `kind` dispatch handler（GC handler 原样保留）。
- `target=All` → 全局停（GC mark / context teardown / hot-reload）；`target=Thread` → 只停目标线程（OSR 只停跑该函数的线程）。

→ **一处机制、四处用**：GC STW、tier OSR、context teardown、hot-reload 帧迁移共用同一停车/握手。

## 4. 线程状态模型（HotSpot 式，新增）
每 mutator 显式状态：
| 状态 | 含义 | safepoint 视角 |
|---|---|---|
| `InVm` | 在 VM 内执行（解释/JIT 码） | 轮询；STW 时 park |
| `InNative` | 在 FFI/native 调用中 | **视作已在安全点**（不碰堆）；STW 无需等它；**返回时先 poll** 再进 `InVm` |
| `Parked` | 已停在安全点 | 等 resume |
- 修"长 FFI 拖 STW"。
- **pinned 交互**：`InNative` 期间该线程持有的 pinned 对象不可移；STW 移动式 GC 须避开 pinned（或 InNative 线程的 pinned 集登记为不可移根）。

## 5. 轮询覆盖
- **interp**：循环回边 + call/return（回边是 OSR 的天然点）。
- **JIT**：内联 poll（`load + sub + store + brif`，非原子 RMW；见 §1 JIT 插桩）；OSR entry 点 = 循环头 poll。
- **native 边界**：进出 FFI 切 `InVm`↔`InNative`。

## 6. 并发 GC 共存 + 请求并发（D4）
- **D4 采纳：单活动 safepoint 操作**（一次一个全局操作，GC / unload / hot-reload 互斥串行；请求排队）。并发 mark 期间的短 STW handshake 是其安全点窗口，其它全局操作排在其后。简单、无交错正确性陷阱。
- 单线程 OSR（`target=Thread`）可与全局操作不冲突时并行（不停世界）。

---

## 7. 精确 GC 与 safepoint 的契约（关键）

z42 追求**精确 GC**。精确的代价/约束**集中在与 safepoint + codegen 的契约**上（"谁是 ref 字段"的对象布局侧归 #2 对象 ABI）：

### 7.1 GC map 只在安全点有效
- 每个安全点需一份 **GC map / stack map**：哪些栈槽/寄存器在此刻是指针。
- interp / JIT / AOT **都要发** GC map（cranelift 支持 user stack maps → JIT 可精确；iOS/wasm 的 AOT 也必须发，否则那条线无法精确）。
- 推论：**GC 只能在安全点发生**（= 现状协作式），安全点之间状态可"不精确"（ref 可能只在未登记的临时槽）。

### 7.2 codegen 契约（最该盯）
安全点处**所有活引用必须在可识别、可更新的位置**：
- **派生/内部指针**（`LoadLocalAddr`、`&local`、`&arr[i]`）：须能从派生指针 recover base，或限制其跨安全点存活。**z42 最大契约风险点。**
- **禁止把指针藏成非指针**（tagging/xor/存成 int 再还原）——破坏精确性。
- **寄存器分配配合**：安全点活的 ref 不能只待在未登记槽。

### 7.3 代价 vs 收益（结论：净正，且对 z42 是必须）
- 代价：GC map 空间（JIT 码尤甚）+ 编译期算 map + 上面 codegen 约束。
- 收益：**moving/compacting → bump-pointer 分配（比保守 free-list 快）+ 局部性 + 小堆**；**无假保留**；分代。
- **对 z42 是硬前提**：保守 GC 下"一个像指针的 int 永久钉住对象/context 且诊断不出" → 直接毁掉 [load-context.md](load-context.md) 的可靠卸载 + `whyRetained`。**精确是 ALC 卸载/诊断的前提**。
- mutator 侧：精确本身不加每指令成本；运行期开销来自写屏障（分代/并发，已有）+ safepoint poll（已有）；**分配反而更快**。

---

## 8. 决策记录（2026-06-21，按讨论推荐采纳，可改）
| # | 决策 | 选择 |
|---|---|---|
| D1 轮询模型 | **协作轮询 only + 线程状态**（iOS/wasm 唯一可移植；不引入 signal） |
| D2 紧迫度 | GC 保留 throttle（容忍延迟）；**context unload / OSR 可 per-reason 降阈值/立即**响应 |
| D3 线程状态 | **采纳 `InNative=安全`**（修 FFI 拖 STW + pinned 正确性） |
| D4 请求并发 | **单活动全局 safepoint 操作 + 排队**（GC/unload/hot-reload 互斥串行）；单线程 OSR 可不停世界 |
| 精确 GC | **追精确**；契约 = GC map@安全点 + ref 可识别 + 派生指针受控（§7） |

## 9. 分阶段
1. `gc_phase` → 泛化 `SafepointRequest{kind,target}`，复用现有停车/握手。
2. 线程状态模型（InVm/InNative/Parked）+ FFI 边界切换 + pinned 对接。
3. JIT poll 插桩重做（reverted fast-path）+ OSR entry 点。
4. 精确 GC map：interp/JIT/AOT 发 stack map + 派生指针契约落地。
5. 接 tier OSR / context teardown / hot-reload 三个 handler。

## 10. 交叉引用
- OSR / tier 回收：[tiered-execution.md](tiered-execution.md) · context 卸载：[load-context.md](load-context.md)
- safepoint/STW 事件：[diagnostics.md](diagnostics.md) · 组件框架：[componentized-runtime.md](componentized-runtime.md)
- 对象布局 / 谁是 ref（精确 GC 的另一半契约）：[object-abi.md](object-abi.md) · 当前架构：[vm-architecture.md](vm-architecture.md)
