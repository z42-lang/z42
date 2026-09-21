# Spec: 单一 `ref` 参数修饰符

## REMOVED Requirements

### Requirement: `out` 参数修饰符

#### Scenario: 源码使用 `out` 作形参修饰符
- **WHEN** 编译 `bool TryParse(string s, out int v)`
- **THEN** 报 `ObsoleteParamModifier`，消息含迁移提示「`out T x` → `ref T x`」，编译失败

#### Scenario: 源码使用 `out` 作调用点修饰符
- **WHEN** 编译 `TryParse(s, out v)` 或 `TryParse(s, out var v)`
- **THEN** 报 `ObsoleteParamModifier`，消息含「`out v` → `ref v`」/「`out var v` → `ref var v`」

### Requirement: `in` 参数修饰符

#### Scenario: 源码使用 `in` 作形参修饰符
- **WHEN** 编译 `int DoubleIt(in int x)`
- **THEN** 报 `ObsoleteParamModifier`，消息含「`in T x` → `T x`（按值）或 `ref T x`（需写回）」

#### Scenario: `in` 仍是 foreach 的关键字
- **WHEN** 编译 `foreach (var x in list) { }`
- **THEN** 正常编译 —— `in` 只是不再作参数修饰符，词法关键字身份不变

---

## ADDED Requirements

### Requirement: 调用点修饰符必须与形参对称

#### Scenario: 形参有 `ref`，实参漏写
- **WHEN** `void Inc(ref int x)` 配 `Inc(c)`
- **THEN** 报 `RefArgModifierMissing`，编译失败
- **注**：这是本变更修掉的首要静默缺陷——此前编译通过且**写入静默丢失**

#### Scenario: 形参无 `ref`，实参多写
- **WHEN** `void Show(int x)` 配 `Show(ref c)`
- **THEN** 报 `RefArgModifierUnexpected`，编译失败

#### Scenario: 形参声明增删 `ref` 后调用点失配
- **WHEN** 已有 `void F(int x)` 与调用 `F(c)`，把声明改成 `void F(ref int x)`
- **THEN** **同包内**所有调用点报 `RefArgModifierMissing` —— 声明变更必被发现

#### Scenario: 跨包调用暂不检查（已知缺口）
- **WHEN** pkgB 调用 pkgA 导出的 `void F(ref int x)` 而漏写 `ref`
- **THEN** 本变更**不报**——`TsigTypeName` 不记录 `ref`，`ImportedSymbolLoader` 无 `IsRef`，
  跨包侧拿不到修饰符信息
- **注**：这是**已知且已记录**的缺口（`DiagnosticCodes.z42` 的 E0465 注释早已写明
  「`ref`/`out` 在 TSIG 格式里根本不记录……实测调用点少写 ref 照样编译通过、修改丢失」）。
  由 follow-up change `record-ref-in-signature` 修（需 minor bump）。
  **在它落地前，不得让 stdlib 导出任何 `ref` 形参的公开 API** —— 否则等于把这个静默缺口
  推给用户包。`enforce-value-type-non-null` 的 TryParse 迁移因此必须排在其后。

### Requirement: `ref` 实参与形参类型精确匹配

#### Scenario: 类型不匹配
- **WHEN** `void Inc(ref int x)` 配 `long n = 0; Inc(ref n);`
- **THEN** 报 `RefArgTypeMismatch`，编译失败
- **注**：此前 `BoundRefArg` 以 `Z42UnknownType` 构造，转换分类器对 unknown 吸收 ⇒ **不报错**

#### Scenario: 基本类型别名视为同一类型
- **WHEN** `void Inc(ref i32 x)` 配 `int c = 0; Inc(ref c);`
- **THEN** 编译通过（`int` ≡ `i32`，Canon 归一后相同）

#### Scenario: 不做隐式数值转换
- **WHEN** `void Take(ref long v)` 配 `int c = 0; Take(ref c);`
- **THEN** 报 `RefArgTypeMismatch` —— 转换会产生临时值，地址失去意义

### Requirement: `ref` 仅用于值类型

#### Scenario: `ref` 用于引用类型形参
- **WHEN** 编译 `void Swap(ref string a, ref string b)`
- **THEN** 报 `RefParamNotValueType`，编译失败

#### Scenario: `ref` 用于 struct
- **WHEN** 编译 `void Normalize(ref Vec3 v)`（`Vec3` 是 struct）
- **THEN** 编译通过

#### Scenario: `ref` 用于 enum
- **WHEN** 编译 `void Advance(ref State s)`（`State` 是 enum）
- **THEN** 编译通过

### Requirement: 被 `ref` 传递的未初始化局部自动取零值

#### Scenario: 无初始化器的局部直接传 `ref`
- **WHEN** `int v; Inc(ref v);` 且 `void Inc(ref int x) { x = x + 1; }`
- **THEN** 编译通过，运行后 `v == 1` —— `v` 在声明处零初始化

#### Scenario: 未被 `ref` 传递的局部不发零值初始化
- **WHEN** 函数含 `int v;` 但 `v` 从未作为 `ref` 实参出现
- **THEN** 该函数的 IR 指令流与本变更前 **byte-identical**（保持「无 init 不发 IR」）

#### Scenario: struct 局部被 `ref` 传递
- **WHEN** `Vec3 v; Normalize(ref v);`
- **THEN** 编译通过，`v` 的 blob 在声明处按布局零初始化

### Requirement: `ref var` 内联声明

#### Scenario: `ref var` 声明并传址
- **WHEN** `if (Int32.TryParse("42", ref var n)) { }`
- **THEN** 编译通过；`n` 在调用点声明、类型取形参类型、初值为零值；调用后 `n` 在作用域内可见

#### Scenario: `ref` + 显式类型内联声明
- **WHEN** `Int32.TryParse("42", ref int n);`
- **THEN** 等价于 `ref var n`，且 `n` 的声明类型必须与形参类型精确匹配，否则 `RefArgTypeMismatch`

#### Scenario: 被调方未写回时读到零值
- **WHEN** `bool F(ref int v) { return false; }` 配 `F(ref var n); print(n);`
- **THEN** 编译通过，输出 `0` —— 这是砍掉 `out` 的已知代价，语义明确不是未定义

### Requirement: `ref _` 丢弃符

#### Scenario: 丢弃出参
- **WHEN** `if (Int32.TryParse(s, ref _)) { }`
- **THEN** 编译通过；编译器分配隐藏零值槽；`_` **不**进入作用域

#### Scenario: `_` 不可读
- **WHEN** `Int32.TryParse(s, ref _); print(_);`
- **THEN** 报未定义符号 `_`

#### Scenario: 多个 `ref _` 互不冲突
- **WHEN** `F(ref _, ref _)`
- **THEN** 编译通过，两个隐藏槽独立

### Requirement: `ref` 实参必须是可取址左值（沿用既有行为）

#### Scenario: 属性 / 索引器 / blob struct 字段 / 静态字段
- **WHEN** 以上四种形态作 `ref` 实参
- **THEN** 报 `E0470`（既有行为，`fix-silent-semantic-gaps` #702 引入），编译失败

---

## MODIFIED Requirements

### Requirement: 重载判重不区分 `ref`

> **依赖 proposal Q1 的裁决。本节按选项 A（不参与重载）书写。**

#### Scenario: 同名同 arity 仅修饰符不同
- **WHEN** 同一类型中同时声明 `void F(int x)` 与 `void F(ref int x)`
- **THEN** 报重复定义（`_dupSigKey` 走 `MangleKey(name, ParamTypes, ParamCount)`，不含修饰符）

#### Scenario: 调用点漏写 `ref` 不会静默选中按值重载
- **WHEN** 仅存在 `void F(ref int x)`，调用 `F(c)`
- **THEN** 报 `RefArgModifierMissing`（而非回落到不存在的按值重载）

### Requirement: 转发生成器只生成 `ref`

#### Scenario: 转发一个含 `ref` 形参的方法
- **WHEN** `ForwardGenerator` 为含 `ref int` 形参的方法生成转发
- **THEN** 生成的签名与实参均带 `ref`，可编译且语义正确
- **注**：此前 `ForwardGenerator.z42:384` 对 `out`/`in` 形参一律生成 `ref `，三态消失后该缺陷自然消失
