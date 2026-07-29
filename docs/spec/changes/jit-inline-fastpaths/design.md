# Design: JIT 内联去箱快路

状态：🔴 DRAFT（2026-07-29）

## Architecture

当前 JIT（`translate.rs`）对每条指令二选一：
- **原生 emitter**（`emit_i64_binop` 等）：reg_types 证明基元类型 → 直接读写 `frame.regs` 的
  24 字节 slot（tag@0 + payload@8），零 helper 调用。仅算术/比较/逻辑/常量/分支走这里。
- **helper 蹦床**：其余全部 `builder.ins().call(hr_xxx, ...)` 跨 extern-C 进 Rust helper,
  操作 boxed `Value`。

本 change 把 **array（Phase 4a）/ field（4b）** 加入原生 emitter 阵营,并引入 **去箱（4c）**。

## Decisions

### Decision 1：数组元素指针如何安全暴露给内联码（Phase 4a 核心）

**问题**：内联原生 load 需要 `elems: Vec<Value>` 的数据指针。但 `GcRef→RegionEntry→
Mutex<ArrayObj>→Vec` 这条链里,`parking_lot::Mutex` 与 std `Vec` 的字段偏移**不受 z42 控制、
不保证稳定**,硬编码偏移在 Cranelift 里是 unsound（版本升级即错/崩）。

**选项**：
- **A（推荐，先做）**：安全 helper `jit_array_data(frame, ctx, arr, *out_ptr, *out_len) -> u8`,
  用真实 Rust 类型取 `elems.as_ptr()` + `len`（安全,无偏移硬编码）。内联码:调一次拿 ptr/len →
  原生边界检查 → 原生元素 load（`ptr + i*24 + 8`）→ 去箱存 dst。**每 get 仍一次调用**,但省
  Value 往返 + 元素访问原生化。预计 ~1.5–2×（低于 3.3× 上限,因调用未消）。
- **B（逼近上限，后评估）**：JIT 识别 loop-invariant 数组,把 `jit_array_data` 提升到循环前,
  循环体内纯原生 load → 逼近 3.3×。需 translator 做不变量分析,复杂 + 风险高。
- **C（最激进）**：给 `RegionEntry` / `ArrayObj` 加 `#[repr(C)]` 固定布局 + z42 自控的稳定偏移,
  Cranelift 全程原生链。最快但侵入 GC 核心类型,风险最高。

**决定（建议）**：**先做 A**,实测真实收益。若 A 已达 ~2× 且满足需求,B/C 作为 Deferred。
若 A 收益不足且循环场景占比高,再评估 B。**不一上来做 C**（GC 类型布局风险）。

> 诚实标注:A 的「每 get 一次 helper」意味着它拿不到完整 3.3%。3.3× 是 B/C（消调用）才有的上限。
> A 是「低风险、验证方向、拿一半收益」的第一步 —— 与本项目「先测再投」纪律一致。

### Decision 2：去箱只限 drop-free 基元（i64/f64/bool/char）

元素/字段是 heap-ref（Str/Array/Object）时,裸 store 会漏 GC 写屏障 + Arc/Drop 处理 → 不安全。
故 Phase 4a/4b 内联**仅当 reg_types 证明元素/字段是 i64/f64/bool/char**,否则维持 helper。
（沿用 `emit_i64_binop` 的 `is_drop_free_primitive` 前提。）

### Decision 3：正确性 fallback

内联 emitter 只在 reg_types **确证**类型时发射;类型 Unknown / 非基元 → 走原有 helper。
运行期语义（边界越界抛异常、坏类型抛异常）必须与 helper 路径逐字一致 —— golden e2e 覆盖。

## Implementation Notes（Phase 4a）

- 镜像 `emit_i64_binop`（translate.rs:1313）的 slot 地址算术（`regs_base + idx*24`,payload@8）。
- 边界检查:`idx >= len` → 跳异常冷块（设 pending_exception + return 1）。复用 BrCond 冷边手法。
- `jit_array_data` 返回 `*const Value`（elems.as_ptr()）+ `len`。**生命周期**:指针只在本 get 内用,
  数组在 frame 存活期不被 move（GcRef 稳定）→ 安全。
- ArraySet 对称 + 若写入 heap-ref 仍走 helper（写屏障）。

## Testing Strategy

- **上限先测**（已做）：`total += a[j]` vs `total += j` = 3.31×（spike，见 proposal）。
- **实施后测**:同 harness 测 A 方案实际把数组循环从 834ms 降到多少;记录到 MODE-COMPARISON。
- **正确性 golden**：i64/f64 数组读写、越界异常、非基元元素回退、ArraySet 写屏障（heap-ref 元素）。
- **full GREEN**：`xtask test`（含 `test e2e --mode jit` JIT 腿）+ 自举不动点 + `cargo test gc`。
- **JIT==interp 等价**：jit-fixpoint-check（JIT 产物与 interp byte-identical / 结果一致）。

## 方案 B 具体设计（安全 loop-invariant 提指针，逼近 3.3×）—— 待实施

方案 A 实测 1.27×（每 get 一次 `jit_array_data` 调用无法被 Cranelift 提出循环）。方案 B 把
指针提到入口块一次取、循环体内纯原生 load。**关键是避免 null 数组的异常时机漂移**：

1. **非抛出提取 helper** `jit_array_data_opt(frame, ctx, arr, out_ptr, out_len)`：`regs[arr]`
   是数组 → 写 ptr/len；不是数组（含 null）→ `*out_ptr = null`（**不抛异常**）。
2. **可提取判定**：数组寄存器 `arr` 在函数内**从不被重新赋值**（从不作任何指令的 dst）。
   参数或 entry 块 set-once 满足。需一个 `written_reg(instr)->Option<u32>` 扫描（从 `max_reg`
   的 dst 匹配抽出复用）收集全部 dst 集合;`arr ∉ dst 集` ⇒ 可提取。
3. **入口块提取**：对每个可提取的 `arr`，在 cl_blocks[0]（支配全图）emit `jit_array_data_opt`
   → SSA `(ptr, len)`。SSA 定义在支配块 → 全函数可直接用，无需 Cranelift Variable。
4. **ArrayGet 内联改造**：`arr` 可提取时用提取的 `(ptr,len)`：
   - `ptr == null`（数组无效/null）→ 回退 `jit_array_get`（在**正确的访问点**抛异常，0 次迭代
     不误抛 → 无时机漂移）。
   - `ptr != null` → 纯原生 bounds + load + 去箱（**零 per-iteration 调用** → 逼近 3.3×）。
   - `arr` 不可提取（被重新赋值）→ 退回方案 A（per-get `jit_array_data`）。

**GC 安全性论证**：提取的 `ptr` 指向数组的 Vec 堆缓冲。z42 数组定长（ArraySet 原地写不 realloc）
→ 缓冲地址稳定；GC 是 mark-sweep **非移动** → 对象不搬迁；RegionEntry chunk Box 拥有、稳定。
故提取的 ptr 在函数执行期（跨 GC、跨 ArraySet 元素写）始终有效。len 同理（定长）。

**验证要求（实施时）**：jit==interp 逐字节（含被重赋值数组、null 数组 0 迭代、OOB）+ full GREEN
+ `test e2e --mode jit` + jit-fixpoint CI（byte-identical）+ `cargo test gc`（提取 ptr 跨 GC）。

**为何单列**：属高风险 codegen（支配/SSA/GC-ptr 有效性/重赋值分析），需专注实施 + 厚验证，
不在方案 A 同批草率合入。

## Deferred

- **jit-inline-B-hoist**：loop-invariant 数组指针提升（逼近 3.3× 上限）。触发:A 收益不足且循环密集。
- **jit-inline-C-repr**：GC 类型 `#[repr(C)]` 全原生链。触发:B 仍不足且确认值得动 GC 布局。
- Phase 4c 去箱、4d 调用内联：各自单列 spec。
