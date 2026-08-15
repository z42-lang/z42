# Proposal: 统一 GC 堆模型（string / delegate / array-backing 收进 GC 变长分配器）

## Why

z42 的托管数据现在走**两套内存管理器并存**：

- **GC 堆**（`Region<T>` 定长槽 + mark/sweep + generation）：只管 `ScriptObject` + `ArrayObj` 两类**定长头**。
- **GC 外自管**：
  - **string** = `Value::Str(Str)`，`Str` = 手写 thin-Arc-DST（`metadata/vstr.rs`，`StrHeader{strong,len}` + 内联字节，**原子引用计数**，完全在 GC 外）。
  - **delegate/closure** = `Value::Closure(Box<ClosureData>)`（外部 Box）；`FuncRef(Str)`（名字走 Arc string）。
  - **array 元素数据** = `ArrayObj` 的 `ArrayBacking` = `Vec<Value>` / `Vec<u8>` / `Vec<i64>`…（**外部 malloc Vec**，GC 只跟 `ArrayObj` 句柄、不跟元素缓冲区）。

后果：① **双重内存管理**——同一个逻辑堆里，一半对象归 tracing GC、一半归 Arc/Box 引用计数 + 外部 Vec，语义与调优点分裂；② **无法走移动/压缩 GC**——只要有 Arc/Box/Vec 持裸指针，就无法整体搬迁对象、无法做 string 去重/压缩、无法统一堆调优；③ 与 CLR/JVM 的**单一托管堆**模型不一致——那些运行时里 string/delegate/array 全是普通 GC 对象。

`unify-object-byte-layout`（已完成，2026-08-15）已把地基铺好：`Value` = 16B、对象/struct/string/引用**句柄**统一 8B、`GcRef` = 8B 标记指针、`ScriptObject`/`ArrayObj` 字节化字段布局。**唯一剩下的例外是「变长 payload 数据本身」**（string 字节、closure 数据、array 元素缓冲区）仍在 GC 外。本变更把这三类变长数据**收进 GC**，达成单一堆管理器。

## What Changes

**终点 = 单一 GC 堆**（User 2026-08-15 裁决「数组、string、delegate 等都是适应 GC 堆来分配」）：

1. **变长 GC 分配器（方向 A'，User 裁决）**：把 `Region` 泛化/新增**变长块分配能力**——一个 GC 对象 = 单个变长块 `{GcBlockHeader, inline payload…}`，`GcRef` 8B 细指针指它，**单次分配 + 内联数据**（等价现 vstr 的紧凑度，但纳入 mark/sweep/generation）。array / closure 复用同一分配器。
2. **string 进 GC**：`Value::Str(Str)` → `Value::Str(GcRef<StrHeader>)`（或等价 GC 细指针）；`StrHeader` 的 `strong: AtomicUsize` 引用计数**换成 GC 块头**（mark/alive/generation）；interned 串池变 GC roots；删 vstr 的 Arc drop 路径。
3. **delegate/closure 进 GC**：`Value::Closure(Box<ClosureData>)` → `Closure(GcRef<ClosureData>)`；`ClosureData.fn_name` 与 `FuncRef` 名字复用 GC string；`StackClosure`（逃逸分析栈产物）**保持栈分配、不进 GC**。
4. **array backing 进 GC**：`ArrayBacking` 的 `Vec<…>` 元素缓冲区搬进 GC 变长块（数组元素数据也在 GC 堆，GC 直接扫元素而非透过外部 Vec）。packed 基元数组 / `struct[]` 内联布局一并迁移。
5. **收敛**：删除 Arc/Box/外部 Vec 与 GC 的双路径残留；单一堆的 mark/sweep/write-barrier/统计口径统一。

**交付纪律（事实校正，仿布局程序，User 已认）**：终点锁死「单一 GC 堆」不中途停，但**实现拆 5 个内部 PR，每个独立 GREEN + rebase**，不做「单个巨红不可回退 PR」（违反 workflow 阶段 8）。

## ⚠️ 已对 User 摆清并接受的权衡（事实校正责任）

**本程序主要是「架构统一 + 为移动/压缩 GC 铺路」，不是即时性能优化——短期很可能是性能/内存的净负担。User 2026-08-15 明确「要的是架构统一 / 为移动 GC 铺路，接受短期 GC 压力」。**

- **失去确定性释放**：Arc(string) / Box(closure) 现为**引用计数即时释放**；纳入 tracing GC 后只在 GC 周期释放 → 浮动垃圾↑、内存峰值↑、GC 频率/停顿↑。**string-heavy workload（z42c 自编译本身就是）最敏感**。
- **与「减分配是最大杠杆」张力**：性能主线指出 malloc≈31% 是最肥靶。把更多东西塞进 GC 可能增加 GC 工作量，除非统一分配器本身比 malloc+Arc 快。收益是**长期架构**（移动/压缩 GC、string dedup、单一堆调优点），不是短期吞吐。
- **string 是不可变叶子**：tracing GC 对它**零环收益**（不形成循环引用），纯为「统一」而纳入。

## Scope（允许改动的文件）

> 本表为**程序总 Scope**；具体某个 PR 只触及其子集，PR 描述里再收窄。以 Explore 测绘为准，实施中细化。

### 运行时（Rust VM）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/region.rs` | MODIFY/NEW | 变长块分配能力（size-class / bump over 字节 chunk）+ 块头 + mark/sweep/generation 集成 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | 新增变长 region 驱动；alloc/mark/sweep/scan/write-barrier/统计接通变长块 |
| `src/runtime/src/gc/refs.rs` | MODIFY | `GcRef<StrHeader>` / 变长块的细指针支持（复用 8B 标记指针）|
| `src/runtime/src/gc/heap.rs` / `types.rs`（gc） | MODIFY | 堆 trait / finalizer / trace 签名（若需）|
| `src/runtime/src/metadata/vstr.rs` | MODIFY/DELETE | `Str` Arc → GC 块头；删 refcount drop 路径 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `Value::Str`/`Closure`/`FuncRef` payload 迁 GcRef；`ClosureData`/`ArrayObj`/`ArrayBacking` 数据迁 GC；`trace_children` 扫变长 payload |
| `src/runtime/src/interp/*.rs` | MODIFY | string/closure/array 构造与访问点（~287 Str + ~40 Closure + array 元素读写）|
| `src/runtime/src/jit/**` | MODIFY | string/closure/array 元素访问 helper + interned 串池 + 数组元素 stride |
| `src/runtime/src/corelib/*.rs` | MODIFY | String/Array/Delegate 反射与内建触达点 |
| `src/runtime/src/host/*` / `native/marshal.rs` | MODIFY | FFI marshal 的 string/array 表示（评估）|

### 文档 / 测试

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/design/runtime/gc.md` / `gc-handle.md` | MODIFY | 变长 GC 分配器 + 单一堆模型；A' 设计原理 |
| `docs/design/runtime/object-abi.md` | MODIFY | §5「字符串进 GC」从 interim 提为落地；§6 移动 GC 预留衔接 |
| `docs/roadmap.md` | MODIFY | 「统一堆」子目标状态更新 |
| `src/runtime/src/gc/region_tests.rs` 等 | NEW/MODIFY | 变长分配器单测（含 Miri/ASAN 敏感）|
| `src/tests/e2e/…` | NEW | string/closure/array GC 生命周期 + 保留图 + 循环回收端到端 |

## Out of Scope

- **移动 / 压缩 GC**（forwarding / card table 搬迁 / young-old 分代压缩）——本变更把变长数据**纳入 GC 分配器**为其铺路，但仍保持**非移动 region GC**。移动 GC 是独立后续程序。
- **string 去重 / interning 优化**——统一堆后可做（GC 可见所有 string），但属后续。
- **B 方向（单一 size-classed 统一堆、废类型化 region）**——A' 达成「统一堆」实质后，B 作为收敛目标另议。
- **AOT 后端**（interp 全绿前不碰；JIT 在本变更内更新）。

## Open Questions（design 阶段定，非阻塞）

- [ ] 变长块 GC 头设计：现 `RegionEntry<T>` 头很重（`Mutex<T>` + marked + alive + gen_age + generation + finalizer + location + soft_ref_count）。变长块（尤其 string 叶子）该用多轻的头？leaf（string，无引用、无 finalizer）能否省 finalizer/soft_ref 槽？
- [ ] size-class vs bump-over-byte-chunk：变长 region 内部布局取哪种？回收碎片 vs 简单性权衡。
- [ ] string-heavy（z42c 自编译）纳入 GC 后的 GC 频率/内存峰值实测——benchmark 门禁。
- [ ] closure `env: GcRef<ArrayObj>`（已 GC）与 `ClosureData` 本体进 GC 后的 trace 边。
- [ ] 无格式 bump 确认：string/closure/array 的**运行时表示**变，zbc/zpkg 序列化格式应不变（同布局程序 PR-3/4/5 纯运行时）——逐 PR 复核。
