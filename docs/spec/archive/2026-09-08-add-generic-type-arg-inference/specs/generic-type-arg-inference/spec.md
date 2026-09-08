# Spec: 方法级类型实参推断 + 形参位类型实参代换

> capability：`generic-type-arg-inference` ｜ 配套 [proposal](../../proposal.md) / [design](../../design.md)

## MODIFIED Requirements

### Requirement: 泛型形参位的实参类型检查

**Before:** 形参类型含泛型形参时，实参一律被 `Conversion` 分支 B「恰一侧含泛型形参 → 擦除放行」
静默通过。检查只覆盖形参类型是**具体类型**的成员。

**After:** 当调用点能确定型参绑定（受者实例化 / 显式类型实参 / 推断成功）时，形参类型按绑定代换后
参与与赋值、`return`、var-decl **同一条**可转检查门（`TypeChecker.CheckImplicitConvert`）。
确定不了绑定时，行为与 Before 逐字节一致。

#### Scenario: 泛型类实例方法的实参被真检查

- **WHEN** `List<string> l = new List<string>(); l.Add(42);`
- **THEN** 报 `E0402`，消息含 `cannot assign int to String`，Span 指向实参 `42`

#### Scenario: 合法调用不报

- **WHEN** `List<string> l = new List<string>(); l.Add("ok");`
- **THEN** 诊断袋为空

#### Scenario: 显式类型实参的实参被真检查

- **WHEN** 同包声明 `T IdOf<T>(T a) { return a; }`，调用 `IdOf<string>(7)`
- **THEN** 报 `E0402`（`int` → `string`）

#### Scenario: 多实参不符逐条报

- **WHEN** `Dictionary<string,int> d = …; d.Set(7, "x");`
- **THEN** 报 **2** 条 `E0402`（键位与值位各一），不短路

### Requirement: 方法级 `where` 约束在推断调用上也校验

**Before:** 方法级约束只在**显式**写类型实参时校验（`Max<int>(a,b)` 校验、`Max(a,b)` 不校验）
——`_applyMethodTypeArgs` 在 `call.TypeArgCount == 0` 时第一行早退。

**After:** 推断成功时复用同一条 `ConstraintChecker.CheckMethod` 路径校验。推断失败时不校验（同 Before）。

#### Scenario: 推断出的类型实参违反约束

- **WHEN** 声明 `void f<T>(T a) where T : IFoo {}`、`class D {}`，调用 `f(new D())`
- **THEN** 报约束违反诊断（与显式写 `f<D>(new D())` **逐字相同**的码与消息）

## ADDED Requirements

### Requirement: 类型实参推断（结构化 unify）

从实参类型反推方法级型参绑定，省略 `<...>` 也能得到与显式写出**相同的类型检查结果**。

#### Scenario: 裸型参位推断

- **WHEN** 声明 `T IdOf<T>(T a)`，调用 `IdOf(7)`
- **THEN** 推断 `T = int`；调用合法、无诊断

#### Scenario: 数组元素位推断

- **WHEN** 声明 `void Copy<T>(T[] src, T[] dst, int n)`，调用 `Copy(byteArr, byteArr2, 3)`
- **THEN** 推断 `T = byte`；无诊断

#### Scenario: 推断出的绑定用于实参检查

- **WHEN** 声明 `void Pair<T>(T a, T b)`，调用 `Pair("s", 7)`
- **THEN** 同一型参绑到两个不同类型 ⇒ **整体推断失败** ⇒ 无诊断（与今天一致）
  > v1 刻意不做「最佳公共类型」；登记 Deferred `generic-inference-best-common-type`

#### Scenario: 型参未被任何形参位覆盖 → 整体失败

- **WHEN** 声明 `T Make<T>(int n)`，调用 `Make(3)`
- **THEN** 推断失败；不发任何诊断；编译行为与今天逐字节一致

#### Scenario: lambda 实参位跳过

- **WHEN** 形参类型含型参且对应实参是 lambda（绑定为 `Z42UnknownType`）
- **THEN** 该位不产生绑定信息；若因此型参未绑定 → 整体失败 → 无行为变化

### Requirement: 推断结果不改变发射

推断是**纯类型检查**能力，不改变任何发射决策。

#### Scenario: 隐式泛型调用的 opcode 不变

- **WHEN** `Array.Copy(src, dst, n)`（推断 `T` 成功）
- **THEN** `BoundCall.MethodTypeArgCount` 仍为 0 ⇒ 发 `Op.Call` 而非 `Op.CallGeneric`
- **AND** zbc 字符串池内容与基线逐字节一致

#### Scenario: 全仓自举字节不动点

- **WHEN** 用本 change 的 z42c 重建 stdlib + compiler 两轮
- **THEN** gen1 与 gen2 产物逐字节相同（`xtask test` 的 build wave 自带该校验）

### Requirement: callee 真消费型参时要求显式类型实参（阶段 D，可裁）

推断不回灌 `MethodTypeArgs` ⇒ 若 callee 体内消费型参，省略尖括号会让运行期读到空
`frame.method_type_args`。把该静默错值换成编译错误。

#### Scenario: 消费型参的 callee 被隐式调用

- **WHEN** 同包声明 `T[] MakeArr<T>(int n) { return new T[n]; }`，调用 `MakeArr(3)`
- **THEN** 报错，消息要求显式写出类型实参（给出 `MakeArr<T>(3)` 的修法）

#### Scenario: 不消费型参的 callee 不受影响

- **WHEN** `Array.Copy(src, dst, n)`（体内只转发到非泛型 `CopyRange`）
- **THEN** 无诊断

#### Scenario: 判定不到（导入方法无本地 Decl）→ 放行

- **WHEN** 跨包调用一个泛型方法且本地无 `Decl`
- **THEN** 不报错（= 今天行为，严格无回归）

## IR Mapping

**零新 IR 指令、零新 opcode、零格式 bump。**

- 本 change **不写** `BoundCall.MethodTypeArgs`（写入条件保持「仅显式类型实参」不变）
  ⇒ `ZbcInstr.z42:35-41` 的 `MethodTypeArgCount > 0` 分支不被新触发
  ⇒ 不产生 `Op.CallGeneric`(0xB4) / `Op.VCallGeneric`(0xB5)。
- 不新增字符串池条目（`IrInstrCall.StrCount()/StrAt()` 的输入不变）。
- 运行期 `frame.method_type_args`（`interp/frame.rs:43-48`）载体不动；
  native/JIT 快路径门 `method_type_args.is_empty()`（`exec_call.rs:135`）不受影响。
- zbc / zpkg minor **不 bump** ⇒ 不受 bootstrap-seed 两-nightly support/use 纪律约束。

## Pipeline Steps

受影响的 pipeline 阶段（按顺序）：

- [ ] **Lexer** —— 无改动（不引入新 token）
- [ ] **Parser · AST** —— 无改动（`CallExpr.TypeArgs` 为空本就合法）
- [ ] **SymbolCollector** —— 无改动（`MethodSymbol.TypeParamCount` 已存在且跨包已还原）
- [x] **TypeChecker** —— 唯一改动面：`MemberResolver` 三处接线 + 新增 `TypeArgInference`
- [ ] **IR Codegen** —— 无改动（不变式 I1：代换结果不进 emit 决策）
- [ ] **VM interp** —— 无改动
- [ ] **JIT / AOT** —— 无改动（interp 全绿前不碰）
