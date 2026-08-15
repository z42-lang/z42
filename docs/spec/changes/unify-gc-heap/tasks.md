# Tasks: 统一 GC 堆模型（变长 GC 分配器 A'）

> 状态：DRAFT（待 6.5 gate）| 创建：2026-08-15
> 终点 = 单一 GC 堆（string/closure/array 变长数据都进 GC 变长分配器）；内部分 5 PR，每 PR 独立 GREEN + rebase。
> User 三决策：动机=架构统一/为移动 GC 铺路（接受 GC 压力权衡）；分配器=A'；scope=全包。

## 进度概览
- [ ] PR-1: 变长 GC 分配器原语（inert，无消费者，UB 门禁）
- [ ] PR-2: string 进 GC（`Value::Str`→GcRef，删 Arc；~241 迁移 + benchmark）
- [ ] PR-3: delegate/closure 进 GC（`Closure(Box)`→GcRef；StackClosure 保持栈）
- [ ] PR-4: array backing 进 GC（`ArrayBacking` Vec→GC 变长块；packed/struct[] 全迁）
- [ ] PR-5: 收敛 + 文档（删双路径残留；gc.md/object-abi/roadmap 收口）

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

## PR-2: string 进 GC
- [ ] 2.0 **heap 接线（从 PR-1 移入，D9）**：`ArcMagrGC` 加 `region_var: Mutex<VarRegion>`；`mark_phase` 遇 `VarGcRef` 按 type_tag mark + trace-hook 扫 payload（string 叶子终止）；`sweep_phase` 驱动 `region_var.sweep()`；`alloc_var_block` + write-barrier 接口——**由 string 真实分配驱动验证**
- [ ] 2.1 GC string 块布局（`{GcBlockHeader, len, UTF-8 bytes}`）；`vstr.rs` `Str` 的 Arc 头 → GC 块头，删 `strong` 原子计数 + Arc drop（payload 派生走原始 NonNull，D8 铁律）
- [ ] 2.2 `Value::Str(Str)` → `Value::Str(GcRef<...>)`（仍 8B）；`is_heap_ref`=true、`trace_children` string 臂（叶子终止）
- [ ] 2.3 interned 串池 `Module.interned_strings: Vec<Str>` → GC roots 注册（+ `jit/vm_interface.rs` 访问点）；const-str clone 变 GcRef clone
- [ ] 2.4 str_meta 缓存身份 key（现 `Str::as_ptr`）→ GcRef 块头地址
- [ ] 2.5 ~241 处 `Value::Str(` 非测试迁移（reflection/jit-helpers/repl/corelib 热点）——`cargo build --lib` 错误列表 = worklist
- [ ] 2.6 benchmark：z42c 自编译（string-heavy）GC 频率/内存峰值/吞吐——量化 GC 压力回归，超阈值调 GC 触发
- [ ] 2.7 GREEN：cargo --lib + 集成/bench 编译 + Miri + `xtask test`（self-host 5/5 逐字节，纯 runtime）+ e2e string 生命周期

## PR-3: delegate/closure 进 GC
- [ ] 3.1 `ClosureData` 进 GC 变长块；`Value::Closure(Box<ClosureData>)` → `Closure(GcRef<ClosureData>)`
- [ ] 3.2 `ClosureData.fn_name: String` → GC string（复用 PR-2）；`FuncRef(Str)` 名字复用 GC string
- [ ] 3.3 `env: GcRef<ArrayObj>` trace 边不变；closure 块 trace 扫 env + fn_name
- [ ] 3.4 `StackClosure` 保持栈（不进 GC，逃逸分析产物）——确认边界
- [ ] 3.5 ~28 Closure + ~24 FuncRef 构造点迁移（`interp/exec_call.rs`、`jit/helpers/closure.rs`、`corelib/object.rs`）
- [ ] 3.6 GREEN：cargo + Miri + `xtask test`（self-host 逐字节）+ e2e closure 生命周期/捕获

## PR-4: array backing 进 GC（最大最敏感）
- [ ] 4.1 `ArrayBacking` 各变体元素缓冲 → GC 变长块内联（`Boxed`/packed `Bytes/I32/I64/…`/`StructBytes`）
- [ ] 4.2 `ArrayObj` 头与元素块关系（头留 `region_array` 指变长块，或头并入变长块——PR-4 定）
- [ ] 4.3 trace：`Boxed` 逐元素、packed 叶子跳过、`StructBytes` 按引用位图；`gc_refs()` 等价物走 GC 块
- [ ] 4.4 packed 基元数组 / `struct[]` / StackArray（保持栈）边界；`element_type: Arc<str>` 顺带迁 GC string
- [ ] 4.5 write-barrier array 元素写接通变长块
- [ ] 4.6 benchmark：数组密集 workload GC 压力；GREEN cargo + Miri + `xtask test`（self-host 逐字节）+ e2e 数组生命周期/packed/struct[]

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
