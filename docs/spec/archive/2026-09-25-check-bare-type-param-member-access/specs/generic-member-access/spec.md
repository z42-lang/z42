# Spec: 型参收者上的成员可用性

**规则（本变更让它真正生效，而非新立）**：型参收者 `T` 上可用的成员 =
**`Std.Object` 继承来的成员** ∪ **该型参 `where` 约束（含父接口闭包、方法级 + 类级两个来源）
提供的成员**。其余一切**不可用**。

## MODIFIED Requirements

### Requirement A: 约束提供的**属性**在型参收者上读出真实值

**Before:** `where T : IHasName` 的 `a.Name` 返回 `null`（静默错值）；链式 `a.Name.Length` 崩
`FieldGet: not an object or known value type, got Null`。同一约束的**方法** `a.GetName()` 正常。

**After:** 属性与方法同口径，都返回真实值。

#### Scenario: 约束接口属性读
- **WHEN** `interface IHasName { string Name { get; } }`、`class Person : IHasName`，
  `string f<T>(T a) where T : IHasName { return a.Name; }`，求值 `f(new Person("Ada"))`
- **THEN** 结果为 `"Ada"`（**不是** `null`）
- **AND** 与非泛型 `new Person("Ada").Name` 逐字一致

#### Scenario: 约束接口属性的链式访问
- **WHEN** 同上，函数体为 `return a.Name.Length.ToString();`
- **THEN** 结果为 `"3"`，不抛

#### Scenario: 属性的静态类型不再是 Unknown
- **WHEN** `where T : IHasName` 的 `a.Name` 被赋给 `int n`（类型不符）
- **THEN** 编译期报 **E0402**（今天松绑成 Unknown ⇒ 零诊断）

#### Scenario: 类级约束来源同样生效
- **WHEN** `class Holder<T> where T : IHasName { string f(T a) { return a.Name; } }`
- **THEN** 结果为真实值（约束查找覆盖方法级与类级两个来源，与方法路同口径）

### Requirement B: 成员名在任何已知类型上都不存在时，编译期报错

**Before:** 型参收者上访问一个**压根不存在的成员名**（`a.Bogus` / `a.NoSuch()`）
→ **编译期零诊断**，运行期崩（`FieldGet: expected object, got BoxedStruct(…)` /
`VCall on boxed struct` / `FieldGet: not an object…`）。

**After:** 编译期报 **E0401**，指出名字不存在 + 可加约束。

#### Scenario: 成员名全仓不存在 → 报错
- **WHEN** `T f<T>(T a) { return a.Bogus; }`，且**没有任何已知类型**声明过 `Bogus`
- **THEN** 编译期报 **E0401**，消息含「no known type declares that name」
- **AND** `a.NoSuch()`（方法形态）同样报 E0401

#### Scenario: 🔴 收窄的支点 —— 名字存在于某个已知类型 ⇒ **不报**
- **WHEN** `class P { public int value; }` 存在，且 `T f<T>(T a) { return a.value; }`
- **THEN** **不报**（松绑照旧）
- **WHY** 引用类型经擦除返回位流出来时运行期派发正常，**全面判红会误报惯用写法**。
  实测证据（既有 e2e `generic_constraints` 抓下的）：
  `T Max<T>(T a, T b) where T : IComparable { … }` + `var m = Max(a, b); m.value`
  —— `Num` 是 class，**今天打 7**。按字面全面执行规则会把它判红。
- **AND** `where T : Animal`（**基类**约束）提供的 `pet.legs` / `pet.Describe()` 同样不报
  （既有 e2e `generic_baseclass`）

#### Scenario: 重载 / 属性的键形态不得误判成「不存在」
- **WHEN** 某类型只以 `Substring$2$int$int`（重载 mangle 键）或 `get_Name`（属性）形态声明该成员
- **THEN** 视为**存在**、不报（判「名字存不存在」必须按真实键形态查 —— #801 的同一个坑）

#### Scenario: 一个错误一条诊断
- **WHEN** `a.Bogus`
- **THEN** 恰好一条诊断

#### Scenario: 🔴 `id(v).X`（blob struct 经擦除返回位）仍不报、仍运行期崩
- **WHEN** `struct Vec2 { long X; long Y; }` + `T id<T>(T a)` + `id(v).X`
- **THEN** 编译期**不报**（`X` 存在于 `Vec2` 上），运行期行为与本变更前相同
- **WHY** 该格根因是泛型**特化**（实测两份 IR 只差 callee 名：`@id` vs `@id<Vec2>`），
  不是成员解析；报错只是换个说法、修不好它。已登记后续（见 design Deferred）。

### Requirement C: 方法级 `where` 细化类级型参时，约束在调用点被校验

**Before:** `class Box<T> { int Cmp() where T : IComparable { … } }` —— 方法级 `where` 细化类级
型参：成员可用（正确），但**约束满足性从不检查** ⇒ `new Box<Opaque>().Cmp()` 零诊断、
运行期崩 `VCall: function \`Opaque.CompareTo\` not found`。
根因是**互相推诿**：声明期在 `md.TypeParams.Count == 0` 早退，调用点对 `pi < 0` 静默跳过
并注明「归声明期报」。

**After:** 与「方法自己的型参」「类级型参」两种形态同口径，违反报 **E0402**。

#### Scenario: 细化约束不满足 → 编译期报错
- **WHEN** `class Opaque { }`（不实现 `IComparable`），`class Box<T> { public int Cmp() where T : IComparable { … } }`，
  求值 `new Box<Opaque>(…).Cmp()`
- **THEN** 编译期报 **E0402**，指出 `Opaque` 不满足 `IComparable`
- **AND** **不是**运行期 `VCall … not found`

#### Scenario: 细化约束满足 → 照常工作
- **WHEN** 同上类，`new Box<int>(3, 7).Cmp()`
- **THEN** 结果为 `-1`（与本变更前一致）

#### Scenario: 同类里不带约束的方法不受牵连
- **WHEN** `class Box<T> { int Cmp() where T : IComparable {…}  int Plain() { return 0; } }`，
  求值 `new Box<Opaque>(…).Plain()`
- **THEN** ✅ 正常（约束只约束声明了它的那个方法，**不上升为类级约束**）

#### Scenario: where 挂在既非方法级也非类级的名字上 → 报未知型参
- **WHEN** `class Box<T> { int f() where U : IComparable { return 0; } }`
- **THEN** 编译期报 **E0401**「where clause references unknown type parameter `U`」
  （与既有的方法级 / 类级同款措辞；今天因早退而**静默**）

#### Scenario: stdlib 的 `List<T>.Sort()` 声明约束后仍可正常使用
- **WHEN** `List<int>().Sort()` / `List<string>().BinarySearch(x)`
- **THEN** ✅ 照常工作
- **AND** `List<Opaque>` 本身的构造、`Add`、`Count` 等**不受影响**（类级无约束）

#### Scenario: 🔴 C 的作用域边界 —— 只在**同包**生效（实测，不是设计选择）
- **WHEN** 从**用户代码**（跨包）调 `List<Opaque>().Sort()`
- **THEN** **仍无诊断**（与本变更前相同），运行期行为不变
- **WHY** **TSIG 不导出方法级 `where`**（导入符号 `HasDecl=false`，元数据里没有 where 这一项）
  ⇒ **所有**方法级约束跨包都验不了，包括本变更前就存在的「方法自己的型参」那种：
  实测跨包 `Array.Sort<Opaque>(a)` **今天也无诊断**。类级约束能跨包是因为
  `add-associated-types PR-1` 专门给它加了通道。
- **AND** 这是**既存的、比本变更更大的洞**，已登记为独立后续（见 design Deferred）。
  本变更在同包侧新增了执行力，**不使任何情况变差**。
- ⚠️ 我最初把「方法自己的型参 → ✅ E0402」报成无条件成立，那是**只造了同包探针**的结论
  —— 覆盖面不足导致边界记宽。记在这里防止下一个人重犯。

## ADDED Requirements

### Requirement D: 放行面逐条不变（回归护栏）

**判据**：以下每条今天都能工作，本变更后**逐字不变**。

#### Scenario: `Object` 继承成员
- **WHEN** 裸型参 `a` 上调 `a.ToString()` / `a.GetHashCode()` / `a.GetType().Name` / `a.Equals(a)`
- **THEN** 全部照常工作；`ToString()` 仍正确派发到用户 `override`

#### Scenario: 约束提供的方法
- **WHEN** `where T : IColl` 的 `a.Add(1)` / `a.Size()`
- **THEN** 照常工作，实参检查与返回类型不变（`check-constraint-iface-method-args` 的 E0463 行为不变）

#### Scenario: 不访问成员的用法
- **WHEN** `Vec2 r = id(v);`（把擦除返回值赋给具体类型的局部，随后 `r.X`）
- **THEN** 照常工作，得 `7`

#### Scenario: `typeof(T)` 不受影响
- **WHEN** 泛型方法体内 `typeof(T)`
- **THEN** 行为不变（它不是成员访问；今天已由 E0455 守着）

#### Scenario: 自举与全仓不产生新红
- **WHEN** 跑完整 `xtask test`（含 z42c 自举、stdlib、examples）
- **THEN** 全绿；**若 stdlib 内部有依赖松绑的写法，必须逐处改成约束或显式类型实参，
  不得为了让门禁过而放宽判据**

## Pipeline Steps

- [ ] Lexer / Parser —— 不涉及
- [x] TypeChecker —— `MemberResolver` 型参收者两条路（方法 / 属性·字段）+ `ConstraintChecker` 细化约束校验
- [x] Bound 树 —— `BoundCall` 增「返回位是裸型参」标记（照 `RetIsNullable` 的接线）
- [ ] IR Codegen —— **零改动**（A 走既有 `BoundCall(get_X)` 发射；B 只报错不发射）
- [ ] VM interp / JIT —— **零改动**
