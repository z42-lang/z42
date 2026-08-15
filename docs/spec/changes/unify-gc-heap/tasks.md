# Tasks: 统一 GC 堆模型（变长 GC 分配器 A'）

> 状态：6.5 gate 已过，PR-1 完成 | 创建：2026-08-15
> 终点 = 单一 GC 堆（string/closure/array 变长数据都进 GC 变长分配器）；内部分 5 PR，每 PR 独立 GREEN + rebase。
> User 三决策：动机=架构统一/为移动 GC 铺路（接受 GC 压力权衡）；分配器=A'；scope=全包。
> **⚠️ 执行顺序已重排（D10，2026-08-15）**：closure→array→string（string 分配管线最难，放最后；见下）。

## 进度概览（执行顺序）
- [x] **PR-1**: 变长 GC 分配器原语（inert，无消费者，Miri 门禁）—— 本地全绿 commit a0fbd79d（+ PR-2a drop-glue 3550e869）
- [x] **PR-2**: delegate/closure 进 GC（+ heap 接线 2.0）—— 本地全绿 commit 5c910cd6（cargo 965 + Miri + self-host 5/5 逐字节 + e2e closures 12/delegates 26/gc 20 全绿）
- [ ] **PR-3**: array backing 进 GC（`ArrayBacking` Vec→GC 变长块；packed/struct[] 全迁）
- [ ] **PR-4**: string 进 GC（最难：弥散 `.into()` + 加载期 interning + safepoint；走 ambient 堆最本质方向 D11）
- [ ] **PR-5**: 收敛 + 文档（删双路径残留；gc.md/object-abi/roadmap 收口）

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

## PR-3: array backing 进 GC（最大最敏感）
- [ ] 3.1 `ArrayBacking` 各变体元素缓冲 → GC 变长块内联（`Boxed`/packed `Bytes/I32/I64/…`/`StructBytes`）
- [ ] 3.2 `ArrayObj` 头与元素块关系（头留 `region_array` 指变长块，或头并入变长块——PR-3 定）
- [ ] 3.3 trace：`Boxed` 逐元素、packed 叶子跳过、`StructBytes` 按引用位图；`gc_refs()` 等价物走 GC 块
- [ ] 3.4 packed 基元数组 / `struct[]` / StackArray（保持栈）边界；创建点走 `ctx.heap()`（已有）
- [ ] 3.5 write-barrier array 元素写接通变长块
- [ ] 3.6 benchmark：数组密集 workload GC 压力；GREEN cargo + Miri + `xtask test`（self-host 逐字节）+ e2e 数组生命周期/packed/struct[]

## PR-4: string 进 GC（最难：弥散分配 + 加载期 interning + safepoint）
> **架构方向已定（D11）= 堆作为一等 ambient 分配服务**（CLR/JVM 模型；非 188 处线程穿透）。
- [ ] 4.0 ambient 堆：泛化 `CURRENT_VM` thread-local → 正规 `current_heap()`（ungate native-interop；`VmGuard` 已包裹每个 `exec_function`，`interp/mod.rs:745`）；覆盖非 interp 路径审计
- [ ] 4.1 GC string 块布局（type_tag=Str，`{GcBlockHeader, UTF-8 bytes}`，len 由 header.size 派生）；`vstr.rs` `Str` 的 Arc 头 → GC 块头 / `Str::new` 从 ambient 堆分配，删 `strong` 原子计数 + Arc drop（payload 派生走原始 NonNull，D8）
- [ ] 4.2 **加载期 interning**：`Module.interned_strings` 在加载期用堆引用（VmCore.heap 早于模块加载存在）GC 分配 + 注册永久 GC roots（`merge.rs:26`/`loader.rs:686` 有界单点）；`set_external_root_scanner`（`vm_context.rs:672`）扫 interned 池 + `JitModuleCtx.string_pool`
- [ ] 4.3 **safepoint 纪律**：临时 string（表达式中间值）分配触发 GC 前须可达（register root / 抑制 mid-alloc collect）——设计 + 验证
- [ ] 4.4 `Value::Str(Str)` → `Value::Str(VarGcRef)`（8B）；`is_heap_ref`=true、`trace_children` string 臂（叶子终止）；str_meta 缓存身份 key → 块头地址
- [ ] 4.5 ~188 处 `.into()` 保持不变（ambient 堆）；`fn_name`/`element_type: Arc<str>` 顺带迁 GC string
- [ ] 4.6 benchmark：z42c 自编译（string-heavy）GC 频率/内存峰值/吞吐——量化 GC 压力回归（本程序预期短期代价）；GREEN cargo + Miri + `xtask test`（self-host 逐字节）+ e2e string 生命周期

## PR-5: 收敛 + 文档
- [ ] 5.1 删 Arc/Box/外部 Vec 双路径残留；收敛 `trace_children` vs `scan_object_refs` 双 visitor（若时机合适）
- [ ] 5.2 mark/sweep/write-barrier/内存统计口径统一到单一堆
- [ ] 5.3 docs：`gc.md`/`gc-handle.md`（变长分配器 + 单一堆 + A' 原理，配 mermaid/伪代码）、`object-abi.md` §5（string 进 GC 落地）/§6（移动 GC 衔接）、`roadmap.md` 收口
- [ ] 5.4 GREEN 全绿 + dist；change 容器归档

---

## 全局验证纪律（每 PR）
- 改 runtime 前 `cargo build --release --bin z42vm`（xtask warm-skip 不重建 → 陈旧假红）。
- 改 ScriptObject/TypeDesc/Value/Region 后须 `cargo test --tests --no-run` + `cargo bench --no-run`（--lib 不编集成/bench，.slots 教训）。
- 变长分配器 unsafe → Miri/ASAN 是硬门禁。
- 纯 runtime、无格式 bump（逐 PR 复核）→ self-host 5/5 gen1==gen2 逐字节应不动；动了说明踩到序列化面，停下查。
- benchmark（string/array-heavy）量化 GC 压力回归——这是本程序**预期的短期代价**，量化而非追求零回归。
