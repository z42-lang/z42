# Design: 让 `Value` 成为 `Copy`（瞬态 arena 句柄）

## Architecture

```
                 register file: Vec<Value>   (Value: Copy, 16B)
                        │
     ┌──────────────────┼───────────────────────────────┐
   标量/Str/GcRef/     句柄变体 {idx,frame_id}         GcRef(Copy)
   VarGcRef(Copy)       │                                Object/Array/BoxedStruct
                        ▼
              VmContext::transient_arena : Arc<Mutex<TransientArena>>
                        │  slots: Vec<TransientSlot { frame_id, payload }>
                        │  payload ∈ { RefKind | PinnedViewData
                        │             | StackClosureData | StructArrayElem }
                        │
          push_frame → base 戳记 ;  pop_frame → truncate(base)  (interp + JIT 共用)
          GC mark → scan_roots(visit)  访问 payload 内 GcRef 叶子
```

`TransientArena` 与既有 `StackArena`(stack_alloc.rs) / `StructArena`(struct_arena.rs) 同构：
per-`VmContext`、`Mutex` 保护、`frame_id` staleness 守卫、LIFO truncate、`scan_roots` 作 GC root。

## Decisions

### Decision 1: 统一 arena vs 4 个独立 arena
**问题：** 4 个 payload 类型异构，各建一个 arena 还是一个统一 arena？
**选项：**
- A（4 独立）：`ref_arena`/`pinview_arena`/... 各一个 `Vec` + base 字段 + truncate + scan。样板 ×4。
- B（统一）：一个 `Vec<TransientSlot>`，`TransientSlot` 是 4 payload 的 enum。一处 base、一处 truncate、
  一处 scan_roots（`match slot.payload` 决定访问哪些 GC 叶子）。
**决定：** 选 **B**。4 者生命周期同为「创建帧 LIFO 内」，共享一个 arena 减少接线点（VmContext 一个字段、
VmFrame 一个 base、GC 一处 scan），且 payload enum 的判别在冷路径（构造/消费）不影响热路径（热路径只拷贝
8B 句柄）。

### Decision 2: 句柄编码 `{ idx: u32, frame_id: u32 }`
**问题：** 8B payload 怎么排？
**决定：** 复用 `StackObject`/`StructRef` 的 `{ idx: u32, frame_id: u32 }`（`#[repr(C,u8)]` 下 8B）。
`idx` = arena 下标；`frame_id` = 创建帧单调 id（staleness 守卫）。与既有句柄变体逐字节同构，
`size_of::<Value>()==16` 断言不变。

### Decision 3: GC —— arena root 覆盖，句柄不穿 trace
**问题：** 句柄化后 `visit_gc_children`/`mark_if_unmarked` 无 ctx，怎么保持 payload 内 GcRef 存活？
**选项：**
- A：给 trace/mark 传 ctx，穿句柄读 arena payload 再 trace。侵入 GC 签名 + 二次标记风险。
- B：`TransientArena::scan_roots` 作 GC root（每次收集扫），payload 内 GcRef 始终被标记；
  `visit_gc_children`/`mark_if_unmarked` 对 4 句柄变体 **no-op**（同 `StructRef`/`StackObject` 现状）。
**决定：** 选 **B**。这正是 `struct_arena`/`stack_arena` 已验证的模型：arena 是 root ⇒ 内容恒被标记 ⇒
无需穿句柄 trace、无需写屏障（arena 每次被重扫）。**净效果是从 GC mark 热路径移除工作**，非增加。
正确性前提：`scan_roots` 必须接进**所有** GC root 扫描点（mark + categorized root，vm_context.rs
~744 区，与 stack_arena/struct_arena 并列）。

### Decision 4: `value_to_str` / `PartialEq` 退化为句柄级
**问题：** `value_to_str(v:&Value)` 无 ctx、`PartialEq` 无 ctx，读不到 arena payload。
**决定：** 照 `StackObject`/`StackArray`/`StructRef` 既有先例——`value_to_str` 4 变体返回通用串
（escape sink 保证句柄不到达用户 stringify）；`PartialEq` 按 `{idx,frame_id}` 句柄相等。见 spec
MODIFIED Requirements。有 ctx 的消费点（delegate_*/marshal/FieldGet/deref_ref/CallIndirect）仍读真
payload。

### Decision 5: `GcRef: Copy`
**问题：** `GcRef` 有显式空 `Drop`（阻止 `Copy`）。删之安全吗？
**决定：** 安全。`Drop` 体是 no-op（D8：无 refcount，finalizer 在 sweep 触发）；backing = `Tagged<T>`
（已 `Copy`）+ `PhantomData`。删 `Drop` + 加 `impl<T> Copy for GcRef<T>` + `Clone` 改 `*self`。
`BoxedStruct(GcRef)` 随之自动 `Copy`。

## Implementation Notes

- **`TransientArena` API**（镜像 `StructArena`）：
  - `base()->usize` / `truncate(base)` / `alloc(frame_id, payload)->u32` /
    `with<R>(idx,frame_id,f)->Result<R>` / `scan_roots(visit)`。
  - `TransientSlot { frame_id: u32, payload: TransientPayload }`；
    `enum TransientPayload { Ref(RefKind), PinView(PinnedViewData), StackClos(StackClosureData), StructElem(StructArrayElem) }`。
  - `scan_roots`：`match &slot.payload` → `Ref(RefKind::Array/Field{gc_ref})` visit gc_ref；
    `StructElem(e)` visit `Value::Array(e.arr)`（构造一个临时 `Value` 或直接 `GcRef::mark`——用
    `visit(&Value)` 接口则临时包 `Value::Array(e.arr)`）；`PinView`/`StackClos`/`Ref::Stack` 无 GC 叶子。
- **frame_id 来源**：interp `Frame` 已有 `frame_id`；JIT helper 经 `vm_ctx_ref(ctx)` + 当前 VmFrame id
  （JIT 已用相同方式读 `stack_arena` 的 `{idx,frame_id}` 句柄，array.rs:148/227/287）。
- **VmFrame base**：加 `transient_base: usize`，`push_frame` 戳 `transient_arena.lock().base()`，
  `pop_frame` `truncate(f.transient_base)`（与 `struct_base` 并列，vm_context.rs:1103-1120）。
- **consumers 改写**：`Value::Ref(kind)` 直读 → `Value::Ref{idx,frame_id}` +
  `ctx.transient_arena.lock().with(idx,frame_id,|s| match &s.payload { TransientPayload::Ref(k)=>… })`。
  `deref_ref(kind,ctx)` 签名改为接收 `{idx,frame_id}` 或在调用前解出 `RefKind`（`RefKind` 仍 `Clone`，
  可 clone 出来释放锁再用）。
- **注意锁序**：arena `Mutex` 与 heap/其他锁不嵌套持有（构造/消费点短临界区：读出 payload 副本或在
  闭包内完成，避免在持 arena 锁时再进 GC/heap 锁）。`RefKind`/payload `Clone` 出来即释放锁。

## Testing Strategy

- **单元测试**（`transient_arena_tests.rs`）：alloc→with 读回；frame_id 不符→stale err；
  truncate 后 idx 复用+frame_id 变→stale err；scan_roots 访问 Array/Field/StructElem 的 gc_ref。
- **`size_of::<Value>()==16`** 编译期断言保留（回归护栏）。
- **回归 golden / e2e**（既有用例已覆盖 4 变体的用户可见行为，靠 GREEN 兜）：
  - `ref`/`out`/`in` 参数（Ref）— e2e ref-out 用例
  - stack closure / lambda（StackClosure）— closure e2e
  - `struct[]` 元素读写（StructRefHeap）— struct-heap-inline e2e
  - FFI `PinPtr`/`UnpinPtr`（PinnedView）— native-interop 集成测试（`--tests`）
- **VM 验证**：`xtask test`（完整 GREEN gate）+ `cargo test --release --tests --no-run`
  （编集成测试，避免 [[xtask-test-excludes-cargo-test]] 漏网）+ **自举 5/5 gen1==gen2 逐字节**。
- **性能 A/B**：前端 typecheck big.z42 hyperfine（基线 `/tmp/z42vm_vcopy_base` = origin/main），
  目标复现 spike 的 ~1.2×；profile 确认 `Value::clone` 离开头部、`drop<Frame>` 大降。
- **JIT 内存**：跑一个 struct[]/closure 密集的 JIT 用例，确认 arena 随帧 truncate（不无界增长）。
