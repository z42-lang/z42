# Tasks: fix-runtime-load-order-determinism

> 状态：🟢 已完成 | 创建：2026-08-23 | 完成：2026-08-23 | 类型：fix（最小化模式）

**变更说明：** 修复 runtime 两处「资源加载顺序依赖非确定枚举源」的隐患（`runtime_review.md` H1），
消除跨 OS CI 差异风险。

**原因：** `common-pitfalls.md §1` 强制规则——任何 first-wins 注册的加载循环都必须先按稳定键排序，
禁止依赖 `read_dir` / 哈希表迭代的「碰巧字母序」。当前 native 库 dlopen 加载与 lazy_loader 的两个
候选查询直接迭代 `read_dir` / `FxHashMap`，在不同 OS / 不同运行间顺序非确定。

**文档影响：** 无对外行为 / 机制 / 命令面变更（纯内部确定性修复）；`common-pitfalls.md §1` 已是该规则
的 SoT，不新增文档。

## 任务

- [x] 1.1 H1a：`native/ext.rs` `load_via_dlopen` —— `read_dir` 结果 collect 到 `Vec<PathBuf>` 后
  `sort()` 再迭代（下游 `ExtBuiltinTable::register` first-wins）
- [x] 1.2 H1b：`metadata/lazy_loader.rs` `candidates_for_namespace` + `remaining_declared` ——
  迭代 `FxHashMap` 后对返回的 `Vec<String>` `sort()`
- [x] 1.3 回归测试：`lazy_loader_tests.rs` 新增 `candidates_returns_sorted_order` +
  `remaining_declared_returns_sorted_order`（逆序插入、断言内部已排序，不靠调用方 sort）
- [x] 1.4 验证：`cargo build --release` ✓ + `cargo test --lib`（lazy_loader 19 + write_barrier 8 全绿）
  + 完整 `xtask test` GREEN（TRUE_EXIT=0；e2e/cross-zpkg/multi-exe + stdlib 282/22 + compiler 26 units
  + self-host 5/5 gen1==gen2 + vscode-syntax，全 stage ✅）

## 备注

- **H4（write barrier release 防护）已从本 change 剥离**：核查发现 review 的 H4 推荐与
  `arc_heap_tests/write_barriers.rs:64-85` 明确记录的既有设计契约冲突（「barrier 本身不过滤
  primitive，call site 负责过滤，observer 记录全部调用以供端到端验证」），且 `maybe_mark_cross_gen_card`
  / `mark_if_unmarked` 已对 primitive 安全 no-op（H4 并非 review 所称的 release 内存安全 bug）。
  按 philosophy.md 设计完整性 + 规范冲突检测，H4 待 User 裁决后另行处理，不并入本 change。
