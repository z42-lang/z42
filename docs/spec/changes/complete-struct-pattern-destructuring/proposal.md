# Proposal: 补齐 struct record 模式解构的两个 E0402 defer（嵌套 struct 字段 + boxed struct subject）

## Why

#316 给 struct record 加了位置/属性解构，但**显式 defer 了两类情况并报 E0402**（`DiagnosticCodes.TypeMismatch`）：

1. **嵌套 struct 字段**：struct record 的字段本身又是 struct record（`struct Line(Point A, Point B)` →
   `Line(a,b)=l`）。binder `_guardNestedStructField`（`PatternBinder.z42:227-233`，报 :231）拦下，因为 emit 侧
   `_emitPatFieldRead` 对 struct 容器只发**单条** `StructFieldGetPrimInstr`，字段是 struct 时会把子 blob 误当叶子
   基元解码 → 崩「初版限基元 + 引用字段」。
2. **boxed struct subject**：subject 是 `object`/接口持有装箱 struct（`object o = point; o is Point(x,y)`）。
   binder `_guardStructSubject`（`PatternBinder.z42:213-221`，报 :219）拦下，因为 emit 侧 struct 模式恒
   `needTest=false`（`_isBlobStruct` 为真）→ 跳过 `IsInstance` 类型测试 → `o` 非 `Point` 时误匹配，且把
   `object` 句柄当裸 blob 误读。

**关键发现：两类 defer 所需的运行时原语都已就绪**——`exec_object.rs:388`（BoxedStruct 的 `IsInstance`）、
`:407-421`（`AsCast`/`unbox_struct` 拷到 arena `StructRef` 值副本）、`exec_struct.rs:189-204`（BoxedStruct base
的 `StructFieldGetPrim`）、`corelib/convert.rs:54`（`__box_struct`）。这两个 defer**纯是编译器 emit/binder 未接线**，
无需动 Rust 运行时。补齐它们让 struct record 模式解构达到与 class record 对等的完整覆盖。

## What Changes

- **嵌套 struct 字段递归读**：emit 侧 `_emitPatFieldRead` 在 struct 容器分支里，先查 `Layouts.FieldIsStruct(sname, fieldName)`；
  是 struct 字段则走 `StructAllocInstr` + copyRegion 产出**子 blob 句柄**（值副本），返回该句柄让
  `EmitMatch`/`EmitIrrefutable` 递归下降到子模式；否则维持现有单条 `StructFieldGetPrim`。**模型直接抄
  `AccessEmitter._emitBlobFieldGet`（:219-235）**。binder 移除 `_guardNestedStructField`（递归绑定本已就绪，
  仅被 guard 拦在前）。
- **boxed struct 拆箱 + 类型守卫**：binder 放开 `_guardStructSubject`——subject 是 `object`/接口/可含该 struct 的
  类型时不再报错（仍拒绝无关不相容类型）。emit **打开类型测试**：subject 静态即该 struct → 走现路径
  （`needTest=false` 直读 blob）；subject 是 boxed（静态非该 struct）→ `needTest=true` 发 `IsInstanceInstr`，
  通过后先发 `AsCastInstr` 拆箱得 arena `StructRef` 句柄，再以该句柄作 base 逐字段读。**参考类型模式
  `BoundTypePattern` 的 IsInstance→BrCond→AsCast 三段式（`PatternEmitter.z42:42-58`）**。建议 boxed 路径统一
  先 AsCast 拆箱成值副本，下游全部复用现有值-subject 读路径（三条 base 来源 offset 基准不同，统一拆箱风险最低）。
- **无新语法、无 token、无 zbc/zpkg 格式 bump、无 runtime 改动**（运行时原语全就绪，纯 semantics emit/binder 补线）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | MODIFY | ①`_emitPatFieldRead`(:233-245) struct 分支加 `Layouts.FieldIsStruct` 判 → 嵌套 struct 字段走 `StructAlloc`+copyRegion 产子 blob 句柄递归；②`needTest` 决策(:64,70) 区分值 subject vs boxed subject，boxed 时打开 IsInstance + 插 AsCast 拆箱 |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | ①移除/放宽 `_guardNestedStructField`(:227-233)；②放宽 `_guardStructSubject`(:213-221) 接受 boxed subject（仍拒不相容类型）|
| `src/compiler/z42c.semantics/src/AccessEmitter.z42` | MODIFY | `_copyRegion`(:365，现 private) 提为 internal（或加 `_copyStructRegion` 包装），供 PatternEmitter 经 ExprEmitter 复用嵌套 struct 值副本逻辑；避免复制逻辑分叉 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY(若需) | 若 `_copyRegion` 经 ExprEmitter 转发，加转发方法（`_access` 现 private） |
| `src/tests/pattern-matching/pattern_tests.z42` | MODIFY | :59-65（嵌套 `Line(a,b)=l`）、:67-73（boxed `o is Point(x,y)`）从「期望 E0402」改为「期望成功匹配 + 字段值正确」；jit 双验 |
| `src/tests/pattern-matching/pattern_struct_complete.z42` | NEW(若需) | 补充 e2e：多层嵌套 struct（`Triangle(Line(Point(x,_),_),_)`）、boxed struct 在 switch/is/解构声明三位点；jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 两个 defer 从 Deferred 移除；记录嵌套 struct 值副本（StructAlloc+copyRegion）+ boxed 拆箱（AsCast→StructRef）机制 |

**只读引用**（理解上下文必须读，不修改）：
- `src/compiler/z42c.semantics/src/AccessEmitter.z42` — `_emitBlobFieldGet`(:219-235) 嵌套 struct 读模型
- `src/libraries/z42.ir/src/…/StructLayout.z42` — `FieldIsStruct`(:238)/`StructSize`(:58)/`IsBlobStruct`(:197)/`FieldByteOffset`
- `src/runtime/src/interp/exec_object.rs` — IsInstance(:388)/AsCast·unbox(:407-421)（boxed 支持）
- `src/runtime/src/interp/exec_struct.rs` — StructFieldGetPrim boxed base(:189-204)/`unbox_struct`(:67)
- `src/runtime/src/corelib/convert.rs` — `__box_struct`(:54)
- `src/compiler/z42c.semantics/src/PatternEmitter.z42` — `BoundTypePattern` 三段式(:42-58)、`_emitFieldSeq`(:210-227)

## Out of Scope

- 泛型 struct record 解构（由 `add-generic-record-destructuring` / struct 布局工作推进）
- 任何 zbc/zpkg 格式变更、任何 Rust 运行时变更（原语全就绪，本变更纯 emit/binder）

## Open Questions

- [ ] `_copyRegion` 提 internal vs 经 ExprEmitter 转发 vs PatternEmitter 内复制——三选一，design 定（推荐提
  internal，避免逻辑分叉；跨类边界低风险）。
- [ ] boxed 路径统一先 AsCast 拆箱成 arena StructRef 再按 struct 相对 offset 读——确认三条 base 来源
  （arena StructRef / boxed / 对象内联）offset 基准差异下，拆箱后统一走值-subject 路径正确（`exec_struct.rs:226`
  `as_struct_ref` 分支）。
- [ ] `CheckIrrefutable`（`PatternBinder.z42:43,53`）对嵌套 struct 字段的精确类型校验，在 `_guardNestedStructField`
  移除后不被误放/误拒——解构声明路径 `EmitIrrefutable`(:159-196) 也调 `_emitPatFieldRead`，需一并验证。
