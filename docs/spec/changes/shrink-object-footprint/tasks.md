# Tasks: 压缩每个堆对象的实际占用（shrink-object-footprint）

> 状态：🟡 实施中 | 创建：2026-09-04 | 类型：vm
> proposal / design 见同目录。三项一个 PR，**每项一个 commit + 独立度量**。

## P1 终结器槽 24 → 8 字节

- [x] 1.1 `gc/region.rs`：`RegionEntry.finalizer` 由 `Mutex<Option<FinalizerFn>>`
      改为 `AtomicPtr<FinalizerFn>`（`Box::into_raw` 形态，null = 无）
- [x] 1.2 `gc/region.rs`：`RegionEntry` 新增 `Drop`，释放非空终结器指针
- [x] 1.3 `gc/refs.rs`：`set` / `cancel` / `take` / `has` 四个访问器改走原子指针，签名不变
- [x] 1.4 `gc/arc_heap/{collect,generational,control}.rs`：5 处 `finalizer.lock().take()`
      改为 `take_finalizer_raw(entry)`
- [x] 1.5 `gc/region_tests.rs`：加布局断言 + set/take/cancel/drop 不泄漏的单测
- [x] 1.6 度量：`RegionEntry<ScriptObject>` 128 → 112；RSS A/B

## P2 `bytes` + `refs` 合成单块

- [ ] 2.1 `metadata/types/obj_storage.rs`（NEW）：`ObjStorage` + 四个安全访问器 +
      `Drop` + `unsafe impl Send/Sync` + 单测（空 / 仅字节 / 仅引用 / 混合 / drop 计数 / 对齐）
- [ ] 2.2 `metadata/types/object.rs`：`ScriptObject.bytes`/`refs` → 私有 `storage`，
      加 `bytes()` / `bytes_mut()` / `refs()` / `refs_mut()`
- [ ] 2.3 `metadata/types/type_desc.rs`：`object_regions()` → `object_storage()`
- [ ] 2.4 编译器穷举其余调用点（gc/collect、gc/generational、interp、corelib、jit）
- [ ] 2.5 度量：`ScriptObject` 72 → 56；RSS + `09_alloc_ctorless` A/B

## P3 `native` + `type_args` → `Option<Box<ObjExtras>>`

- [ ] 3.1 `metadata/types/object.rs`：`ObjExtras` + 惰性建盒 + `native()` / `type_args()` 访问器
- [ ] 3.2 调用点收敛（reflection accessors / convert / interp / jit）
- [ ] 3.3 度量：`ScriptObject` 56 → 32；`RegionEntry` → 72；RSS

## 验收

- [ ] 4.1 `cargo test --lib` 全过（含新增 `ObjStorage` / 终结器单测）
- [ ] 4.2 wasm32 检查 0 error
- [ ] 4.3 `xtask test` ✅ GREEN（含 `arc_heap_tests/finalization.rs` 原样通过、
      `concurrent_gc_mode_stress`、cross-zpkg 静态初始化）
- [ ] 4.4 内存 A/B：留住 200 万 `Node` 的 peak RSS，目标 ≥ −25%
- [ ] 4.5 CPU A/B：`09_alloc_ctorless` 不回归
- [ ] 4.6 启动：instructions retired + 死字段对照组（布局彩票判据）

## 实测

同机；base = main(#423) 编出的 VM。内存载体 = 留住 200 万 `Node`；CPU 载体 =
`bench/scenarios/09_alloc_ctorless`。

| | peak RSS | 每对象* | `09_alloc_ctorless` |
|---|---:|---:|---|
| base(main) | 516.64 MB | 236 B | 264.7 ms ± 7.2 |
| **P1**（终结器槽 24→8）| **474.38 MB（−8.2%）** | **215 B** | 258.8 ms ± 5.0（1.02×，不回归）|

\* 每对象 =（RSS − 空跑 12.29 MB − 200 万格数组约 32 MB）/ 200 万。

`RegionEntry<ScriptObject>` 128 → **112**（单测 `region_entry_stays_lean_for_script_objects`
把「header = T + 40」钉住，涨了就红）。
