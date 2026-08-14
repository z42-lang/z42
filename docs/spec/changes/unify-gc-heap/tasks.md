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

## PR-1: 变长 GC 分配器原语（行为不变、inert）
- [ ] 1.1 `GcBlockHeader` 定义（mark/alive/generation/gen_age/size/type_tag，对齐 8，叶子无 finalizer/soft_ref）
- [ ] 1.2 `VarRegion`（或泛化 `Region`）：字节 chunk（`Vec<Box<[u8; VAR_CHUNK]>>`）+ bump 分配对齐块 + size-class free-list 回收
- [ ] 1.3 `alloc(size, type_tag) -> GcRef` / `tombstone` / `iterate_alive`（按 size 推进）/ `resolve`（地址稳定）
- [ ] 1.4 `GcRef` 对变长块的支持（复用 8B 标记指针 `Tagged<T>`；变长块无静态 T → 评估 `GcRef` 泛型参数或专用句柄）
- [ ] 1.5 mark/sweep/generation 接入：`ArcMagrGC` 加 `region_var: Mutex<VarRegion>`；`mark_phase`/`sweep_phase` 驱动变长 region；trace-hook 按 type_tag 分类扫 payload
- [ ] 1.6 write-barrier 对变长块（array<Value>/closure 元素/字段写）——inert 阶段仅接口，无消费者
- [ ] 1.7 单测（含 Miri/ASAN，UB 敏感）：各 size 分配 / free-list 复用 / generation ABA / sweep tombstone / 地址稳定 / trace 分类 / OOM
- [ ] 1.8 GREEN：`cargo test --lib` + `cargo test --tests --no-run` + `cargo bench --no-run`（.slots 教训）+ Miri（vstr/refs/region unsafe）+ `xtask test`（自举字节不动，纯 runtime inert）+ `cargo build --release --bin z42vm`

> **决策待定（PR-1 定）**：① size-class 分档策略（幂次 vs 固定）；② `GcRef` 如何指异构变长块（泛型 vs type-erased 句柄）；③ 变长 region 是否初版仅 STW、generational 后补。

## PR-2: string 进 GC
- [ ] 2.1 GC string 块布局（`{GcBlockHeader, len, UTF-8 bytes}`）；`vstr.rs` `Str` 的 Arc 头 → GC 块头，删 `strong` 原子计数 + Arc drop
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
