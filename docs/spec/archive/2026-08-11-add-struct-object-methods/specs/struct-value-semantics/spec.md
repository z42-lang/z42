# Spec: struct 合成对象协议方法

## ADDED Requirements

### Requirement: blob 值 struct 有编译器合成的值语义 Equals / GetHashCode / ToString

每个 blob 值 struct（`IsBlobStruct`）自动获得 `Equals(object)` / `GetHashCode()` / `ToString()`（用户显式
声明同名则不合成）；boxed struct 经对象协议 VCall 派发到它们。

#### Scenario: Equals 同值
- **WHEN** `var a=new P(1,2); var b=new P(1,2); object o=b;` → `a.Equals(o)`（或 `((object)a).Equals(o)`）
- **THEN** `true`（逐叶子值相等，嵌套字段递归；string 叶子内容、object 叶子引用）

#### Scenario: Equals 异值 / 异类型
- **WHEN** `a.Equals(new P(1,9))` / `a.Equals("hi")` / `a.Equals(new Q(...))`
- **THEN** 分别 `false` / `false` / `false`（`other is P` 不成立即 false）

#### Scenario: GetHashCode 同值同 hash
- **WHEN** 两个字段相等的 P
- **THEN** `GetHashCode()` 相等（值语义 hash：Dictionary/HashSet 契约）

#### Scenario: ToString
- **WHEN** `o.ToString()`（o=boxed P）
- **THEN** 类型名字符串（如 `"P"`；C# `ValueType.ToString` 默认语义）

#### Scenario: 用户显式声明覆盖
- **WHEN** struct 源码已写 `public bool Equals(object o){...}` / `ToString()` 等
- **THEN** 用用户版本，不合成（不重复注册）

## MODIFIED Requirements

### Requirement: boxed struct 的对象协议 VCall

**Before（PR2a）:** boxed struct VCall 除 `GetType` 外 fallback `Std.Object.{method}`（this=boxed 值），
`Equals`/`ToString`/`GetHashCode` 无值语义（bail「PR2b 提供」）。

**After:** VCall 先 unbox `this` → StructRef，prepend `{type_name}.{method}$arity` 候选 → 命中合成方法
（值语义）；`GetType` 仍特判保留精确类型。

### Requirement: `==` on boxed struct（D5）

`==`/`!=` on `object`-typed boxed struct = **值相等**（`Value::BoxedStruct` PartialEq：type_name ∧ bytes ∧
refs）。`.Equals()` = 合成叶子方法（float 用 `Eq` → NaN≠NaN，精确）。二者仅 float NaN 边角微差（文档标注）。

## IR Mapping

- 合成 `{FQ}.Equals$1` / `.GetHashCode$0` / `.ToString$0` = 普通 IR 函数（`build_func_index` 按名注册）。
- Equals body：`IsInstance`(0x71) + `AsCast`(0x72，拆箱) + PR1 的 `StructFieldGetPrim`+`Eq`+`BrCond` 叶子链。
- GetHashCode body：`StructFieldGetPrim` 逐叶子 + 算术（Mul/Xor/Add）FNV 合并 + 引用叶子 `GetHashCode` VCall。
- ToString body：`ConstStr(typeName)` + ret（单块）。
- **无新 opcode / 无格式 bump**（全部复用现有指令 + builtin 派发）。

## Pipeline Steps

- [ ] Lexer / Parser / AST —— 无
- [ ] TypeChecker —— 无（合成在 IrGen；反射面 ExportedTypeExtractor 加签名）
- [x] IR Codegen —— IrGen 注入合成方法 + FunctionEmitter/ExprEmitter body 发射
- [x] VM interp / JIT —— boxed vcall 臂 unbox this + 候选 prepend（interp+JIT 对称）
