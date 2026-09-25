# Spec: 值类型永不可空

## ADDED Requirements

### Requirement: 值类型不接受 null

#### Scenario: null 赋给值类型局部
- **WHEN** 编译 `int x = null;`
- **THEN** 报 `NullToValueType`，编译失败
- **注**：此前**编译通过**

#### Scenario: null 作值类型实参
- **WHEN** `void F(int a)` 配 `F(null)`
- **THEN** 报 `NullToValueType`

#### Scenario: 值类型字段初始化器为 null
- **WHEN** `class C { int n = null; }`
- **THEN** 报 `NullToValueType`

#### Scenario: 引用类型不受影响
- **WHEN** `string s = null;` / `object o = null;`
- **THEN** 编译通过（引用类型可空，本变更不改）

### Requirement: 值类型与 null 比较是错误

#### Scenario: 值类型变量与 null 比较
- **WHEN** `int n = 0; if (n == null) { }`
- **THEN** 报 `ValueTypeNullComparison`，消息说明"值类型永不为 null，此比较无意义"
- **注**：此前静默恒假——代码看起来做了检查，分支永不执行

#### Scenario: 值类型字段与 null 比较
- **WHEN** `class C { int n; void M() { if (this.n != null) { } } }`
- **THEN** 报 `ValueTypeNullComparison`

#### Scenario: 未约束泛型参数不报
- **WHEN** `void F<T>(T v) { if (v == null) { } }`
- **THEN** 编译通过 —— `T` 既可能是值类型也可能是引用类型，保守不报

#### Scenario: object 与 null 比较合法
- **WHEN** `object o = 42; if (o == null) { }`
- **THEN** 编译通过 —— `object` 是引用类型

### Requirement: 值类型不允许 `?` 标注

#### Scenario: 可空值类型局部
- **WHEN** 编译 `int? a = 42;`
- **THEN** 报 `NullableValueTypeNotSupported`

#### Scenario: 可空值类型返回值带迁移提示
- **WHEN** 编译 `int? TryParse(string s) { ... }`
- **THEN** 报 `NullableValueTypeNotSupported`，消息含「`int? F(...)` → `bool F(..., ref int v)`」

#### Scenario: 引用类型 `?` 标注仍合法
- **WHEN** 编译 `IPAddress? TryParse(string s)`
- **THEN** 编译通过 —— 引用类型的 `?` 在本变更中保持现状（语义由后续 change 赋予）

### Requirement: 存储按声明类型零初始化

#### Scenario: 静态值类型字段无初始化器
- **WHEN** `static int N;` 后读取 `N`
- **THEN** 得 `0`，不抛异常
- **注**：此前 `__box_prim: expected integer value, got Null`

#### Scenario: 实例值类型字段无初始化器
- **WHEN** `class C { int n; bool b; char c; double d; }` 后 `new C()` 读各字段
- **THEN** 依次得 `0` / `false` / `'\0'` / `0.0`
- **注**：此前 `corelib/assemblyloadcontext.rs:37` 一律填 `Value::Null`

#### Scenario: 值类型数组元素
- **WHEN** `int[] a = new int[10];` 读 `a[3]`
- **THEN** 得 `0`（非泛型路径既有行为，本变更保持）

#### Scenario: struct 的引用叶子仍为 null
- **WHEN** struct 含引用类型字段且未初始化
- **THEN** 该字段为 `null` —— 引用类型的零值就是 null，合法

#### Scenario: JIT 路径一致
- **WHEN** 同一段读未初始化值类型字段的代码在解释器与 JIT 下分别执行
- **THEN** 两者结果一致 —— `jit/frame.rs:138` 与解释器初始化同步

### Requirement: 拆箱分两段检查

#### Scenario: 拆箱 null
- **WHEN** `object o = null; int x = (int)o;`
- **THEN** 抛 `NullReferenceException`，消息形如「cannot unbox null to `int`」，**带源位置**
- **注**：此前（#717 之后）静默得到 null，再在下游某处以内部错误崩溃

#### Scenario: 拆箱类型不符
- **WHEN** `object o = "hi"; int x = (int)o;`
- **THEN** 抛 `InvalidCastException`，消息形如「object holds `string`, not `int`」

#### Scenario: 两条错误可分辨
- **WHEN** 分别触发上述两种情形
- **THEN** 异常类型与消息不同 —— 错因不同，不得合成一条

#### Scenario: 正常拆箱不受影响
- **WHEN** `object o = 42; int x = (int)o;`
- **THEN** 得 `42`

---

## MODIFIED Requirements

### Requirement: stdlib 的「可能没有」API 规约

#### Scenario: 值类型结果用 ref 出参
- **WHEN** 调用 `int v = 0; bool ok = Int32.TryParse("42", ref v);`
- **THEN** `ok == true`，`v == 42`

#### Scenario: 解析失败写零值
- **WHEN** `int v = 99; bool ok = Int32.TryParse("abc", ref v);`
- **THEN** `ok == false`，`v == 0` —— 失败路径显式写零值

#### Scenario: Guid 同样迁移
- **WHEN** `Guid g; bool ok = Guid.TryParse("not-a-guid", ref g);`
- **THEN** `ok == false`，`g` 为全零 Guid（`Guid` 是 struct）

#### Scenario: 引用类型 API 不变
- **WHEN** `IPAddress? a = IPAddress.TryParse("192.168.1.1");`
- **THEN** 编译通过，行为不变 —— 引用类型走单个可空返回值，不加 bool

#### Scenario: Version 的唯一调用点
- **WHEN** `Version.Parse("1.2.3.x")` 触发 `_parseComponent` 的失败路径
- **THEN** 抛 `FormatException`，消息不变 —— 内部改用 `ref` 出参，对外行为一致
