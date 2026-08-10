# Spec: struct 值相等（`==` / `!=`）

## ADDED Requirements

### Requirement: blob 值 struct 的 `==` / `!=` 是字段级值相等

两个同类型 blob 值 struct（`StructLayout.IsBlobStruct` 为真：`FieldCount>=2` 且 `Size>0`）用
`==` / `!=` 比较时，结果由**全部展平叶子的值相等合取**决定，而非句柄身份。

#### Scenario: 扁平 struct 全字段相等
- **WHEN** `struct P { int x; int y; }`，`var a = new P(1,2); var b = new P(1,2);`
- **THEN** `a == b` 求值为 `true`，`a != b` 求值为 `false`

#### Scenario: 扁平 struct 存在不等字段
- **WHEN** `var a = new P(1,2); var b = new P(1,9);`
- **THEN** `a == b` 求值为 `false`，`a != b` 求值为 `true`

#### Scenario: 嵌套 struct 字段递归比较
- **WHEN** `struct Line { P a; P b; }`，两个 `Line` 的所有 4 个展平叶子（`a.x/a.y/b.x/b.y`）分别相等
- **THEN** `line1 == line2` 为 `true`；任一嵌套叶子不同 → `false`

#### Scenario: string 叶子按内容相等
- **WHEN** `struct Tagged { P pt; string name; }`，两个 `Tagged` 的 `pt` 各叶子相等且 `name`
  内容相同（`Arc<str>` 不同实例但同内容）
- **THEN** `t1 == t2` 为 `true`（`string` 叶子走现有 `Eq` = 内容相等，非引用相等）

#### Scenario: 短路——首个不等叶子即停
- **WHEN** 第一个叶子已不等
- **THEN** 结果为 `false`（`!=` 为 `true`），后续叶子不再读取（`BrCond` 短路到失败分支）

#### Scenario: 非 struct 操作数不受影响
- **WHEN** `==` / `!=` 的操作数是基元（`int`/`bool`/`char`/`float`）、`string`、引用类型对象或
  单叶子 wrapper（非 blob struct）
- **THEN** 仍发射单条 `EqInstr` / `NeInstr`（`_emitCompare` 原路径），行为不变

## MODIFIED Requirements

### Requirement: struct 操作数的 `==` 语义

**Before:** `p1 == p2`（p1/p2 为多字段 struct）发射单条 `EqInstr`，VM 比 `StructRef` 句柄
（arena `idx`+`frame_id`）→ 两个同值 struct 恒判不等。

**After:** 编译器检测两操作数均为 blob 值 struct 时，脱糖为逐叶子值比较的短路合取；结果为字段级
值相等。非 blob 操作数路径不变。

## IR Mapping

- **不新增 opcode / 不 bump 格式。** 仅复用现有指令：
  - `StructFieldGetPrim`（0xC2）——按编译期烘焙的 byte offset + TypeTag 读每个叶子（基元走字节
    codec，`string`/object/array 叶子走 refs 侧表）
  - `Eq`（0x30）——每叶子一次值比较（`string` 内容相等 / object 引用相等由现有 `Eq` 提供）
  - `ConstBool`（0x02）——写最终 `true`/`false` 结果
  - `BrCond`（0x41）/ `Br`（0x40）——短路控制流 + 结果汇合（镜像三目 `_emitConditional` 的
    result-reg-in-branches 范式）

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer —— 无
- [ ] Parser / AST —— 无
- [ ] TypeChecker —— 无（struct 现为类类型，`==`/`!=` 已合法产出 bool；仅确认无回归）
- [x] IR Codegen —— `ExprEmitter._emitBinary` 分流 + `_emitStructEquality` / `_emitLeafEqChecks`
- [x] VM interp —— 无新指令；沿用现有 `StructFieldGetPrim` / `Eq` / `BrCond` handler
