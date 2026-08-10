# Design: struct→object 健全装箱 + 身份（PR2a）

> 「struct 值类型完备化」工作流 PR2a。保留 `z42.core/Object.z42` 的「unboxed struct 无 vtable、编译器合成
> 值方法」既有决定——struct 当 object 用靠**装箱**桥接（C# `ValueType`/boxing 模型），非形式 base/vtable。

## Architecture

```
装箱  (BoxIfNeeded 扩)         拆箱 ((P)o, AsCast 扩)
  var p = new P(1,2)            P q = (P)o
  object o = p                     │
     │ TypeChecker.BoxIfNeeded      │ exec: as_cast / convert
     │ (blob struct→object)         │  BoxedStruct → alloc 当前帧 arena slot
     ▼                              ▼  拷 bytes + clone refs → StructRef(值)
  BoundBox(kind=struct)         q 是独立值副本
     │ ExprEmitter._emitBox
     ▼
  ConstStr "Demo.P"
  Builtin __box_struct %o,%p,%cls      ┌─ Value::BoxedStruct(Box<BoxedStruct{
     │ VM builtin_box_struct(ctx)      │     type_name: Arc<str>,
     ▼  读 StructRef arena slot        │     bytes: Box<[u8]>,     ← blob 快照
  堆 BoxedStruct(拷 bytes+clone refs)  │     refs:  Box<[Value]>,  ← 引用叶子(GC 扫)
                                       └─  }))
身份:  o.GetType()→type_name   o is P / is object→true   o as P→拆箱 / as object→保持
GC:    trace BoxedStruct → 扫 refs
```

## Decisions

### Decision 1: 堆表示 = 新 `Value::BoxedStruct`（owned blob），非 ScriptObject、非扩 BoxedPrim
**问题：** boxed struct 存哪种堆表示？
**选项：** A ScriptObject（type_desc+slots）——需给 struct TypeDesc 加 Object base+四方法=**加 vtable，反转
既有决定**；B 扩 `BoxedPrim{class,inner:Value}` 的 inner 承载 blob——inner 到处假设单标量，污染 prim 路径；
C 新 `Value::BoxedStruct(Box<BoxedStruct{type_name,bytes,refs}>)`。
**决定：** 选 **C**。owned `bytes`（blob 快照）+ `refs: Box<[Value]>`（引用叶子作真 Value → GC 扫 + 内存安全，
镜像 `struct_arena::StructSlot` 去掉 `frame_id`）+ `type_name`。unboxed struct 仍无 vtable；boxed 的对象协议
由 VM 按变体原生回答（身份）/ 2b 合成方法（Equals 等），非 struct vtable。**payload 不存 layout**——size=
`bytes.len()`、拆箱 alloc 用它，GetType/is 只需 type_name（避免存 `Arc<StructTypeLayout>`）。

### Decision 2: 装箱 kind 走 `BoundBox` 扩（prim/struct），发射复用 Builtin opcode → 无格式 bump
**决定：** `BoundBox` 加 kind 标记（`prim` 现状 / `struct` 新），或并列 `BoundBoxStruct`（实现时取更小 diff 者；
倾向给 `BoundBox` 加一个 `IsStruct`/kind 字段）。`_emitBox` 对 struct kind 发 `ConstStr(structFQ)` +
`Builtin(dst,"__box_struct",[structHandle,cls])`——与 `__box_prim` 同范式，**复用 Builtin 0x51，无新 opcode、
无 zbc/zpkg bump**（同 [[primitive-value-boxing]] 的关键省法）。

### Decision 3: `BoxIfNeeded` 扩为「基元 或 blob struct 擦除即装箱」
**决定：** `TypeChecker.BoxIfNeeded`（现 `!(vt is Z42PrimType)` 即 return）改为：`vt` 是整型基元（现状）
**或** `vt` 是 blob 值 struct（`Layouts.IsBlobStruct(vt.Name())`）且 target 擦除到 object/接口 → 插
`BoundBox`（对应 kind）。集中一处覆盖所有 coercion 点（var-decl / return / arg via `BoxArgs` / array-store /
params-tail）——与 prim 装箱同插入面，无需逐点改。

### Decision 4: 拆箱 `(P)o` = AsCast/convert 扩，堆 blob 拷回当前帧 arena StructRef
**决定：** object→blob struct 的 `(P)o` / `o as P`：VM 分配**当前帧** arena slot（size=`bytes.len()`，
`frame_id`=当前帧）、`copy_from_slice(bytes)` + `refs.clone()` 填入、返回 `Value::StructRef`。`as P` 精确
类型匹配才拆箱，否则（as object/base）保持 boxed、（不兼容）Null。实现落 `exec_value.rs` convert 路径 +
`exec_object.rs::as_cast` 的 BoxedStruct 分支。

### Decision 5: 身份查询分支（is/as/GetType）+ provisional `==`
**决定：** `is_instance` / `as_cast`（`exec_object.rs`）+ `builtin_obj_get_type`（`corelib/object.rs`）加
`BoxedStruct` 分支：`GetType`→`make_type_from_name(type_name)`；`is P`/`is object`/`is Object`→true（镜像
Boxed prim 的 Std.Object 短路），`is 其它`→false；`as P`→拆箱、`as object`→保持、else→Null。
**`==`（PartialEq）provisional**：2a 给 `BoxedStruct` 一个**值相等**分支（type_name 相等 ∧ bytes 相等 ∧
refs 逐 Value 相等）以给出确定答案——⚠️ bytes 比较对 float NaN 会误判等，且 `==` vs `Equals` 语义（值 vs
引用装箱）最终由 **2b** 与合成 `Equals` 统一裁定（Open Question，见 proposal）。2a 不依赖此语义正确性通过测试。

### Decision 6: GC——boxed struct 是根/被根引用即扫其 refs
**决定：** trace/scan 加 `BoxedStruct` 分支遍历 `refs`（每个引用叶子 Value 递归 trace）。owned `Box` 内联在
Value 里（8B 指针），随 Value trace；`bytes` 纯基元无需扫。**无写屏障问题**——boxed struct 存进堆对象字段时，
其 refs 作为该 Value 的一部分被对象 trace 覆盖（与 Boxed prim 同）。

## Implementation Notes

- `builtin_box_struct(ctx, args)`：`args[0]`=StructRef（装箱点 struct 活，slot 有效）、`args[1]`=类型名 Str。
  `ctx.struct_arena.lock().with(idx, frame_id, |slot| ...)` 读 slot → `bytes.to_vec().into()` + `refs.to_vec().into()` +
  `type_name=args[1]` → `Value::BoxedStruct(Box::new(...))`。幂等：`args[0]` 已是 BoxedStruct 直接返回。
- 拆箱 alloc：复用 `struct_arena.alloc(size, frame_id)` 得 slot idx，`copy_from_slice` + refs 写入，返回
  `StructRef{idx, frame_id=当前帧}`。
- **边界**：boxed struct 存 `object[]` / 堆对象字段 → 值随 Value 走，refs 被容器 trace（D6）。空 struct 不
  IsBlobStruct → 不装箱（走原引用路径，与今日一致）。
- **Rust 侧对称**：`Value` 新变体牵动 Debug/PartialEq/trace/（可能 Clone 派生）——逐个补 `BoxedStruct` 分支，
  非测试代码不 `unwrap`（`anyhow`）。

## Testing Strategy

- **Rust 单测**（`exec_struct_tests.rs` 或新）：box→模拟帧退出（truncate arena）后经 boxed 值 GetType/is/拆箱
  不 stale；装箱值快照（改原 arena slot 不影响 boxed bytes）。
- **Golden e2e**（`src/tests/types/struct_boxing.z42`，断言自检 + EXIT=0）：
  - `object o = new P(1,2)` 跨函数返回后 `o.GetType()` 名 == "P"、`o is P` / `o is object` true、`o is Q` false。
  - `P q=(P)o; Assert.Equal(1,q.x)`；装箱快照独立性（`p.x=99` 后 `((P)o).x==1`）。
  - 含 string 叶子 struct 装箱后拆箱内容保真。
- **完整 `xtask test` GREEN**（**不传 `Z42_HOME`**）+ self-host 5/5 + `cargo test --lib`（Rust 单测，见
  [[xtask-test-excludes-cargo-test]]——VM 改动必跑）。
