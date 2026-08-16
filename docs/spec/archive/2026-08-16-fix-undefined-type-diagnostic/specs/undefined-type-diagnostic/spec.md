# Spec: 未定义类型引用诊断

## ADDED Requirements

### Requirement: 未定义类型名在任何类型注解位置报 E0443

#### Scenario: 局部变量声明引用未定义类型
- **WHEN** 函数体内 `C c;` 且 `C` 不是已声明类型 / 泛型形参 / 内建类型
- **THEN** 报 `E0443 undefined type: C`（指向 `C` 的 span）

#### Scenario: 字段 / 参数 / 返回类型引用未定义类型
- **WHEN** `Undef f;`（字段）/ `void P(Undef a)`（参数）/ `Undef Ret()`（返回）
- **THEN** 各报 `E0443 undefined type: Undef`

#### Scenario: 泛型实参 / 数组元素为未定义类型
- **WHEN** `List<C> x;` 或 `C[] a;`（`C` 未定义）
- **THEN** 报 `E0443 undefined type: C`（经 `CheckTypeRef` 对 `Z42InstantiatedType` 实参 /
  `Z42ArrayType` 元素的递归）

#### Scenario: cast / as / is / typeof / new / default / catch 目标为未定义类型
- **WHEN** `(C)x` / `x as C` / `x is C` / `typeof(C)` / `new C()` / `default(C)` / `catch (C e)`（`C` 未定义）
- **THEN** 报 `E0443 undefined type: C`

## MODIFIED Requirements

### Requirement: `new <未定义类型>()` 统一报 E0443（原 E0401 双报收敛）

此前 `_bindNew`（`ExprTyper.z42`）在 `new C()` 的 `C` 解析为 Unknown 时**单独**发一条
`E0401 unknown type in new`，随后又调 `_chkTypeRef`。引入 E0443 后 `_chkTypeRef` 亦对该未定义
类型报 E0443 → **同一错误双重报告**。

- **WHEN** `new C()` 且 `C` 未定义
- **THEN** **仅**报 `E0443 undefined type: C`（删去 `_bindNew` 内的 E0401 特例，交由唯一
  choke point `CheckTypeRef` 统一报）——与其它类型注解位置一致，不再双报。

> Scope 注：为此在允许改动清单外追加 `src/compiler/z42c.semantics/src/ExprTyper.z42`（删 3 行
> 冗余 E0401），属单一 choke point 设计（design D1）的直接后果。

## 不报（负向场景，防误报）

#### Scenario: `var` 推断不报
- **WHEN** `var x = 5;`
- **THEN** 无 E0443（`var` 在 `_varType` 中于 `_chkTypeRef` 前被过滤，走 init 推断）

#### Scenario: 泛型形参不报
- **WHEN** 泛型方法/类体内引用其类型形参 `T`（如 `T tmp;`）
- **THEN** 无 E0443（`T` 经携形参的 `ResolveTypeP` 解析为 `Z42GenericParamType`，非 Unknown）

#### Scenario: 合法已声明 / 已 import / 内建 / 嵌套类型不报
- **WHEN** 引用本包类、`using` 到的跨包类、内建类型（`int`/`string`/…）、嵌套类型 `Outer.Inner`
- **THEN** 无 E0443

## IR Mapping
无（纯类型检查期诊断，不产 IR / 不改 zbc·zpkg 格式）。

## Pipeline Steps
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [x] TypeChecker — `AccessChecker.CheckTypeRef` 新增未定义类型分支；`SymbolTable.ResolveTypeP` 记未解析名
- [ ] IR Codegen — 无
- [ ] VM interp — 无
