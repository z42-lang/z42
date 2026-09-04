# Design: 压缩每个堆对象的实际占用

> proposal 见同目录 `proposal.md`。本文只写 How 与决策依据。

## 目标布局（实测 `size_of`，非推算）

| | 现在 | 目标 |
|---|---:|---:|
| `ScriptObject` | 72 | **32** |
| ↳ `Arc<TypeDesc>` | 8 | 8 |
| ↳ `bytes: Box<[u8]>` + `refs: Box<[Value]>` | 16 + 16 | **16**（`ObjStorage` 单块）|
| ↳ `native: NativeData` + `type_args: Box<[String]>` | 16 + 16 | **8**（`Option<Box<ObjExtras>>`）|
| `RegionEntry<ScriptObject>` = `T + 56` → `T + 40` | 128 | **72** |
| ↳ 终结器槽 | 24 | **8** |
| 载荷块 | 两块（~48 + ~32）| 一块（~80）|
| **每对象合计（含分配器余量）** | **236** | **约 172（−27%）** |

## P1：终结器槽 24 → 8 字节

**决策：`Mutex<Option<FinalizerFn>>` → `AtomicPtr<FinalizerFnBox>`，而不是 proposal 里
写的 Region 级侧表。**

- 侧表能省满 24 字节，但要把 `GcRef::{set,cancel,take,has}_finalizer` 四个访问器从
  「只拿到 entry 指针」改成「要拿到 Region」——而 sweep 三条路径（`collect.rs`、
  `generational.rs`、`control.rs`）正在遍历 entry，再去 `&mut region.finalizers` 会
  和遍历借用打架，得先收集 location 再排空。**为最后 8 字节引入一个跨模块 API 改动
  + sweep 每对象一次哈希查，不划算。**
- `AtomicPtr` 版把 24 压到 8（`Option<Box<FinalizerFn>>` 的裸指针形态），
  **完全 entry-local**，四个访问器签名不变，sweep 只是把 `lock().take()` 换成
  `swap(null, AcqRel)` + `Box::from_raw`。顺带去掉一把每对象的 `parking_lot::Mutex`。
- 真注册了终结器的对象额外付一次 16 字节的 Box——**当前全仓生产代码注册数为 0**。

`RegionEntry::drop` 必须释放非空指针（否则泄漏 `Arc<dyn Fn>`）。`RegionEntry` 目前没有
手写 `Drop`，本次新增一个。

## P2：`bytes` + `refs` 合成单块（`ObjStorage`）

一次分配，布局 **`[refs: Value; n_refs][bytes: u8; n_bytes]`**：

```
  ┌────────────────────────┬──────────────────────┐
  │ refs: [Value; n_refs]  │ bytes: [u8; n_bytes] │
  │ 16 B 对齐，天然在前     │ 起点仍 8 对齐         │
  └────────────────────────┴──────────────────────┘
```

- 引用放前面是为了**对齐**：`Value` 要 8 对齐，`n_refs * 16` 恒为 8 的倍数，
  所以字节区起点必然 8 对齐——复合布局把 i64/f64 放在 8 对齐偏移上这一前提得以保持。
- `ObjStorage { ptr: NonNull<u8>, n_refs: u32, n_bytes: u32 }` = **16 字节**。
- 两者皆 0 → **不分配**，`ptr` 用 `NonNull::dangling()`（无字段类同样不付 malloc，
  与今天 `Box::from([])` 的行为一致）。
- 字节区用 `alloc_zeroed`（零 = 每个基元字段的默认值，与今天 `vec![0u8; nb]` 等价）；
  引用槽**逐个 `ptr::write(Value::Null)`**——不假设 `Value::Null` 的全零表示，
  那是 Rust 不保证的。
- `Drop`：先对每个 ref 槽 `drop_in_place::<Value>()`，再 `dealloc`。
- `unsafe impl Send/Sync`：`ObjStorage` 像 `Box` 一样**独占**该分配，访问一律经
  `&self` / `&mut self`，故 Send/Sync 由 `Value: Send + Sync` 推出。
- **所有 `unsafe` 关在这一个类型里**；`ScriptObject` 只见到四个安全切片访问器
  （`bytes()` / `bytes_mut()` / `refs()` / `refs_mut()`）。

`ScriptObject.bytes` / `.refs` 两个 pub 字段改成私有 `storage`，靠编译器把调用点全找出来
（这是本次唯一的机械改面）。

## P3：`native` + `type_args` 合并进 `Option<Box<ObjExtras>>`

```rust
struct ObjExtras { native: NativeData, type_args: Box<[String]> }
```

32 → 8 字节。两者对**绝大多数对象都是空的**：`NativeData::None` 只有
WeakRef / TypeHandle / LoadContextHandle / AssemblyHandle 四种内建盒子用，
`type_args` 只有泛型实例化用。有其一即分配这个冷侧结构。

访问器：`native()` 返回 `&NativeData`（`None` 时返回一个 `static NONE: NativeData`），
`type_args()` 返回 `&[String]`（`None` 时返回 `&[]`），
`extras_mut()` 在写入时惰性建盒。

## 分阶段与度量

三项各一个 commit，**每个 commit 单独给改前/改后同机数字**：

- 内存：`scratchpad` 的「留住 200 万 `Node`」程序 peak RSS（每项各测一次）。
- CPU：`bench/scenarios/09_alloc_ctorless` 同-runner A/B（P2 期望 ≥ +5%，P1/P3 期望不回归）。
- 启动：按 `benchmarking.md`「布局彩票」的配方——看 instructions retired + 死字段对照组。

## 风险与止损

| 风险 | 处置 |
|---|---|
| P2 动 `ScriptObject` 布局，调用点多 | 字段改私有，让编译器穷举；`ObjStorage` 自带单测（空/仅字节/仅引用/混合/drop 计数）|
| P2 的手写 `unsafe` | 全部关在 `obj_storage.rs`；对齐与零初始化各有单测；跑 `cargo test --lib` + `xtask test` |
| P1 的 `AtomicPtr` 漏 free | `RegionEntry` 新增 `Drop`；`arc_heap_tests/finalization.rs` 原样通过 |
| 布局变化触发 GC 竞态时序（memory 里的旧教训）| 三项都不新增 per-context 预分配；`concurrent_gc_mode_stress` 归在 `xtask test` 里 |
