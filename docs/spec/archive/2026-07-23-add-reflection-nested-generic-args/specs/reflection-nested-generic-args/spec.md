# Spec: 嵌套泛型反射参数

## ADDED Requirements

### Requirement: 嵌套构造泛型的类型实参可递归反射

`typeof` 一个含嵌套构造泛型的类型时，其 `GetGenericArguments()` 返回的每个实参
Type 自身保留其构造泛型信息（可继续 `GetGenericArguments()`），任意深度。

#### Scenario: 一层嵌套 GetGenericArguments
- **WHEN** 存在 `class Box<T> {}` 与 `class Pair<A,B> {}`，求
  `typeof(Box<Pair<int,string>>).GetGenericArguments()`
- **THEN** 返回长度 1 的数组，`[0]` 是 `Pair<int,string>` 的构造 Type
- **AND** `[0].IsGenericType == true` 且 `[0].IsGenericTypeDefinition == false`
- **AND** `[0].GetGenericArguments()` 返回 `[typeof(int), typeof(string)]`

#### Scenario: 多层嵌套
- **WHEN** 求 `typeof(Box<Pair<Box<int>,string>>).GetGenericArguments()[0].GetGenericArguments()`
- **THEN** 返回 `[Box<int> 构造 Type, typeof(string)]`
- **AND** 该 `Box<int>` 构造 Type 的 `GetGenericArguments()` 返回 `[typeof(int)]`

#### Scenario: 平铺（非嵌套）泛型不回归
- **WHEN** 求 `typeof(Box<int>).GetGenericArguments()`
- **THEN** 返回 `[typeof(int)]`（与 add-reflection-generic-type-definition 行为一致）
- **AND** `typeof(Pair<int,string>).GetGenericArguments()` 返回 `[typeof(int), typeof(string)]`

#### Scenario: 嵌套实参名（Name）
- **WHEN** 求 `typeof(Box<Pair<int,string>>).GetGenericArguments()[0].Name`
- **THEN** 返回 `"Pair"`（基定义简单名，与顶层构造 Type 的 Name 语义一致）

#### Scenario: 实例路径一致
- **WHEN** `new Box<Pair<int,string>>()` 的 `obj.GetType().GetGenericArguments()[0]`
  （注：实例嵌套 args 由 instance-generic-args 路径产出，本变更保证其与 typeof 一致或
  按既有实例路径行为——见 design Testing Strategy）
- **THEN** 若实例路径已携带嵌套 type_args，则反射结果与 `typeof` 形式一致

## MODIFIED Requirements

### Requirement: Typeof 指令的泛型实参编码

**Before:** z42c `_typeofName` 把 `Typeof` 的**构造泛型实参**（本身是 instantiated）压成裸定义名
`"Pair"`——`<int,string>` 在发射时即丢失；runtime 无从重建嵌套。

**After:** z42c `_typeofArgName` 递归产**带尖括号的完整实参名** `"Pair<int,string>"`，塞进
`TypeofInstr` 现有 `string[]` 实参槽（**wire 布局 / TypeofInstr 接口 / zbc·zpkg 版本全不变**）；
runtime `make_type_from_name` 遇 `<...>` 按括号深度递归解析 → 嵌套构造 `Std.Type`。

## IR Mapping

- `Typeof` opcode（0x73）编码**不变**：`dstTag | dstId:u16 | TypeName:u32 | count:u8 | [argStr:u32]×count`。
- 唯一差异：构造泛型实参的 `argStr` 由 `"Pair"` 变为 `"Pair<int,string>"`（串**内容**带括号，
  非 wire 布局变化）。**无格式 bump**（zbc 1.28 / zpkg 0.33 不动）。

## Pipeline Steps

- [ ] Lexer — 无（不新增语法）
- [ ] Parser / AST — 无（嵌套泛型已由 `_parseType` 的 `>>/>>>` 拆分支持）
- [ ] TypeChecker — 无（`Z42InstantiatedType.TypeArgs` 已是树）
- [x] IR Codegen — `_emitTypeof` 实参改用递归 `_typeofArgName`（ExprEmitter.z42），`TypeofInstr` string[] 不变
- [ ] zbc writer/reader — 无（wire 布局不变）
- [x] VM interp — `make_type_from_name` 括号解析 → `make_constructed_type`（逐 arg 递归）
- [x] JIT — 无需改（`jit_typeof` 仍 marshal `*const String`；解析在 `make_type_from_name` 共用路径）
