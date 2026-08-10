# Proposal: struct 值相等（`==` / `!=` 逐叶子值比较）

> 「struct 值类型完备化」工作流 **PR1**。PR2 = struct→object 健全装箱（blob 拷到堆 + boxed struct 的
> Equals/ToString/GetHashCode/GetType/is/as），本 PR 落地后单独 DRAFT，其 `Equals` 复用本 PR 的值相等语义。
> User 裁决：保留「unboxed struct 无 vtable、编译器合成值方法」的既有设计，struct 当 object 用靠装箱非形式继承。

## Why

struct 值语义程序 ② 项。当前多字段 blob 值 struct 的 `==` / `!=` 仍走引用语义——两个操作数
持 `Value::StructRef{idx, frame_id}` 句柄，VM 的 `Eq` 用 derived `PartialEq` 比 arena 下标，
**两个字段完全相同的 struct 恒判不等**（`new P(1,2) == new P(1,2)` → `false`）。这与「struct 是
C# 真值类型」的语义相违背：值类型的相等应是**字段级值相等**。

不做的话，用户拿 struct 当值类型用（放进逻辑判断、去重、断言）都会得到反直觉的错误结果，且与
①（嵌套字段值语义）已建立的「struct 赋值/传参/复制都是值语义」形成不一致的坑。

## What Changes

- `p1 == p2` / `p1 != p2`（两操作数均为 blob 值 struct，同类型）在编译器 `ExprEmitter` 里
  **脱糖为逐叶子值比较的短路合取**：递归展平到真叶子（嵌套 struct 字段一并展开），每个叶子用
  **现有** `StructFieldGetPrim` 取值 + **现有** `Eq` 比较，用 `BrCond` 短路——任一叶子不等即
  整体不等。
- **无新 IR 指令、无 zbc/zpkg 格式 bump**——与 ①「嵌套整字段复制逐叶子分解、复用现有指令」同范式
  （纯前端脱糖）。叶子比较语义由现有 `Eq` 自动继承：基元→值相等、`string`→内容相等、
  `object`/`array`→引用相等（符合 z42 对象 `==` 默认语义 + C# `ValueType.Equals` 对引用字段的行为）。
- 非 blob struct 操作数（基元 / 引用类型 / 单叶子 wrapper）**不受影响**，仍走原 `_emitCompare`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `_emitBinary` 对 blob struct 的 `==`/`!=` 分流到新 `_emitStructEquality`；新增 `_emitStructEquality` + `_emitLeafEqChecks`（逐叶子比较-短路，镜像 `_copyRegion` 的叶子枚举） |
| `src/tests/types/struct_equality.z42` | NEW | golden：扁平/嵌套/含 string 叶子 struct 的 `==`·`!=` 正负用例（断言自检，EXIT=0） |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 加「struct 值相等」小节；「收敛面与延后」里 `struct==` 从 ⏳ Deferred 移到 ✅ |

**只读引用**（理解上下文必须读，不修改）：

- `src/compiler/z42c.semantics/src/StructLayout.z42` — `IsBlobStruct` / `LayoutOf` / 叶子字段数组 / `StructLeafKind`
- `src/libraries/z42.ir/src/IrInstr.z42` — `StructFieldGetPrimInstr` / `EqInstr` / `ConstBoolInstr`
- `src/libraries/z42.ir/src/IrTerminator.z42` — `BrTerm` / `BrCondTerm`
- `src/tests/types/struct_nested.z42` — ① 的 golden 范式参考

## Out of Scope

- **单标量叶子 struct 塌缩**（`GCHandle` 等 `FieldCount<2`）——保持现有标量模型（Phase B），其 `==`
  已由现有基元/句柄比较处理，不在本变更。
- **`<` `<=` `>` `>=`**：struct 无序关系，类型检查器本就不允许，本变更只接 `==` / `!=`。
- **跨包 struct 值相等**：依赖 P4 跨包布局元数据；本变更只覆盖当前 z42c 已能布局的同工程 struct
  （与 ① 同边界）。
- **对象/数组叶子的深比较**：明确选择引用相等（复用现有 `Eq`），不递归进堆对象。

## Open Questions

- 无（设计已在探索阶段与 User 确认走「方案 B：编译器脱糖，无 bump」）。
