# Tasks: 让 `Value` 成为 `Copy`（瞬态 arena 句柄）

> 状态：🟡 进行中 | 创建：2026-08-18 | 类型：vm（规范先行，已实测 spike 1.21×）

## 进度概览
- [ ] 阶段 1: `TransientArena` 地基 + `GcRef: Copy`
- [ ] 阶段 2: 4 变体句柄化（types.rs）+ VmContext/frame 接线
- [ ] 阶段 3: 构造点 / 消费点改写（interp + JIT + corelib + marshal）
- [ ] 阶段 4: GC root 接线 + 移除穿句柄 trace/mark
- [ ] 阶段 5: `derive(Copy)` + 测试 + GREEN + 性能 A/B + 文档

## 阶段 1: 地基
- [ ] 1.1 新建 `interp/transient_arena.rs`：`TransientPayload` enum（4 payload）+ `TransientSlot`
      + `TransientArena`（base/truncate/alloc/with/scan_roots），镜像 `struct_arena.rs`
- [ ] 1.2 `interp/mod.rs` 加 `pub(crate) mod transient_arena;`
- [ ] 1.3 `gc/refs.rs`：删 `impl Drop for GcRef`；`Clone` 改 `*self`；加 `impl<T> Copy for GcRef<T>`
- [ ] 1.4 `transient_arena_tests.rs`：alloc/with/staleness/truncate/scan_roots 单测

## 阶段 2: 句柄化 + 接线
- [ ] 2.1 `metadata/types.rs`：4 变体 `Box<T>` → `{ idx:u32, frame_id:u32 }`；`RefKind`/`PinnedViewData`/
      `StackClosureData`/`StructArrayElem` 保留（移到/留作 arena payload）
- [ ] 2.2 `vm_context.rs`：`transient_arena: Arc<Mutex<TransientArena>>` 字段 + 2 处构造初始化
- [ ] 2.3 `VmFrame` 加 `transient_base: usize`；`push_frame` 戳 base；`pop_frame` truncate
- [ ] 2.4 `size_of::<Value>()==16` 断言确认通过（8B 句柄）

## 阶段 3: 构造 / 消费改写
- [ ] 3.1 `exec_address.rs`：`Ref` 3 处构造 → `transient_arena.alloc(frame_id, Ref(kind))`
- [ ] 3.2 `interp/mod.rs`：`deref_ref`/`get_deref`/`set_thru_ref` 经 arena 解 `RefKind`
- [ ] 3.3 `exec_native.rs`：`PinnedView` 构造(2) + `UnpinPtr` 消费经 arena
- [ ] 3.4 `native/marshal.rs`：`PinnedView` FFI marshal 读 `pv.ptr/len` 经 arena
- [ ] 3.5 `exec_object.rs`：`PinnedView` `.ptr/.len` FieldGet + `StructRefHeap` 消费经 arena
- [ ] 3.6 `exec_array.rs`：`StructRefHeap` 构造 → arena；`exec_struct.rs`：消费经 arena
- [ ] 3.7 `exec_call.rs`：`StackClosure` 构造 + `CallIndirect` 消费经 arena
- [ ] 3.8 `corelib/object.rs`：`builtin_delegate_target/fn_name/eq` 读 `StackClosure` 经 arena
- [ ] 3.9 `jit/helpers/closure.rs`(StackClosure 构造) + `array.rs`(StructRefHeap 构造) +
      `object.rs`(StructRefHeap 消费) 经 arena；`jit/translate.rs` match/注释同步
- [ ] 3.10 trivial 模式改 `{..}`：`exec_vcall.rs`、`corelib/threading.rs`
- [ ] 3.11 `corelib/convert.rs`：`value_to_str` 4 变体降级通用串（照 StackObject 先例）

## 阶段 4: GC
- [ ] 4.1 `vm_context.rs`：`transient_arena.scan_roots` 接进所有 GC root 扫描点（mark + categorized）
- [ ] 4.2 `gc/arc_heap.rs`：移除 `Ref`/`StructRefHeap` 的 `mark_if_unmarked`/`mark_phase`/`gen_age` 穿句柄臂
- [ ] 4.3 `types.rs`：`visit_gc_children` 的 `Ref`/`StructRefHeap` 臂 → no-op；`PartialEq` 4 臂 → 句柄相等；
      `tag`/`heap_size`/`Debug` 各臂适配句柄

## 阶段 5: 收口
- [ ] 5.1 `types.rs`：`Value` 加 `#[derive(..., Copy)]`
- [ ] 5.2 测试文件构造点改 arena：`types_tests.rs`/`corelib/tests.rs`/`threading_tests.rs`/`exec_struct_tests.rs`
- [ ] 5.3 `cargo build --release --bin z42vm` 无错 + `cargo test --release --tests --no-run` 编集成测试
- [ ] 5.4 `cargo test --lib` + `xtask test`（e2e / cross-zpkg / stdlib / compiler / vscode-syntax 全绿）
- [ ] 5.5 **自举 5/5 gen1==gen2 逐字节**（--workspace，C#-free）
- [ ] 5.6 性能 A/B（hyperfine 前端 typecheck vs `/tmp/z42vm_vcopy_base`）复现 ~1.2×；profile 确认坍缩
- [ ] 5.7 JIT struct[]/closure 密集用例确认 arena 随帧 truncate（内存不无界）
- [ ] 5.8 文档：新建 `docs/design/runtime/value-representation.md`（挂 SUMMARY 若走 book）；
      `interp/README.md` 加 `transient_arena.rs`；roadmap Deferred 索引（如涉及）

## 备注
- spike（`&'static` 泄漏版）已实测 **7.231s→5.977s=1.21×**、输出逐字节一致、`Value::clone` 离开
  profile 头部、`drop<Frame>` 195→68。真实版用 arena 句柄（非泄漏），行为等价、GC 正确、可回收。
- 基线二进制 `/tmp/z42vm_vcopy_base`(origin/main)；A/B 配方见
  [[interp-bigfour-perf-program]] 恢复环境段。
