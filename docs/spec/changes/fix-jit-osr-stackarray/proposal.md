# Fix: JIT 数组 helper 未处理 StackArray → OSR 下 stack-alloc 数组崩

> 状态：IMPL（bug fix，纯 runtime，无格式 bump、自举字节不动）。
> 发现：`Z42_OSR_THRESHOLD=1` 全 e2e 压测（jit-unbox-regalloc 2C 验证期）撞到；2B/main 二进制同样复现，
> 与 2C 无关——是**预存在**的 JIT/OSR + escape-analysis 交互 bug。

## 症状

`Z42_OSR_THRESHOLD=1`（强制每循环走 OSR）下跑完整 e2e，golden 重生工具（z42 代码）抛
`ArraySet: expected array, got StackArray { idx: 0, frame_id: 39503 }`，regen 中止、e2e EXIT=1。
默认阈值（1000）下不触发，故平时门禁绿、latent。

## 根因

逃逸分析栈上分配（escape-analysis-stack-alloc）把不逃逸的局部数组分配进 **interp 的 per-context
stack arena**，值为 `Value::StackArray { idx, frame_id }` 句柄。**interp 的 `exec_array` 三个
访问器（get/set/len）都处理 StackArray**（经 `ctx.stack_arena` 解析），但 **JIT 的对应 helper
（`jit/helpers/array.rs` 的 `jit_array_get`/`jit_array_set`/`jit_array_len`）只处理
`Value::Array`、把 StackArray 落到 `other =>` 报错**。

平时不出问题：非 OSR 的 JIT 函数从头在 JIT 跑，其 `ArrayNew` 走堆分配（不产 StackArray）。
**但 OSR 是 interp→JIT 中途切换**：interp 段先创建了 StackArray（在 `frame.regs` 里），回边 OSR
进 JIT 后，JIT 代码访问该数组 → 命中未处理 StackArray 的 helper → 崩。

（JIT 的**原生内联**数组快路径不受影响：其 hoist helper `jit_array_data_opt` 对非
`Value::Array` 返回 `ptr=null,len=0`，把每次访问路由到冷 helper——所以修 helper 即全覆盖。）

## 修复

给 `jit_array_get`/`jit_array_set`/`jit_array_len` 各加一条 `Value::StackArray` 臂，**逐行镜像
interp `exec_array`**：经 `vm_ctx_ref(ctx).stack_arena.lock().with_arr`/`with_arr_mut(aidx,
frame_id, …)` 解析读写/长度；set 不发 GC write barrier（栈槽非堆槽，栈数组的堆-ref 元素由 arena
root scan 保活），与 interp 一致。at-most 语义、越界/stale-handle 错误经 `set_exception` 抛出。

## 验证

- **直接**：`Z42_OSR_THRESHOLD=1 xtask test e2e` 从 EXIT=1（regen 崩）→ **490 passed, 0 failed**
  （全 490 goldens 在强制 OSR 下逐字节 == interp）——同时**首次**把 2C 的 OSR 驻留路径压测覆盖到全套。
- **回归**：normal `xtask test e2e` 490/0 + 自举 5/5 gen1==gen2 + stdlib + cargo --lib。纯新增
  StackArray 臂、不改 Array 路径 → 默认路径零影响。

## 相关

- 逃逸分析栈分配：`escape-analysis-stack-alloc`（interp arena）。
- OSR：`add-osr-loop-tiering`（interp→JIT，`from_interp_regs` 拷 `frame.regs`）。
- **潜在同类（已核实存在、留 follow-up）**：JIT 的**对象**字段 helper（`jit_field_get`/`jit_field_set`,
  `helpers/object.rs`）同样**只处理 `Value::Object`、不处理 `Value::StackObject`**（逃逸分析同样栈分配
  对象，见 escape-analysis-stack-alloc「对象+数组」）。理论上 OSR 下栈对象被 JIT 字段访问会同样崩。
  **但**：① 本 fix 后全 490 goldens 在 `Z42_OSR_THRESHOLD=1` 下 490/0，未触发对象路径；② 手工构造
  `new P(...)` 局部对象在循环里字段访问也未复现（ctor 调用使对象逃逸→堆）；③ interp 的 StackObject
  字段路径远比数组复杂（byte-aware 字段解析 + field-IC/PIC），镜像进 JIT 字段 helper**不是**数组那样的
  1:1，需先能复现才能可靠验证。故对象侧**不在本 fix**（不上无法验证的复杂代码），作独立 follow-up：
  先构造能触发的 stack-object-in-OSR-loop 用例，再镜像 interp 的 StackObject 字段逻辑（含 IC）。
