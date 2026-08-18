# Spec: Value-Copy 瞬态 arena 句柄

## ADDED Requirements

### Requirement: `Value` 是 `Copy`

`Value` 枚举实现 `Copy`：所有 payload 都是 `Copy`（标量、`Str`、`GcRef`、`VarGcRef`、
`{idx,frame_id}` 句柄），无 `Box`、无自定义 `Drop`。

#### Scenario: clone 是平凡拷贝
- **WHEN** interp/JIT 在寄存器间拷贝、传参、Mov、读字段一个 `Value`
- **THEN** 编译为 16B memcpy（`Value: Copy`），无判别号分支、无堆操作、无 drop-glue

#### Scenario: 帧析构 O(1)
- **WHEN** 一个 `Frame`（含 `regs: Vec<Value>`）drop
- **THEN** `Vec<Value>` 以 O(1) 释放缓冲（`Value: Copy` ⇒ 无逐元素 drop-glue）

#### Scenario: 尺寸不变
- **WHEN** 编译期 `size_of::<Value>()` 断言
- **THEN** 仍为 16B（`{idx:u32, frame_id:u32}` = 8B payload，不超过既有最大 payload）

### Requirement: `GcRef<T>` 是 `Copy`

`GcRef<T>` 删除显式 no-op `Drop`，实现 `Copy`。其 backing 是 POD 标记指针，无被拥有资源、
drop 本就 no-op；finalizer 仍在 sweep 触发（不受影响）。

#### Scenario: 句柄拷贝无副作用
- **WHEN** 拷贝一个 `GcRef`
- **THEN** 8B memcpy，无 refcount、无 finalize；GC 生命周期语义不变

### Requirement: 瞬态变体经 `TransientArena` 承载

`Ref`/`PinnedView`/`StackClosure`/`StructRefHeap` 的 payload 存进 per-`VmContext`
`TransientArena`；`Value` 只持 `{ idx, frame_id }` 句柄。

#### Scenario: 构造分配进 arena
- **WHEN** interp/JIT 产生一个上述变体（`LoadElemAddr`/`LoadFieldAddr`/`LoadLocalAddr` 的 `Ref`、
  `PinPtr` 的 `PinnedView`、`MkClos` stack-alloc 的 `StackClosure`、`ArrayGet` struct[] 元素的 `StructRefHeap`）
- **THEN** payload 以当前帧 `frame_id` push 进 `TransientArena`，返回 `idx`，寄存器存
  `Value::Variant { idx, frame_id }`

#### Scenario: 消费经 arena 解句柄
- **WHEN** 消费点读取 payload（`deref_ref`、`UnpinPtr`、FFI marshal、`FieldGet .ptr/.len`、
  `CallIndirect` 读 env/fn_name、`StructFieldGetPrim/SetPrim`、`__delegate_*`）
- **THEN** 经 `arena.with(idx, frame_id, |slot| …)` 校验 frame_id 后读 payload，行为与旧
  `Box` 直读一致

#### Scenario: 帧退出 LIFO 释放
- **WHEN** 创建这些句柄的帧 `pop_frame`
- **THEN** `TransientArena` truncate 回该帧入口 base，释放该帧所有瞬态 payload

#### Scenario: staleness 守卫
- **WHEN** 一个句柄在其创建帧退出后被解引用（逃逸分析误判 / bytecode 非法）
- **THEN** frame_id 不符 → 返回明确错误（非静默 use-after-free），与
  `StackObject`/`StructRef` 守卫同款

### Requirement: arena 作 GC root

`TransientArena` 被 GC 每次收集作 root 扫描，其内 `RefKind`(Array/Field 的 `gc_ref`)、
`StructArrayElem`(`arr`) 等 GC 引用叶子始终被标记。

#### Scenario: 句柄引用的堆对象存活
- **WHEN** GC mark 阶段，某帧寄存器持有 `Value::Ref{...}`（`RefKind::Array` 指向堆数组）
- **THEN** 该数组由 `TransientArena::scan_roots` 标记存活（不再穿句柄 `visit_gc_children` trace）

#### Scenario: 句柄变体不再穿 trace
- **WHEN** GC 追踪一个 `Value::Ref`/`StructRefHeap` 句柄的 children
- **THEN** `visit_gc_children` 对其为 no-op（reachability 由 arena root 保证，同 `StructRef`/`StackObject`）

## MODIFIED Requirements

### Requirement: 瞬态变体的相等 / stringify 退化为句柄级

**Before:**
- `Value::Ref` `==` 比较 `RefKind` 字段（Stack: frame_idx+slot；Array/Field: gc_ref ptr_eq + idx/name）
- `Value::PinnedView` `==` 比较 ptr+len+kind
- `value_to_str(StackClosure)` = `<closure {fn_name}>`；`value_to_str(StructRefHeap)` = `{type}{...}`

**After:**
- 4 变体 `==` 按 `{idx, frame_id}` 句柄相等（同 `StackObject` 先例）；不同句柄即不等
- `value_to_str` 4 变体降级为通用串（`<ref>` / `<pinned view>` / `<closure>` / `<struct value>`），
  照 `StackObject`/`StackArray`/`StructRef` 既有先例：ToString 是 escape sink，逃逸分析保证这些瞬态
  句柄永不到达用户可见 stringify 路径，此臂仅为防御性 fallback
- **理由**：这些变体是内部瞬态值，用户代码无「按值比较 / 直接 ToString」的可观察语义依赖；
  既有句柄变体（StackObject/StructRef）已如此，本变更保持一致

## IR Mapping

无。不涉及新 IR 指令 / 新 opcode / zbc·zpkg 格式变化（纯运行时 `Value` 表示）。

## Pipeline Steps

- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及
- [ ] TypeChecker — 不涉及
- [ ] IR Codegen — 不涉及
- [x] VM interp — 4 变体构造/消费 + arena
- [x] VM JIT — 同上（JIT 也产生 StructRefHeap/StackClosure）
- [x] GC — arena root 扫描 + 移除穿句柄 trace/mark
