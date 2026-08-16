# Tasks: 统一 GC 堆模型（变长 GC 分配器 A'）

> 状态：6.5 gate 已过，PR-1 完成 | 创建：2026-08-15
> 终点 = 单一 GC 堆（string/closure/array 变长数据都进 GC 变长分配器）；内部分 5 PR，每 PR 独立 GREEN + rebase。
> User 三决策：动机=架构统一/为移动 GC 铺路（接受 GC 压力权衡）；分配器=A'；scope=全包。
> **⚠️ 执行顺序已重排（D10，2026-08-15）**：closure→array→string（string 分配管线最难，放最后；见下）。

## 进度概览（执行顺序）
- [x] **PR-1**: 变长 GC 分配器原语（inert，无消费者，Miri 门禁）—— 本地全绿 commit a0fbd79d（+ PR-2a drop-glue 3550e869）
- [x] **PR-2**: delegate/closure 进 GC（+ heap 接线 2.0）—— 本地全绿 commit 5c910cd6（cargo 965 + Miri + self-host 5/5 逐字节 + e2e closures 12/delegates 26/gc 20 全绿）
- [x] **PR-3**: array backing 进 GC（`ArrayBacking` Vec→GC 变长块；packed/struct[] 全迁）—— 本地全绿（cargo 965+21 + Miri var_region 14 + array/struct 块访问 types_tests 31 clean 0 UB + self-host 5/5 逐字节 + xtask test GREEN 全 stage C#-free）。StackVec 变体保留逃逸分析栈数组非 GC；删 derive(Clone)→deep_copy；两块 struct[]（bytes ArrayStruct POD + refs ArrayValue）
- [x] **PR-4**: string 进 GC（最难：弥散 `.into()` + lazy per-ctx interning + ambient 堆 D11）✅ 2026-08-16 — 单一堆 payload 闭合
- [x] **PR-5**: 收敛 + 文档 ✅ 2026-08-17 —— ① 删 interned_strings/JitModuleCtx.string_pool 死代码链（d6ac7831）② ClosureData.fn_name String→GC Str，闭包全 POD 删 BlockType::Closure drop-glue（a48a6401）③ trace_children/scan_object_refs 合并为 visit_gc_children(for_marking) 单一访问器（4290aef5）④ docs（gc.md「变长块堆」机制页+mermaid、object-abi §5/§6、roadmap）。**事实校正**：frame `Arc<str>`（诊断元数据、非 Value::Str payload、perf-frame-name-precompute 热路径）**保留不迁**；element_type `Arc<str>` 延后

> 重排理由（D10）：closure/array 创建点全持 `ctx`（`ctx.heap()` 可分配），string 创建弥散在无 ctx 自由上下文 + interned 串加载期建（堆未存在）→ string 是**分配管线最难**而非最易。先用 closure 把变长分配器→mark→trace→sweep 全链路 battle-test，string 放最后。原 PR-2(string)/PR-3(closure)/PR-4(array) 编号已按新序调整。

---

## PR-1: 变长 GC 分配器原语（自包含、inert）✅ 本地完成
> **scope 收敛（D9）**：PR-1 = 自包含 `VarRegion` 分配器 + Miri-clean 单测；**heap 接线（ArcMagrGC region_var + mark/sweep 驱动 + write-barrier + alloc_var_block）移到 PR-2 首步**（与首个消费者 string 同落地，可被真实分配驱动验证；inert 无消费者不加死代码）。
- [x] 1.1 `GcBlockHeader`（16B，repr(C,align8)：generation/size/marked/alive/type_tag/size_class；叶子无 Mutex/finalizer/soft_ref）；`DATA_OFFSET=16` 使 payload 8-aligned（镜像 vstr StrHeader）
- [x] 1.2 `VarRegion`：16-aligned 原始 chunk（`alloc(Layout)`，非 `Box<[u8]>` 因需对齐）+ bump 分配 + 2 的幂 size-class free-list 回收 + oversized 专用 chunk
- [x] 1.3 `alloc(payload, BlockType) -> VarGcRef`（零初始化 payload）/ `resolve`（generation guard）/ `tombstone`（bump gen + 入 free-list）/ `iterate_alive`（走稳定 block 列表）/ `sweep`（STW，未 mark→tombstone）/ `live_count`
- [x] 1.4 `VarGcRef` = **type-erased 8B 标记指针**（64 位低 48 地址+高 16 gen；wasm32 cfg-gate `{ptr,gen}`；`payload()/payload_mut()`；size 断言 8B + Option 8B niche）
- [x] 1.5 单测 13 例（各 size / 空 payload / 零初始化 / 地址稳定 / tombstone stale / free-list 同类复用 + gen bump / ABA / sweep mark 保留 / iterate / oversized 专用 chunk / chunk 跨界增长 / 全 BlockType）
- [x] 1.6 **Miri 全绿**（`-Zmiri-permissive-provenance`）——抓到并修复 payload 派生 SB UB（D8）；strict-provenance `map_addr`/`expose_provenance` 镜像 refs.rs
- [ ] 1.7 GREEN 收尾：`cargo test --lib` 全量 + `cargo test --tests --no-run` + `cargo bench --no-run`（.slots 教训）+ `xtask test`（自举字节不动，纯 runtime inert，无格式 bump）+ `cargo build --release --bin z42vm`

> **决策已定（6.5 gate，2026-08-15）**：① size-class 分档 = **2 的幂次**；② `GcRef` 指异构变长块 = **type-erased 句柄**（User 拍板，用 `type_tag` 运行期区分）；③ 变长 region **初版仅 STW**（generational 后补，减小 PR-1 面）。

## PR-2: delegate/closure 进 GC（+ heap 接线，最简先跑通全链路）
- [ ] 2.0 **heap 接线（从 PR-1 移入，D9）**：`ArcMagrGC` 加 `region_var: Mutex<VarRegion>`；`mark_phase`/`mark_if_unmarked` 遇 `VarGcRef` 按 type_tag mark + trace-hook 扫 payload（closure 扫 env + fn_name 边）；`sweep_phase` 驱动 `region_var.sweep()`；`alloc_var_block(size, type_tag)` on heap（走 `ctx.heap()`）+ write-barrier 接口——**由 closure 真实分配驱动验证**
- [ ] 2.1 `ClosureData` 进 GC 变长块（payload = env GcRef + fn_name）；`Value::Closure(Box<ClosureData>)` → `Closure(VarGcRef)`（8B）；payload 派生走原始 NonNull（D8 铁律）
- [ ] 2.2 `is_heap_ref` Closure=true 不变；`trace_children`/`scan_object_refs` closure 臂扫变长块 payload 的 `env`（GcRef<ArrayObj>）边（+ fn_name 若已 GC string；PR-4 前 fn_name 暂留 String/Str）
- [ ] 2.3 `StackClosure` 保持栈（不进 GC，逃逸分析产物）——确认边界（同布局程序 StackObject/StackArray 规则）
- [ ] 2.4 ~28 Closure 构造点迁移（`interp/exec_call.rs:319/339`、`jit/helpers/closure.rs:88/99`、`corelib/object.rs:158`）——创建点都有 `ctx`/`vm_ctx_ref`，走 `heap().alloc_var_block`
- [ ] 2.5 GREEN：cargo --lib + 集成/bench 编译（.slots）+ Miri（heap 接线 + closure 块 unsafe）+ `xtask test`（self-host 逐字节，**评估无格式 bump**）+ e2e closure 生命周期/捕获/GC 回收
- [ ] 2.6 benchmark：closure-heavy workload GC 压力（量化）

## PR-3: array backing 进 GC（最大最敏感）✅ 完成
- [x] 3.1 `ArrayBacking` 各变体元素缓冲 → GC 变长块内联：`Boxed{block,len}`（ArrayValue）/ packed `Bytes/Bool/I32/I64/Chars/F64{block,len}`（ArrayPrim）/ `StructBytes{elem_size,len,bytes,refs,layout}`（**两块**：bytes ArrayStruct POD + refs ArrayValue）+ **StackVec(Vec)**（逃逸分析栈数组保持非 GC arena，D13）
- [x] 3.2 `ArrayObj` 头留 `region_array`（Mutex 保护），元素块在 `region_var`、由头唯一拥有、经 borrow/borrow_mut 锁访问 = 可变安全；`len` 内联 backing（定长不脱同步）
- [x] 3.3 trace：`trace_children` Array/RefArray/StructRefHeap 臂 `arr.mark_backing()` 标块（覆盖 STW/minor/concurrent）；`gc_refs()` 走块（Boxed 全切片 / StructBytes refs 块 / packed &[]）；`slice_of`/`slice_of_mut` payload 派生自原始头指针（D8）
- [x] 3.4 packed 基元数组 / `struct[]`（`struct_bytes`/`struct_refs_mut`/`write_struct_elem` 暴露子区）/ StackArray（StackVec 保持栈）边界；构造收敛 `&dyn MagrGC` 参数走 `ctx.heap()`/`self`；删 derive(Clone)→`deep_copy(heap)`；删无调用方 boxed_slice/clear/capacity
- [x] 3.5 write-barrier array 元素写不变（存进块）；`packed_num_ptr` 返块 payload 指针（非移动，JIT hoist 缓存不变）
- [x] 3.6 GREEN：cargo --lib 965+21 + tests/bench 编译 + Miri（var_region 14 + array/struct 块访问 types_tests 31 clean 0 UB）+ `xtask test` self-host 5/5 逐字节 + 全 stage GREEN（C#-free）+ e2e（数组/struct[]/gc/closures 经 xtask e2e stage 覆盖）

## PR-4: string 进 GC（最难：弥散分配 + 加载期 interning + safepoint）✅ 完成（2026-08-16）
> **架构方向已定（D11）= 堆作为一等 ambient 分配服务**（CLR/JVM 模型；非 188 处线程穿透）。as-implemented 见 design.md §4。
- [x] 4.0 ambient 堆：新增专用 `gc/ambient.rs`（`current_heap()` + `HeapGuard`，每帧/JIT run 设 thread-local；未重载 native-interop 的 `CURRENT_VM`）；接入 `exec_function` + `jit::run_fn`，覆盖 interp+JIT
- [x] 4.1 GC string 块布局（`BlockType::Str`，`{GcBlockHeader, UTF-8 bytes}`，len=header.size）；`Str` → `VarGcRef`，删 `strong` 原子计数 + Arc drop；`alloc_str` on `MagrGC`/`ArcMagrGC`（payload 派生走原始 NonNull，D8）
- [x] 4.2 **lazy per-ctx interning**（改初稿：加载期无堆 → 不物化）：`build/populate_interned_strings` no-op；`ConstStr` 首次经 `VmContext::intern_const_str` 活堆分配 + `interned_cache`（`(module,idx)` 键）；external root scanner 扫缓存；interp `const_str` + JIT `jit_const_str` 同源
- [x] 4.3 **safepoint 纪律**：查明**几乎免费**——`maybe_auto_collect` 只置标志、延到 safepoint（默认 `max_bytes=None` 全不 auto-collect）→ 临时 string 落 reg 前天然安全（frame regs 已被 root scanner 扫），与既有 Object/Array 同不变式
- [x] 4.4 `Value::Str(Str)`→`VarGcRef`（8B）；mark_phase + mark_if_unmarked 加 `Str`/`FuncRef` 臂；`is_heap_ref(Str/FuncRef)=true`（写屏障）；`value_heap_ptr` Str 臂；string 是叶子 trace 无出边；str_meta 加 `is_live()` 世代守卫
- [x] 4.5 ~189 处 `.into()` 保持不变（ambient 堆）；`fn_name`/`element_type`/frame `Arc<str>`（Rust 内部 bookkeeping，非 `Value::Str`）**顺带迁延 PR-5 收敛**（不影响 string-payload 进 GC 闭合）
- [x] 4.6 benchmark（string-heavy 实测）：**吞吐 1.76× 更快**（消除原子 refcount）+ 峰值 RSS +13%（默认不 auto-collect 累积）——预期代价实为吞吐净赢；GREEN cargo 970 + Miri 14/0 + `xtask test` self-host 5/5 逐字节 + e2e `gc_string_survives_collect`（interp+jit）

## PR-5: 收敛 + 文档 ✅ 完成（2026-08-17）
- [x] 5.1 删 interned_strings/JitModuleCtx.string_pool 死代码双路径（PR-4 lazy interning 后 write-only）；收敛 `trace_children` vs `scan_object_refs` → 单一 `Value::visit_gc_children(for_marking, …)`（mark 委托 true、枚举委托 false，逐臂等价）
- [x] 5.2 `ClosureData.fn_name` String→GC `Str` → 闭包块全 POD → 删 `var_drop_glue` 的 `BlockType::Closure` 分支（region_var 现仅 `ArrayValue` 需 finalizer）；mark 经 visit_gc_children 统一（trace_children 新增 fn_name 边）。**frame `Arc<str>` 保留**（诊断非 payload，事实校正）；`element_type` 延后
- [x] 5.3 docs：`gc.md`「变长块堆」机制页（A' 变长分配器 + block layout + mermaid + safepoint/ambient/lazy-intern 原理）+ Phase 路线行 + 字符串脚本化动机更新；`object-abi.md` §5（PR-5 收敛落地 + frame Arc<str> 保留说明）；`roadmap.md`（unify-gc-heap 行 + 标 string-全-GC follow-up 完成）。`gc-handle.md` 不涉（其为 Std.GCHandle 用户 API，非内部 VarGcRef）
- [x] 5.4 GREEN 全绿 + change 容器归档（见 §归档）

---

## 全局验证纪律（每 PR）
- 改 runtime 前 `cargo build --release --bin z42vm`（xtask warm-skip 不重建 → 陈旧假红）。
- 改 ScriptObject/TypeDesc/Value/Region 后须 `cargo test --tests --no-run` + `cargo bench --no-run`（--lib 不编集成/bench，.slots 教训）。
- 变长分配器 unsafe → Miri/ASAN 是硬门禁。
- 纯 runtime、无格式 bump（逐 PR 复核）→ self-host 5/5 gen1==gen2 逐字节应不动；动了说明踩到序列化面，停下查。
- benchmark（string/array-heavy）量化 GC 压力回归——这是本程序**预期的短期代价**，量化而非追求零回归。
