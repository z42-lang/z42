# Proposal: struct→object 健全装箱 + 身份（GetType / is / as）

> 「struct 值类型完备化」工作流 **PR2a**。PR2b（boxed struct 的 `Equals`/`GetHashCode`/`ToString`
> 完整对象协议，复用 PR1 `_emitLeafEqChecks` 合成值方法）随后单独 DRAFT。
> User 裁决：保留「unboxed struct 无 vtable、编译器合成值方法」既有设计（`z42.core/Object.z42`），
> struct 当 object 用靠**装箱**而非形式继承/vtable。

## Why

`object o = someStruct;` 今天**类型合法但运行期不健全**：
- 类型检查器有无条件规则（`TypeFactsTc.z42:40`「任何类型可赋给 object」）→ 赋值通过；
- 但 `BoxIfNeeded`（`TypeChecker.z42:44`）**只给整型基元装箱**，blob 值 struct（`Z42ClassType`）直接跳过
  → **裸拷 `Value::StructRef{idx, frame_id}` 句柄进 object 槽**；
- `StructRef` 是 **arena 帧作用域句柄**，创建帧一退出即 LIFO-truncate（`vm_context.rs:1099`）→ 之后经该
  object 访问 = **use-after-free / stale 崩**（`struct_arena.rs:175` "value-struct lifetime unsound"）；
- 且 `is`/`as`/`GetType` 无 `StructRef`/boxed-struct 分支 → 即便未失效也答错类型。

不修：任何把值 struct 存进 `object`（变量 / 参数 / `object[]` / 返回）的程序都是潜在崩溃或错误类型查询。
这是选项 B 值语义落地后暴露的**真健全性缺口**。

## What Changes

- **运行时新增 boxed-struct 表示**：`Value::BoxedStruct`（堆、GC 扫描、**拥有** blob 字节+引用叶子副本 +
  类型名），生命周期脱离帧 arena。
- **装箱插入**：扩 `TypeChecker.BoxIfNeeded`——blob 值 struct 擦除到 `object`/接口时插 `BoundBox`；
  `_emitBox` 对 struct 发射 `__box_struct` builtin（**复用现有 Builtin opcode 0x51**，同 `__box_prim`，
  **无 zbc/zpkg 格式 bump**）。VM builtin 把 `StructRef` 的 arena blob 拷进堆 `BoxedStruct`。
- **拆箱**：`(P)o`（object→blob struct）扩 `AsCast` 分支——把堆 `BoxedStruct` blob 拷回**当前帧**新
  arena `StructRef`，恢复为值 struct。
- **身份查询**：`GetType()` / `is` / `as` 加 `BoxedStruct` 分支（类型名驱动，`is object`/`is P` 真；
  `as P` 精确匹配→拆箱、`as object`→保持、否则 Null）。
- **GC**：trace/scan 加 `BoxedStruct` 分支扫描其 `refs` 引用叶子。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/types.rs` | MODIFY | 新 `Value::BoxedStruct(Box<BoxedStruct>)` 变体 + `BoxedStruct{type_name,bytes,refs}` 结构；PartialEq 分支（2a provisional 值相等，见 design D5）；Debug |
| `src/runtime/src/metadata/trace.rs`（或 types.rs 内 trace） | MODIFY | GC trace/scan 加 `BoxedStruct` 分支扫 `refs` |
| `src/runtime/src/corelib/convert.rs` | MODIFY | 新 `builtin_box_struct(ctx,args)`：读 `StructRef` arena slot → 拷 bytes+clone refs → `Value::BoxedStruct` |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `("__box_struct", convert::builtin_box_struct)` |
| `src/runtime/src/corelib/object.rs` | MODIFY | `builtin_obj_get_type` 加 `BoxedStruct` 分支（type_name → make_type_from_name） |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `is_instance` / `as_cast` 加 `BoxedStruct` 分支（is object/P；as P 拆箱 / as object 保持 / else Null） |
| `src/runtime/src/interp/exec_value.rs` | MODIFY | `(T)x` convert 路径：`BoxedStruct`→blob struct 拆箱（alloc 当前帧 arena StructRef + 拷回） |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | `BoxIfNeeded` 扩：blob struct 擦除到 object/iface → `BoundBox`（区分 prim vs struct 装箱 kind） |
| `src/compiler/z42c.semantics/src/Bound.z42` | MODIFY | `BoundBox` 携带装箱 kind（prim / struct）或新 `BoundBoxStruct`（design D2 定） |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `_emitBox` 对 struct kind 发 `__box_struct`（ConstStr 类型名 + Builtin） |
| `src/runtime/src/interp/exec_struct_tests.rs`（或新 rust 单测） | NEW/MODIFY | box→存object→帧退出后仍可 GetType/is/as（不悬垂）单测 |
| `src/tests/types/struct_boxing.z42` | NEW | golden：`object o=struct` 跨帧存活 + `o.GetType()` + `o is P` + `(P)o` 拆箱值独立性 |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 「struct→object 装箱」小节 + Deferred 更新 |

**只读引用**：`struct_arena.rs`（copy_into/alloc/slot 形状）、`exec_struct.rs`（resolve_layout）、
`vm_context.rs`（帧 struct_base/frame_id）、`StructLayout.z42`（IsBlobStruct）、既有 `__box_prim`/`BoxedPrim`
（prim 装箱范式镜像）。

## Out of Scope（→ PR2b 或更后）

- **boxed struct 的 `Equals` / `GetHashCode` / `ToString`**：需编译器**合成 struct 值方法**（`Equals` body
  复用 PR1 `_emitLeafEqChecks`）+ boxed 对象协议 vcall 派发 → **PR2b**。2a 只保证不悬垂 + 身份正确。
- **`==` on boxed structs 的最终语义**：2a 给 PartialEq 一个 provisional 值相等（design D5），最终由 2b 与
  合成 `Equals` 统一裁定。
- **跨包 struct 装箱**：依赖 P4 跨包布局元数据（当前 boxed struct 靠 arena slot 的 bytes+refs 自足拷贝，
  同工程内 OK；跨包类型解析留 P4）。
- **JIT 值路径**（P5）。

## Open Questions

- [ ] D5：`(object)s1 == (object)s2` 的语义——值相等（值类型直觉）还是引用相等（C# 装箱语义）？2a 暂定
      provisional 值相等，请 User 在 2b 一并裁定（design 已记）。
