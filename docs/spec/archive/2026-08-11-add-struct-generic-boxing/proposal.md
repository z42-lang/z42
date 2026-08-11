# Proposal: struct 泛型容器装箱（P3a）

> 「struct 值类型完备化」工作流 **P3a**（泛型边界装箱）。P3b（真内联进对象字段/数组 + 写屏障，格式 bump，
> 有写屏障设计分叉）后随。P3a **格式中立**——复用 PR2a/2b 的 `__box_struct` + `AsCast` 拆箱基础设施。

## Why

blob 值 struct 已能作值 + 装箱进 `object` 答全对象协议（#154/#156/#158），但**存进泛型容器崩**：
- 泛型路径把 struct 实参当 **未装箱 `Z42GenericParamType`（K/T）** 传入——`BoxIfNeeded` 只对 `object`/接口
  目标装箱，type-param 目标不装箱 → 裸 `StructRef` 流入。
- `Dictionary.Set` 的 `key.GetHashCode()`（Dictionary.z42:39）= 对 `StructRef` 的 VCall → 运行期崩
  「VCall: expected object, got StructRef」；且 `keys[slot]=key`（堆数组）存入帧作用域句柄 → 帧退出
  use-after-free。
- `d[key]=v` 的 indexer-set 走 `ExprTyper._bindAssign` 手搭路径，**绕过 BoxArgs**，键/值均不装箱。

→ `Dictionary<P,V>` / `List<P>` / `HashSet<P>`（经 Dictionary GetHashCode 路径）全不可用。

## What Changes（box 存入 + unbox 取出，闭环）

- **装箱（存入 type-param 存堆）**：
  1. `TypeChecker.BoxIfNeeded` 的 struct 分支 `erasesS` 谓词加 `|| (target is Z42GenericParamType)`——覆盖所有
     走 `BoxArgs` 的方法实参（`List.Add`/`Dictionary.Set`/`Contains`/ctor 等）。
  2. `ExprTyper._bindAssign` 的 `set_Item` indexer-write 分支（现绕过 BoxArgs）：对 index/value 按 `set_Item`
     的 `Signature.ParamTypes`（`Z42GenericParamType`）装箱。
- **拆箱（取出 type-param → 具体 struct）**：`get_Item` 与 `foreach` 元素类型 substitute 为 blob struct 时，
  把 retrieval 结果包 `BoundConvert(→P)` → 复用 `ExprEmitter._emitConvert` 的 `AsCast` 拆箱臂
  （`BoxedStruct → 当前帧 arena StructRef`），使 `P p = dict[k]` / `foreach(P p in list)` 得值 struct。
- **无新 IR / 无格式 bump**：`__box_struct`/`__box_prim` 是 Builtin-opcode builtin；拆箱是现有 `AsCast`；
  容器 backing 仍 `TKey[]/TValue[]/T[]`（运行期擦除），ABI 不变。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | `BoxIfNeeded` struct 分支 `erasesS` 加 type-param 目标；新增对称 `UnboxIfNeeded`（target 为 blob struct ∧ source 非 struct/Unknown → 包 BoundConvert 拆箱） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindAssign` set_Item 分支对 idx/val 装箱；`_bindIndex` get_Item 结果（V/T subst 为 blob struct）包拆箱 |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | `foreach` 循环变量：元素类型 blob struct 时对 get_Item 结果拆箱后再 writeback |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | `_substGeneric` 改 public（供 ExprTyper 检索拆箱）；`_bindInstanceMemberCall` 方法返回 type-param 为 struct → 包拆箱 |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `as_cast` 加 **StructRef 恒等臂**（已是值 struct → `as P` 原样返回；使泛型容器拆箱统一走 AsCast 时，元素是 StructRef（普通 P[]）不被误判 Null）——行为臂，**格式中立** |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | `jit_as_cast` 同上 StructRef 恒等臂（interp/JIT 对称） |
| `src/tests/types/struct_generic_container.z42` | NEW | golden：`Dictionary<P,int>` 存取+ContainsKey+覆盖、`List<P>` add/index/foreach/Contains、取出值独立、string 叶子键、非 struct 回归 |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 「泛型容器装箱」小节 + Deferred 更新（Dictionary<P,V> 从 Deferred 移到 ✅） |

**只读引用**：`Z42Type.z42`（`Z42GenericParamType`/`Z42InstantiatedType`/`_substGeneric`）、`OverloadBinder.z42`
（BoxArgs 调用）、`StmtBinder.z42`（var-decl/return 装箱点）、`Dictionary.z42`/`List.z42`（存取 + GetHashCode/
Equals 调用点）、`ExprEmitter._emitConvert`/`_emitBox`（拆箱/装箱发射）、boxed-vcall 臂（已工作）。

## Out of Scope（→ P3b / 后续）

- **struct 真内联进堆对象字段 / `struct[]` 字节 backing + 写屏障**（P3b，格式 bump，写屏障设计分叉）——本 PR
  只做**装箱**（容器存 boxed struct），非字节内联。密度收益在 P3b。
- 跨包 struct 泛型容器（依赖 P4 跨包布局）；JIT 值路径（P5）；B-radical。

## Open Questions

- 无重大分叉：box/unbox 路径由代码现状determined（复用现有基础设施）。实施中若发现 `get_Item` 类型 substitute
  或 foreach 拆箱有非平凡决策点，停下问 User。
