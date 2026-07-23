# Design: 惰性逐函数 JIT

## Architecture

```
                      ┌─────────────────────────────────────────────┐
   jit::run(ctx,      │ setup(module):                              │
      module, entry)  │   建 JITModule + 注册 helper 符号            │
        │             │   预分配 fn_entries_by_id[N]（全 empty 槽）  │  N = module.functions.len()
        ▼             │   返回 JitModule{ lazy: Mutex<LazyCompiler> }│
   compile_one(entry) └─────────────────────────────────────────────┘
        │
        ▼   执行入口原生码 ── 每个 Call 走 hr_call(jit_call) ──┐
   ┌────────────────────────────────────────────────────────┘
   ▼
  jit_call(method_id / name):
    1. 热路径：fn_entries_by_id[id].get() → Some(&FnEntry) → 直接 native 调用（零锁）
    2. miss：解析 func index（method_id，或 UNRESOLVED 时 module.func_index[name]）
         a. index 命中且函数可翻译：lock(lazy) → 双重检查 → compile_one → 存槽 → native
         b. 不可翻译 / 非合并模块目标：cross_zpkg_via_interp（现状不变）
```

**核心不变量**：编译时机改变，**加载/合并策略（main.rs 的 transitive BFS）完全不动**——
合并模块里仍含整套依赖闭包的函数**定义**，只是不再在加载时全部翻译。

## Decisions

### Decision 1: JIT 模式彻底改 lazy，删除 eager 全量编译
**问题：** 是保留 eager 作为可选路径，还是彻底替换？
**选项：**
- A（彻底替换）：JIT 一律 lazy。符合 philosophy「不留兼容路径 / 最简」。benchmark 首次调用每个
  函数付一次性编译（稳态无影响，与主流 tiered JIT 行为一致）。
- B（保留 eager 开关）：多一个策略分叉 + 测试面，违反「不留兼容路径」，且 AOT 尚未落地、当前无
  第二个 eager 消费者。
**决定：选 A**。AOT 未来若确需 eager 全量编译，届时随 AOT change 引入编译策略分叉；当前不预留。
（⚠️ 待 User 在 6.5 确认。）

### Decision 2: 数据结构——热路径零锁读 + 编译串行化
**问题：** `jit_call` 在共享 `&ctx` 下要就地编译并插入，且不能让已发出的 `&FnEntry` 借用失效
（Vec 扩容 / HashMap rehash 会移动元素）。
**决定：**
- `fn_entries_by_id: Vec<OnceCell<FnEntry>>`，在 setup 时**预分配到 `module.functions.len()` 并
  永不 resize** → 每个槽地址稳定，`OnceCell::get() -> &FnEntry` 借用长期有效、**热路径无锁**。
- 编译状态 `LazyCompiler{ jit: JITModule, helper_ids, module: *const Module }` 放
  `Mutex<LazyCompiler>`，仅**首次编译某函数**时加锁；**双重检查**（拿锁后再看 `OnceCell` 是否已被
  他线程填好）避免重复 define。
- 按名 fallback（`method_id == UNRESOLVED`）**统一经 `module.func_index[name] → index`** 路由回
  同一套 by-id 槽，不再维护独立可变 by-name 表（消除第二套内部可变结构）。真正非合并模块的按名目标
  仍落 `cross_zpkg_via_interp`。

> `OnceCell`（`std::cell::OnceCell` 非线程安全 / `std::sync::OnceLock` 线程安全）：因存在 `spawn`
> 多线程首调，选 **`OnceLock`**；set 竞争由「Mutex 内编译 + 双重检查」保证只 define 一次，`OnceLock`
> 只负责安全发布指针。

### Decision 3: 计数器 / 事件语义
**问题：** `jit_methods_compiled` 与 `JitModuleCompiled` 事件原按「模块函数总数」一次性上报。
**决定：**
- `jit_methods_compiled`：改为每次 `compile_one` 成功 +1 = **实际编译数**（更真实反映 JIT 工作量）。
- `JitModuleCompiled` 事件：保留**每模块一次**，在 `setup` 后 fire，`function_count` 改报
  `module.functions().len()`（模块规模，非已编译数）；`duration_us` 报 setup 耗时。逐函数编译事件
  留待未来（观测性 spec），避免每函数一个事件的洪泛。

### Decision 4: 入口函数显式先编
入口函数无调用者触发惰性路径，故 `jit::run` 在执行前显式 `compile_one(entry)`，再进入原生码执行。

### Decision 5: 线程安全边界
- **编译**：`Mutex<LazyCompiler>` 串行化；`OnceLock` 发布 → 同一函数只 define 一次、无数据竞争。
- **执行**：并发 JIT 执行读取 `OnceLock` 槽是安全的（发布语义）。现有 `JitModuleCtx.vm_ctx` 的
  「单一 vm_ctx / 无并发同 JitModule 入口」假设**不因本变更恶化**——原先并发读 `fn_entries` 已存在；
  本变更新增的并发**写**由锁+OnceLock 覆盖。跨线程 vm_ctx 复用属既有话题，不在本 Scope。

### Decision 6: 集中式解析器（2026-07-23 实施发现）
**问题：** `fn_entries`（by-name）不止被 call/vcall 消费——object.rs（构造器）、closure.rs
（CallIndirect）、value.rs（ToString）共 7+ 处站点各自 `fn_entries.get(name)`。逐站点内联惰性逻辑会
散落且易漏。
**决定：** 在 `JitModuleCtx` 上加**两个集中解析器**，把所有站点的查表统一收口：
- `resolve_fn_by_id(idx) -> Option<&FnEntry>`：热路径读 `OnceLock` 槽；miss 时经 `Mutex<LazyCompiler>`
  双重检查 + `compile_one` + 发布；不可翻译 → 返回 None（站点退 interp/异常，与旧 `get` miss 语义**逐一
  对齐**）。
- `resolve_fn_by_name(name) -> Option<&FnEntry>`：`module.func_index[name] → idx` 后转 `by_id`。
各站点改动 = 机械 swap（`fn_entries.get(x)` → `resolve_fn_by_name(x)`；`fn_entries_by_id.get(id).and_then`
→ `resolve_fn_by_id(id)`），语义不变、惰性逻辑仅一处。**`fn_entries`（by-name HashMap）字段随之删除**
（func_index + by-id 槽完全覆盖）。

## Implementation Notes

- **拆 `compile_module`**：
  - `setup(module) -> LazyCompiler`：建 JITModule、`helpers::register_symbols` / `declare_imports`、
    预分配 `fn_entries_by_id`。
  - `compile_one(&mut self, func_idx) -> Result<&FnEntry>`：`max_reg(f)` → `declare_function` →
    `translate_function`（只需该函数自身 id）→ `finalize_definitions` → `get_finalized_function` →
    构造 `FnEntry` → `OnceLock::set` → `jit_methods_compiled += 1`。
- **`translate_function` 签名**：现取 `&func_ids: HashMap`（仅用于查自身 id，见 translate.rs:254）→
  改为直接取本函数 `FuncId`，去掉全表依赖。
- **`jit_call`（call.rs:44-64）**：`entry_ref` miss 时，不直接 `cross_zpkg_via_interp`；先
  `resolve_index(method_id, name)`：命中且 `jit_unsupported_reason(f).is_none()` → 锁内 `compile_one`
  → 拿 `&FnEntry` 继续现有 native 调用；否则退 `cross_zpkg_via_interp`。
- **`jit_vcall`（vcall.rs:68/114）**：同款 miss→lazy hook。
- **`finalize_definitions` 逐函数**：每次 compile_one 调一次，触发一次 mprotect；成本 O(实际调用函数
  数) ≪ O(闭包全体)。可接受。
- **`JITModule` 生命周期**：`LazyCompiler` 持有 JITModule，由 `JitModule`（`jit::run` 局部）拥有并
  outlive 整个 run；ctx 经原始指针 `*const Mutex<LazyCompiler>` 反查（沿用现有 raw-pointer + Send/Sync
  unsafe impl 模式）。

## Testing Strategy

- **回归（最强保证）**：`xtask test e2e --mode jit` 全部 golden 输出逐字节不变（interp 为参考）。
  这直接覆盖「语义/覆盖不变」的全部 Requirement。
- **单元（`lazy_tests.rs`）**：
  - 首调编译：编译计数从 0 起，调用后仅目标函数计数 +1。
  - 未调不编：模块含未调用函数 → 运行后其未编译（计数不含它）。
  - 幂等：二次调用不再 +1。
  - 多线程首调：两线程并发首调同一函数 → 计数恰 +1、双方结果正确。
- **VM 验证**：`xtask test`（完整 GREEN gate，含 e2e / cross-zpkg / stdlib / compiler）。
- **性能佐证（非门禁）**：`Z42_JIT_PROFILE=1` 跑单个 golden，确认「compiled N functions」从「整套
  stdlib（数千）」降到「该用例实际调用（数十）」。CI 上 `test-vm-jit` shard 墙钟应从 ~55m 回落。
