# Spec: 委托 / 函数类型值上的成员调用

## ADDED Requirements

### Requirement: `.Invoke(args)` 等价于 `(args)`

接收者的静态类型是委托类型（`delegate` 声明的类型、`Action` / `Func` / `Predicate`）或
函数类型（`(T) -> R`）时，`recv.Invoke(a, b, …)` 与 `recv(a, b, …)` **语义完全相同**，
包括返回类型、求值顺序与运行期派发路径。

#### Scenario: 局部变量上的 `.Invoke`
- **WHEN** `Action<string> a = (string s) => Console.WriteLine(s); a.Invoke("x");`
- **THEN** 打印 `x`
- **注**：此前编译期零诊断、运行期崩 `VCall: expected object, got FuncRef("Main__lambda_0")`，
  且**不可 `catch`**（`catch (Exception)` 接不住，程序当场终止）

#### Scenario: 有返回值的委托
- **WHEN** `Func<int, int> f = (int x) => x * 2; int r = f.Invoke(21);`
- **THEN** `r == 42`，且 `.Invoke` 表达式的静态类型是 `int`（不是 `Unknown`）

#### Scenario: 函数类型拼写
- **WHEN** `(int) -> int f = (int x) => x * 2; f.Invoke(21);`
- **THEN** 与上一条同结果 —— 委托与 `(T) -> R` 在编译器里是同一个类型对象 `Z42FuncType`

#### Scenario: 普通字段与 `event` 字段上的 `.Invoke`
- **WHEN** `class D { public event Action<string> OnKey; void Fire(string s) { var h = this.OnKey; if (h != null) { h.Invoke(s); } } }`
- **THEN** handler 被调用
- **注**：这正是 `delegates-events.md:62,264-266` 一直在教的**单播 event 官方触发模板**，
  此前照着写必崩

#### Scenario: 多参数委托
- **WHEN** `Action<int, int> f = (int a, int b) => …; f.Invoke(1, 2);`
- **THEN** handler 收到 `1, 2`（arity 1 与 2 此前均崩）

### Requirement: 委托值上不存在的成员报 E0401

委托 / 函数类型值上访问除 `Invoke` 以外的成员名，报 **E0401**。

#### Scenario: 不存在的方法名
- **WHEN** `(int) -> int f = (int x) => x * 2; f.Bogus();`
- **THEN** 编译期报 E0401，措辞点明接收者是委托类型
- **注**：此前零诊断、运行期崩 `VCall: expected object, got FuncRef`。
  与 prim 收者（#801）、型参收者（#833）同一条口径 —— 那两次收窄漏掉了委托这一格

#### Scenario: `Invoke` 不带括号
- **WHEN** `(int) -> int f = …; var g = f.Invoke;`
- **THEN** 编译期报诊断（方法组取引用形式不支持，见 proposal 的 Out of Scope）

### Requirement: 函数类型调用校验实参个数

对函数类型值的调用（`f(args)` 与 `f.Invoke(args)` 两种拼写），实参个数与形参个数不符时
报 **E1005**（太少）/ **E1006**（太多）。

#### Scenario: 多传实参
- **WHEN** `(int) -> int f = (int x) => x * 2; f(1, 2);`
- **THEN** 编译期报 **E1006**
- **注**：此前 🔴 **多余实参被静默丢掉** —— lambda 照常收到 `x=1`、返回 2，零诊断。
  这比崩溃更坏：错值一路流走

#### Scenario: 少传实参
- **WHEN** `(int) -> int f = (int x) => x * 2; f();`
- **THEN** 编译期报 **E1005**
- **注**：此前零诊断，形参拿到 `Null`，崩在**别的地方**
  （`type mismatch in arithmetic: Null vs I64(2)`，栈顶指向 lambda 体而非调用点）

#### Scenario: `.Invoke` 拼写同样校验
- **WHEN** `(int) -> int f = …; f.Invoke(1, 2);`
- **THEN** 报 E1006（新写法不得绕过校验）

#### Scenario: 实参类型不符照旧报 E0402
- **WHEN** `(int) -> int f = …; f("str");`
- **THEN** 报 E0402 `cannot assign string to Int32 (argument)`
- **注**：实参**类型**一直是查的，本变更只补个数

## UNCHANGED Requirements（须有阴性对照守住）

### Requirement: 多播 `Invoke` 与反射 `Invoke` 行为不变

#### Scenario: 多播 event 触发
- **WHEN** `class B { public event MulticastAction<int> Ev; } b.Ev.Invoke(5);`
- **THEN** 与本变更前逐字相同 —— `MulticastAction<T>` 是 `z42.core` 真类，
  走 class 分支，到不了新增的 `Z42FuncType` 分支

#### Scenario: 反射调用
- **WHEN** `methodof(X.M).Invoke(obj, args)` / `ConstructorInfo.Invoke(args)`
- **THEN** 行为不变（全仓 26 处 `.Invoke(` 全属此类）
- **注**：定位阶段提出过「`CallEmitter.z42:243` 的 DepIndex 捷径可能把 `Invoke/arity-N`
  劫持到 `MulticastAction.Invoke`、发出**直接 Call 到错函数**」的假说，
  实测 arity 1 / 2 均未复现；阴性对照仍须保留，它是「静默调错函数」这一最坏档的唯一守门
