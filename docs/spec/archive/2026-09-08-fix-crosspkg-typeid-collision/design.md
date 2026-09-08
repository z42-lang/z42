# 设计：TypeId 全局唯一化

> Why 见 [proposal.md](proposal.md)。本文只讲 How。

---

## D1 —— 新不变量

```
旧：TypeId 在**单个 Module** 内唯一，且 == type_registry_vec 的下标
新：TypeId 在**整个进程**内唯一；不再承载任何「下标」语义
```

号段沿用 `tokens.rs` 已有划分，只占最低那段：

```text
全局类型号:      [0,             0x7FFF_FFFE]    ← 本变更在这里发号
IMPORT_BASE:     0x8000_0000
PRIM_TYPE_*:     0xFFFE_0000..0xFFFE_000F        ← 合成基元 id，不经发号器
UNRESOLVED:      0xFFFF_FFFF
```

## D2 —— 发号器放哪、怎么发

放 `metadata/tokens.rs`（它已经是 token 号段契约的 SoT）：

```rust
/// fix-crosspkg-typeid-collision：TypeId 必须**进程内全局唯一** —— VCallIC / FieldIC
/// 拿它当全局类型身份 key（见 proposal.md 的现场）。每模块从 0 重新发号会让不同 zpkg
/// 的两个类共号，同一调用点上后到的 receiver 会误命中先到者的缓存条目。
static NEXT_TYPE_ID: AtomicU32 = AtomicU32::new(0);

/// 一次预留 `n` 个连续号，返回首号。批量取使每模块只做一次 `fetch_add`，
/// 且模块内 id 仍连续（读 dump 时好认）。
pub fn alloc_type_id_block(n: u32) -> u32
```

- **越界处理**：`first + n > IMPORT_BASE` 直接 `panic!`（号段耗尽是编程/资源错误，不是
  可恢复状态；静默回绕会把本 bug 原样带回来）。
- **为什么进程级而不是 VmContext 级**：`build_type_registry(&mut Module)` 的 4 个生产
  调用点（`app.rs:222` / `host/ops.rs:168` / `loader/artifact.rs:154,312`）都拿不到
  `VmContext`。进程级计数器无需改签名，且单进程多 VM 时唯一性更强、无副作用。

`loader/type_registry.rs`：

```rust
 let order = topo_sort_classes(module);
-let mut next_type_id: u32 = 0;
+let mut next_type_id: u32 = tokens::alloc_type_id_block(order.len() as u32);
```

其余不动 —— 仍按 topo 序递增，模块内仍连续。

## D3 —— 删掉与新不变量矛盾的死设施

| 删除对象 | 位置 | 依据 |
|---|---|---|
| `Module::type_registry_vec` 字段 | `bytecode/module.rs:37` | 生产代码只写不读（构造 / `push` / `clear`） |
| `Module::type_by_id()` | `bytecode/module.rs:55` | 唯一调用方是 `loader_tests.rs` |
| `Module::register_lazy_type()` | `bytecode/module.rs:69` | 唯一调用方是 `loader_tests.rs`；**它本该是防住本 bug 的那道机制，却从未接线** |
| 三个单测 | `loader_tests.rs:243-395`（Phase 3 S1 块） | 测的就是被删的 API |

连带：`type_registry.rs:181-185` 的 `debug_assert_eq!(registry_vec.len(), type_id.0)`
（新不变量下必然不成立）、`lazy_loader/registry.rs:60,312` 的 `.clear()`、以及约 18 处
测试里 `Module { … type_registry_vec: Vec::new() }` 的字段行。

> **为什么删而不是留着**：留着 = 留一个文档写着「`type_registry_vec[id.0] == registry[name]`」
> 而实际已不成立的字段。这正是 [[scripts-structure-cleanup-program]] 那条教训
> （「没有东西盯着的记述迟早会烂」）的下一个受害者。而且 `register_lazy_type` 的存在
> 会让下一个人误以为跨 zpkg 的 id 已经被重新发号过 —— 本次排查里我自己就先信了它一次。

**提交拆分（IMPL 期修正）**：DRAFT 原计划把本项作为独立 `refactor` commit。实施后发现**分不开、
也不该分**：旧代码里 `debug_assert_eq!(registry_vec.len(), type_id.0)` 直接断言了「id == 本模块
稠密下标」，发号一改这条断言当场为假 —— 把删除留到下一个 commit 会让**第一个 commit 的 debug
构建是坏的**，不可 bisect。故合并进同一个 `fix(runtime)` commit，并在 commit body 里写明理由。
这不违反「拆分与功能变更分开提交」：它不是独立的整理，而是本次修复的**必要组成**。

## D4 —— 常驻断言门（让它会红）

把这次用的临时探针转正，`#[cfg(debug_assertions)]` 下在两个 PIC 的**命中**处校验：

| 缓存 | 校验 |
|---|---|
| `VCallIC` | 缓存 callee 的 declaring class ∈ {receiver 类名, 其祖先链, `Std.Object`} |
| `FieldIC` | `receiver.type_desc.field_index[field_name] == 缓存的 slot` |

不匹配 → `panic!` 并打出 `recv / recv_id / method|field / callee|slot`。
release 构建零成本（`cfg` 编译期剔除），`cargo test`（debug）自动带上这道门。

> 这道门是**通用**的：它不只盯本次这一处，任何未来重新引入的类型身份撞键都会当场炸，
> 而不是变成一个「远在天边」的崩溃或静默错数据。

## D5 —— 回归门（IMPL 期定稿）

DRAFT 里备选了两条：多-exe fixture，或直击不变量的 Rust 单测。**IMPL 期选定后者**，
理由是它更直接、更确定，且能做真正的退回对照：

**`loader_tests.rs::type_ids_are_unique_across_modules`** —— 对两个独立 `Module` 各跑一次
`build_type_registry`，断言两边的 `TypeId` 集合**不相交**。

- 🔴 **退回对照已做**：把发号改回 `next_type_id = 0` 重跑，测试如期红，报
  `PkgA got {0, 1, 2}, PkgB got {0, 1, 2}` —— 正是本 bug 的机制本身。这是一道会红的门。
- 配套 `allocated_type_ids_stay_below_import_base`：守住 id 不越进 `IMPORT_BASE` 以上的
  import / 基元 / 哨兵号段。

**放弃多-exe fixture 的理由**（不是偷懒，是它守不住东西）：修复之后，撞键在源码层面
已**无法构造** —— 发号器保证了不相交，fixture 只能退化成「跨 zpkg 接口多态跑得通」的正向门。
而那条正向路径每次构建都在跑（`ParallelFor.Run` 的 `body.Run(i)` 就同时接
`CompileCuTask` 与 `SrcReadHashTask`，正是本 bug 的现场），再加一个 fixture 是重复覆盖。
撞键的负例由上面的单测（可退回对照）+ D4 的常驻断言（全局覆盖，不限这一处）守。

## D6 —— 不做的事

- **不改 PIC 的槽位布局 / 淘汰策略 / JIT codegen**：key 仍是 u32，`VCallICEntry` /
  `FieldICEntry` 一个字节不动。
- **不改 zbc / zpkg 格式、不 bump version**：`TypeId` 是 load-time 概念，
  `Module.type_registry*` 都是 `#[serde(skip)]`，磁盘上不存在类型号。
- **不碰编译器**：本次根因 100% 在 runtime。

## D7 —— 文档同步

| 文档 | 改什么 |
|---|---|
| `docs/design/runtime/vm-architecture.md` | TypeId 段：per-module → 进程全局唯一；说明「为什么」（PIC 拿它当全局身份） |
| `docs/book/` 对应机制页（VM 派发 / 内联缓存） | 补「PIC key 的唯一性要求」这条实现原理 + 本次现场作反例 |
| `.claude/rules/common-pitfalls.md` | 评估是否够格加一条「per-scope 的号当全局身份用」通则（满足三条标准：跨语言可复现 / 出过真 bug / 修法是模式）—— **建议加，但先请 User 裁决** |
