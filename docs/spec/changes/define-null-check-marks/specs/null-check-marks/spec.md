# Spec: 引用类型的空检查标记

## ADDED Requirements

### Requirement: `?` 标记产生检查义务

#### Scenario: 解引用标 `?` 的返回值未检查
- **WHEN** `IPAddress? TryParse(string)` 配 `var a = IPAddress.TryParse(s); a.ToString();`
- **THEN** 报 `PossibleNullDereference`，编译失败
- **注**：此前编译通过，运行期崩

#### Scenario: 检查后解引用
- **WHEN** `var a = IPAddress.TryParse(s); if (a != null) { a.ToString(); }`
- **THEN** 编译通过

#### Scenario: 早退式检查后解引用
- **WHEN** `var a = IPAddress.TryParse(s); if (a == null) { return; } a.ToString();`
- **THEN** 编译通过 —— `if` 分支不正常结束 ⇒ 其后 `a` 为 NotNull

#### Scenario: 被调方必须检查标 `?` 的形参
- **WHEN** `void F(string? s) { print(s.Length); }`
- **THEN** 报 `PossibleNullDereference`

#### Scenario: 调用方传什么给标 `?` 的形参都合法
- **WHEN** `void F(string? s)` 配 `F(null)` / `F("x")` / `F(maybeNull)`
- **THEN** 全部编译通过 —— 形参标 `?` 只约束被调方

#### Scenario: 未标的东西不产生义务
- **WHEN** `string Find(int id)`（未标）配 `Find(1).Length`
- **THEN** 编译通过 —— 缺席 = 不强制，**不是**保证非空

### Requirement: 义务点清单

#### Scenario: 各类解引用
- **WHEN** 对 MaybeNull 值执行 `x.M()` / `x.F` / `x[i]` / `x + y` / `foreach (var e in x)` / `throw x` / `lock x` / `using x`
- **THEN** 逐一报 `PossibleNullDereference`

#### Scenario: 与 null 比较不是义务点
- **WHEN** `if (x == null)` / `if (x != null)`（`x` 为 MaybeNull）
- **THEN** 不报 —— 这正是检查本身

### Requirement: 传播点 —— return

#### Scenario: 把 MaybeNull 返回给未标的返回类型
- **WHEN** `string F() { var a = MaybeNullCall(); return a; }`
- **THEN** 报 `PossibleNullReturn`

#### Scenario: 给返回类型加 `?` 后合法
- **WHEN** 同上但签名改为 `string? F()`
- **THEN** 编译通过，义务转移到 `F` 的调用方 —— **传播是自愿的**

### Requirement: 窄化

#### Scenario: 短路求值
- **WHEN** `if (x != null && x.Length > 0) { }`
- **THEN** 编译通过

#### Scenario: 三目
- **WHEN** `int n = (x != null) ? x.Length : 0;`
- **THEN** 编译通过

#### Scenario: 赋非空值后
- **WHEN** `string? s = MaybeNullCall(); s = "x"; s.Length;`
- **THEN** 编译通过

#### Scenario: 循环中重新赋值使窄化失效
- **WHEN** `if (x != null) { while (c) { x.M(); x = MaybeNullCall(); } }`
- **THEN** 报 `PossibleNullDereference` —— 循环回边把 MaybeNull 带回 `x.M()`

#### Scenario: try 块里的窄化不进 catch
- **WHEN** `try { if (x != null) { risky(); } } catch (Exception e) { x.M(); }`
- **THEN** 报 `PossibleNullDereference` —— 异常可能在窄化之前抛出

### Requirement: 标 `?` 的字段必须先快照

#### Scenario: 直接解引用标 `?` 的字段
- **WHEN** `class C { string? Name; void M() { if (this.Name != null) { this.Name.Length; } } }`
- **THEN** 报 `NullableFieldNotSnapshotted`，消息给出快照写法

#### Scenario: 快照后合法
- **WHEN** `var n = this.Name; if (n != null) { n.Length; }`
- **THEN** 编译通过

#### Scenario: 未标的字段不受影响
- **WHEN** `class C { string Name; void M() { this.Name.Length; } }`
- **THEN** 编译通过 —— 未标 = 不强制

### Requirement: `Expect("理由")`

#### Scenario: 解除义务
- **WHEN** `var cfg = this._cache.Expect("构造器里已填"); cfg.M();`
- **THEN** 编译通过；`cfg` 为 NotNull

#### Scenario: 运行期真检查
- **WHEN** 上述 `_cache` 实际为 null
- **THEN** 抛异常，消息含「构造器里已填」与源位置 —— **不是**静默通过

#### Scenario: 必须给理由
- **WHEN** `x.Expect()`
- **THEN** 报 `ExpectRequiresLiteralReason`

#### Scenario: 理由必须是字面量
- **WHEN** `x.Expect(someVariable)`
- **THEN** 报 `ExpectRequiresLiteralReason` —— 理由要能被 grep 和 review

### Requirement: 反向推导

#### Scenario: 函数体 return null 但未标
- **WHEN** `string F() { if (c) { return null; } return "x"; }`
- **THEN** 报 `UnmarkedNullReturn`，提示把返回类型改为 `string?`

#### Scenario: 标了之后合法
- **WHEN** 同上但签名为 `string? F()`
- **THEN** 编译通过

### Requirement: override / 接口一致性

#### Scenario: 子类给返回类型加 `?`
- **WHEN** 基类 `virtual string F()`，子类 `override string? F()`
- **THEN** 报 `NullableOverrideMismatch` —— 经基类调用的人没有义务，会漏

#### Scenario: 子类给返回类型去 `?`
- **WHEN** 基类 `virtual string? F()`，子类 `override string F()`
- **THEN** 编译通过 —— 子类给了更强的保证

#### Scenario: 子类给形参加 `?`
- **WHEN** 基类 `virtual void F(string s)`，子类 `override void F(string? s)`
- **THEN** 编译通过 —— 子类接受更多输入

#### Scenario: 子类给形参去 `?`
- **WHEN** 基类 `virtual void F(string? s)`，子类 `override void F(string s)`
- **THEN** 报 `NullableOverrideMismatch`

### Requirement: `?` 不进入类型身份

#### Scenario: 泛型实参
- **WHEN** `List<string>` 与 `List<string?>`
- **THEN** 是**同一个类型**，可互相赋值，无诊断

#### Scenario: 重载判重
- **WHEN** 同一类型中同时声明 `void F(string s)` 与 `void F(string? s)`
- **THEN** 报重复定义 —— 签名相同

#### Scenario: 跨包携带
- **WHEN** pkgA 导出 `string? Find()`，pkgB 调用后直接解引用
- **THEN** pkgB 报 `PossibleNullDereference` —— 标记随签名文本跨包

---

## REMOVED Requirements

### Requirement: `??` 空合并运算符

#### Scenario: 使用 `??`
- **WHEN** 编译 `var s = a ?? "default";`
- **THEN** 报 `ObsoleteNullOperator`，消息给出 if 写法

### Requirement: `?.` 空条件成员访问

#### Scenario: 使用 `?.`
- **WHEN** 编译 `var v = node?.value;`
- **THEN** 报 `ObsoleteNullOperator`
- **注**：`?.` 坏在"检查了但把 null 往下游传"，把 bug 挪到离现场更远的地方

#### Scenario: 生成的外语源串不受影响
- **WHEN** android/ios appbuilder 拼接 `"dest.parentFile?.mkdirs()"` 这类 Kotlin/Swift 源串
- **THEN** 不受影响 —— 那是字符串内容，不是 z42 语法
