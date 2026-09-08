# z42 静态抽象接口成员 — 设计文档

> **状态**：设计（2026-04-24），实现尚未开始
> **参考**：C# 11 static abstract interface members + Rust traits
> **目标**：一套机制统一 operator 重载 / INumber 数值约束 / Rust trait 等价物

---

## 1. 动机

当前 z42 有**三套独立但功能重叠**的机制：

1. **C# 风 operator 重载**（commit `12a3854`）— `public static T operator +(T, T)` 静态方法
2. **INumber 实例方法接口**（commit `d5a1913`）— `interface INumber<T> { T op_Add(T other); }` 以实例方法形式
3. **普通 interface 虚方法**（L3-G2）— 泛型约束 + 虚派发

三套机制让 stdlib 和用户代码重复声明、语义混乱：
- `struct Vec2` 想参与泛型 `Sum<T: INumber<T>>` + 自己写 `a + b` —— 必须**同时**声明静态 operator + 实例 op_Add
- 编译器 desugar 必须"静态优先 / 实例兜底"双路径
- `T.Zero` / `T.Parse(s)` 这类类型级 API **完全无法**表达

**目标**：**单一机制**覆盖所有场景。

## 2. 参考设计

### 2.1 C# 11 Static Abstract Interface Members

C# 11（2022 年发布）引入 **static abstract interface members**，让接口可以声明静态算子和成员：

```csharp
public interface INumber<T> where T : INumber<T> {
    static abstract T operator +(T a, T b);
    static abstract T operator -(T a, T b);
    static abstract T Zero { get; }
    static abstract T Parse(string s);
}

public struct MyInt : INumber<MyInt> {
    public int value;
    public static MyInt operator +(MyInt a, MyInt b) => new(a.value + b.value);
    public static MyInt operator -(MyInt a, MyInt b) => new(a.value - b.value);
    public static MyInt Zero => new();
    public static MyInt Parse(string s) => new(int.Parse(s));
}

// 泛型代码通过约束派发到 T 的静态成员
public static T Sum<T>(IEnumerable<T> xs) where T : INumber<T> {
    T acc = T.Zero;                  // 派发到实现者的静态 Zero
    foreach (var x in xs) acc = acc + x;  // 派发到实现者的静态 operator +
    return acc;
}
```

**核心语义**：
- `static abstract` 成员在接口里是**契约声明**，不带实现
- 实现者类型必须提供**静态成员**，签名完全匹配（replacing T with self type）
- 泛型代码 `where T: I` 可以对 T 做类型级操作（`T.Member` / `a op b`）
- 运行时派发：JIT 通过泛型实例化（对 struct 是 monomorphized，对 class 是 shared code + TypeDesc）

### 2.2 Rust Traits（对照）

```rust
pub trait Num: Sized + Copy {
    fn zero() -> Self;
    fn parse(s: &str) -> Self;
}
pub trait Add<Rhs = Self> {
    type Output;
    fn add(self, rhs: Rhs) -> Self::Output;
}

pub fn sum<T>(xs: &[T]) -> T where T: Num + Add<Output = T> + Copy {
    let mut acc = T::zero();
    for &x in xs { acc = acc + x; }
    acc
}
```

**对比要点**：

| 特性 | C# 11 | Rust | z42 决策 |
|------|:-----:|:----:|:-------:|
| 接口声明静态方法 | ✅ | ✅ | ✅ |
| 接口声明静态算子 | ✅ `operator +` | ✅ `trait Add` | ✅ `operator +` |
| 类型级调用 `T::foo` | ✅ `T.foo()` | ✅ `T::foo()` | ✅ `T.foo()`（C# 语法）|
| 关联类型 | ❌ | ✅ `type Output` | ❌（L3-G3c 延后） |
| 高阶类型 HKT | ❌ | ❌ | ❌ |
| 默认方法（DIM） | ✅ C# 8+ | ✅ trait 默认实现 | ⏸ 延后 |
| 约束继承 | ✅ `I: J` | ✅ `trait A: B` | ⏸ 延后 |
| 特化 / 单态化 | JIT 混合 | 编译期单态 | 代码共享 + type_args |

**结论**：C# 11 的静态抽象机制 ≈ Rust traits 的 ~95%。z42 按 C# 11 设计即可。

### 2.3 C# 11 设计的不足 / z42 微调

| C# 11 问题 | z42 调整 |
|----------|---------|
| 实例 DIM（C# 8）历史包袱复杂（钻石问题、可见性） | **z42 不支持实例 DIM**（iter 1；iter 2+ 独立特性） |
| 静态默认实现通过 `static virtual`（无需实现端 `override`） | **z42 对齐 class 继承模型：接口声明 `static virtual`，实现端必须 `static override`**（双端显式、防拼写漂移） |
| `static abstract` 双关键字冗长 | **保留**（与 C# 语法兼容） |
| `T.StaticMember` 访问依赖 JIT 运行时类型参数 | **z42 采用值驱动派发**作为主模式（对 operator 场景足够）；类型级访问 `T.Zero` 延后到 iter 2 |
| CRTP 约束 `where T : INumber<T>` 语法冗余但必要 | **保留**，类型安全代价 |
| 没有关联类型 | **z42 同不支持**（与 C# 一致；等 L3-G3c） |
| `sealed` / `new` 修饰方法（C# 有）| **iter 1 不引入**；默认"不可 override"无需 `sealed` 修饰（与 class 实例方法同规则） |

---

## 3. z42 语法设计

### 3.1 接口声明

```z42
public interface INumber<T> where T : INumber<T> {
    // 静态抽象运算符
    static abstract T operator +(T a, T b);
    static abstract T operator -(T a, T b);
    static abstract T operator *(T a, T b);
    static abstract T operator /(T a, T b);
    static abstract T operator %(T a, T b);

    // 静态抽象方法（非运算符）
    static abstract T Parse(string s);

    // 静态抽象属性
    static abstract T Zero { get; }
    static abstract T One  { get; }

    // （继续可以混入实例方法）
    string ToDisplayString();
}
```

**语法规则**：
- `static abstract` 双关键字必须相邻（顺序不限，建议 `static abstract`）
- 静态抽象成员只能是：方法、运算符、属性（`{ get; }` 形式）
- 静态抽象方法/算子**没有 body**（`;` 结尾）
- 静态抽象属性**没有 getter body**（`{ get; }`）
- 一个接口可以混合 instance 方法和 static abstract 成员

### 3.2 实现者

```z42
public struct int : INumber<int> {
    public static int operator +(int a, int b) { return a + b; }
    public static int operator -(int a, int b) { return a - b; }
    // ... 5 个运算符
    public static int Parse(string s) { /* native / script */ }
    public static int Zero { get { return 0; } }
    public static int One  { get { return 1; } }
}

public struct Vec2 : INumber<Vec2> {
    int x; int y;
    Vec2(int a, int b) { this.x = a; this.y = b; }

    public static Vec2 operator +(Vec2 a, Vec2 b) { return new Vec2(a.x + b.x, a.y + b.y); }
    // ...
    public static Vec2 Zero { get { return new Vec2(0, 0); } }
    public static Vec2 One  { get { return new Vec2(1, 1); } }
}
```

**实现验证规则**：
- 对每个接口静态抽象成员，class/struct 必须提供**同名 static 成员**
- 签名比较：把接口签名里的 T 替换为**实现者自己**（self-type）后，应该与 class 的 static 成员签名相同
- 成员类别必须匹配（operator vs 普通方法 vs 属性）

### 3.3 泛型用法

```z42
T Sum<T>(T[] xs) where T : INumber<T> {
    T acc = T.Zero;                          // ← 类型级访问 (iter 2 ⏸)
    foreach (var x in xs) {
        acc = acc + x;                       // ← 运算符派发 (iter 1 ✅)
    }
    return acc;
}

void Main() {
    var total = Sum(new int[] { 1, 2, 3 });          // T=int
    var vecs  = Sum(new Vec2[] { new Vec2(1,1), new Vec2(2,3) }); // T=Vec2
}
```

### 3.4 与 existing operator 重载的关系

完全兼容 —— C# 11 的运算符声明语法和 C# 1-10 一致。用户写的 `public static Vec2 operator +(Vec2, Vec2)` **既是 C# 标准运算符重载，也满足 `INumber<Vec2>` 契约**。一处声明两用。

### 3.5 三档静态成员 + `override` 显式（方案 D1'）

**核心规则**：接口静态成员分三档，**与 class 实例方法 `abstract`/`virtual`/无修饰完全同构**。实现端 override 必须用 **`static override`** 关键字（强显式、防拼写漂移）。

#### 接口端三档

```z42
public interface INumber<T> where T : INumber<T> {
    // 档 1：抽象 — 必须 override
    static abstract T operator +(T a, T b);
    static abstract T operator *(T a, T b);

    // 档 2：虚 + 默认 body — 可选 override；若不 override 则继承接口默认
    static virtual T Double(T x) { return x + x; }
    static virtual T AddThrice(T x, T y, T z) { return x + y + z; }

    // 档 3：具体（无修饰）— 不可 override，sealed by default
    static T VersionTag() { return 1; }
}
```

| 档 | 关键字 | body | 实现端要求 |
|---|------|:---:|-------|
| 1 | `static abstract` | ❌ | **必须** `static override`，否则 E0412 |
| 2 | `static virtual` | ✅ | **可选** `static override`（不 override 则用默认）|
| 3 | `static`（无 virtual/abstract）| ✅ | **不可** override（E04xx CannotOverrideSealed）|

**与 class 实例方法规则完全同构**：

| Class 实例方法 | Interface 静态成员 |
|--------------|----------------|
| `public abstract void M();` | `static abstract T M();` |
| `public virtual void M() { ... }` | `static virtual T M() { ... }` |
| `public void M() { ... }`（无 virtual） | `static T M() { ... }`（无 virtual）|

#### 实现端：`override` 必显式

```z42
public struct MyInt : INumber<MyInt> {
    int v;
    MyInt(int n) { this.v = n; }

    // 实现抽象：必须 override
    public static override MyInt operator +(MyInt a, MyInt b) { return new MyInt(a.v + b.v); }
    public static override MyInt operator *(MyInt a, MyInt b) { return new MyInt(a.v * b.v); }

    // 覆盖虚默认：必须 override
    public static override MyInt Double(MyInt x) { return new MyInt(x.v * 2); }

    // AddThrice 不 override → 使用接口默认 x + y + z
    // VersionTag 不可 override → 若声明 `public static MyInt VersionTag()` 直接报错
}
```

**规则**：
- override 抽象 / 虚 → 必须 `static override`
- 省略 `static override` 的同名静态方法 → 编译错（提示应加 `override` 或重命名）
- 拼写拼错（方法名不存在于接口） → 编译错（没有可 override 的目标）—— **防御"静默变新方法"**

#### 运行时派发（值驱动 + 默认回退）

参数中任意 T 类型的值都带有 concrete type 信息。VM 派发算法：

```
static_call_via_iface "INumber" "Double" [x]:
  concrete = resolve_concrete_class(x)          // e.g., "MyInt"
  fn_name = "{concrete}.Double"                 // "MyInt.Double"
  if module has fn_name:
      call module[fn_name]                      // override 胜出
  else:
      fallback = "INumber.Double"
      if module has fallback:
          call module[fallback]                 // 接口默认
      else:
          error
```

示例：`T.Double(myInt)` where T=MyInt → 查 `MyInt.Double` → 找到 → 执行 override
反之：`T.AddThrice(a, b, c)` where T=MyInt（没 override）→ 查 `MyInt.AddThrice` → 未找到 → 回退 `INumber.AddThrice`（接口默认 body `a + b + c`）→ 执行，内部的 `+` 又是值驱动派发

#### Iter 1 Scope 限制

- 默认 body **只能使用 operand-input 的调用**（参数中至少有一个 T 值）
- **不能**使用无操作数的类型级访问：`T.Zero` / `T.Parse(s)` / `T.MaxValue`（iter 2+）
- ✅ 可以：`static virtual T Double(T x) { return x + x; }`
- ❌ 暂不可：`static virtual T Sum(T[] xs) { T acc = T.Zero; ... }`（用了 T.Zero）
- ❌ 暂不可：`static virtual T Zero2 { get { return T.Zero; } }`（静态属性 + 类型级访问）

#### 编译器职责

- Parser：识别三档 `static abstract` / `static virtual` / `static`；实现端识别 `static override`
- SymbolCollector：`Z42InterfaceType` 分别存三档静态成员
- TypeChecker：
  - 实现者对 abstract 必须 override；对 virtual 可选；对具体 sealed 不许 override
  - override 的签名必须匹配（T → self-type 替换后）
  - 拼写不匹配（没找到 override 目标）→ 报错
- Codegen：默认 body 编译为 `{InterfaceFullName}.{MethodName}` 独立函数
- 运行时 `static_call_via_iface`：实现者优先 → 接口默认回退

---

## 4. 语义与类型检查

### 4.1 存储结构

`Z42InterfaceType` 存储三档静态成员，用 `StaticMemberKind` 枚举区分：

```csharp
public enum StaticMemberKind { Abstract, Virtual, Concrete }

public sealed record Z42StaticMember(string Name, Z42FuncType Signature, StaticMemberKind Kind);

public sealed record Z42InterfaceType(
    string Name,
    IReadOnlyDictionary<string, Z42FuncType> Methods,      // instance abstract
    IReadOnlyList<Z42Type>? TypeArgs = null,
    IReadOnlyDictionary<string, Z42StaticMember>? StaticMembers = null
    // iter 2+: IReadOnlyDictionary<string, Z42Type>? StaticProperties,
    //         IReadOnlyDictionary<string, Z42FuncType>? InstanceDefaults  (DIMs)
);
```

- `Kind == Abstract`：无 body（实现者必须 override）
- `Kind == Virtual`：有 body（实现者可选 override；默认 body 编译成 `{InterfaceFullName}.{MethodName}` 独立函数）
- `Kind == Concrete`：有 body（实现者不可 override；可以直接 `INumber.VersionTag()` 调用）

### 4.2 实现者验证

扩展 `SymbolCollector.Classes.cs` 第四遍：

```
对 class C : INumber<C>：
  查 C 上每个 static 方法 (name, sig)：
    若 name 存在于 INumber.StaticMembers：
      要求 C 的声明带 `static override` 修饰
      不带 override → E04xx MissingOverride
      若 iface Kind == Concrete → E04xx CannotOverrideSealed
      签名必须匹配（T → C 替换后）→ 不匹配 E0412 InterfaceMismatch
    若 name 不在 INumber.StaticMembers 且 C 声明带 `override`：
      → E04xx NoOverrideTarget（拼写防护）

  查 INumber.StaticMembers 每一条 (name, sig, kind)：
    若 kind == Abstract：
      C 必须提供 `static override` → 否则 E0412
    若 kind == Virtual：
      C 可选 override（不提供则继承接口默认）
    若 kind == Concrete：
      C 不允许同名 static 方法（见上）

  查 INumber.Methods（instance）每一条：同 L3-G2 现有逻辑
```

### 4.3 运算符派发（方案 A：值驱动）

**情况 1**：非泛型、具体类型的 `a + b`
- `a.Type = Vec2`, `b.Type = Vec2`
- 查 `Vec2.StaticMethods["op_Add"]` 签名 `(Vec2, Vec2) -> Vec2`
- 生成 `BoundCall(Static, class=Vec2, method=op_Add, args=[a, b])`
- **已实现**（commit `12a3854`）

**情况 2**：泛型、T 约束 `where T : INumber<T>` 的 `a + b`
- `a.Type = Z42GenericParamType("T")`
- 查约束接口 `INumber<T>.StaticMethods["op_Add"]`
- **编译期无法确定 T**
- 生成**新的 BoundCall 种类**：`InterfaceStaticCall(interface=INumber, method=op_Add, args=[a, b])`
- IR 层：新指令 `OpInterfaceStaticCall(iface_name, method_name, args)` 或复用 VCall 但 kind=InterfaceStatic
- VM 运行时：
  - 取 `args[0]` 的 concrete type（primitive → class_name 如 `Std.Int32`；Object → type_desc.name）
  - 查该 class 的 `StaticMethods[method_name]`
  - 调用，返回结果
- **值驱动 limitation**：`args[0]` 必须是非 null 值；对 `T.Zero` 这类无参静态成员**不适用**

### 4.4 类型级访问（方案 B：类型参数驱动）— iter 2+

**情况 3**：`T.Zero` / `T.Parse(s)`
- 需要在泛型 callsite 传递 T 的 TypeDesc
- z42 已有 L3-G3a 的 constraint metadata；扩展为 runtime type_args 传递
- 本设计 iter 1 不覆盖

### 4.5 与现有 L3-G2 约束体系的关系

- `where T: I` 的 interface 约束已经存在
- 对 instance 方法派发仍然走 vtable（不变）
- 对 static 方法派发走新路径（值驱动 / 类型驱动）
- 同一约束可同时提供 instance 和 static 成员，互不冲突

---

## 5. 运行时模型

### 5.1 值驱动静态派发

新 IR 指令（或 VCall 扩展）：

```
%ret = static_call_via_iface  "INumber"  "op_Add"  %a  %b
```

VM 执行（含默认实现回退）：

```rust
Instruction::StaticCallViaIface { dst, iface_name, method, args } => {
    let receiver = frame.get(args[0])?;
    let class_name = resolve_concrete_class(receiver)?;   // primitive / Object

    // Step 1: 查实现者的静态方法
    let impl_name = format!("{}.{}", class_name, method);
    if let Some(callee) = module.lookup_function(&impl_name) {
        return exec_function(module, callee, &collected_args);
    }

    // Step 2: 回退接口默认实现（static virtual）
    let default_name = format!("{}.{}", iface_name, method);
    if let Some(callee) = module.lookup_function(&default_name) {
        return exec_function(module, callee, &collected_args);
    }

    bail!("no implementation for {}.{} on {}", iface_name, method, class_name);
}
```

### 5.2 性能考虑

- primitive 的 `a + b` 编译为 StaticCallViaIface → VM 派发到 `Std.Int32.op_Add`
- `Std.Int32.op_Add` body 是 `return a + b` → IR AddInstr
- 开销：**1 次函数调用 + 原生加法**（vs 直接 AddInstr）
- 对泛型代码可接受；未来可加 IR-level specialization（iter 3+）

### 5.3 与现有 primitive_class_name 机制复用

`src/runtime/src/interp/exec_instr.rs` 已有：

```rust
pub(crate) fn primitive_class_name(obj: &Value) -> Option<&'static str> {
    match obj {
        Value::I64(_)  => Some("Std.Int32"),
        Value::F64(_)  => Some("Std.Double"),
        ...
    }
}
```

新的 StaticCallViaIface 指令 dispatch 复用此函数 + Object.type_desc 组合。

---

## 6. 实现路线图

### 迭代 1 — operator 派发（本轮）
- **阶段 1**：Parser + AST（`static abstract` 在接口）✅ 已完成
- **阶段 2**：`Z42InterfaceType.StaticMethods` 存储 ✅ 已完成
- **阶段 3**：实现者验证（接口静态抽象成员漏缺检查）
- **阶段 4**：stdlib INumber 迁移到 static abstract
- **阶段 5**：primitive struct (int/long/double/float) 改为提供 static operator
- **阶段 6**：generic `a + b` 派发（TypeChecker 生成 InterfaceStaticCall）
- **阶段 7**：新 IR 指令 + VM 执行
- **阶段 8**：TSIG 跨 zpkg 导出/导入 StaticMethods
- **阶段 9**：测试 + 文档 + 归档

### 迭代 2 — 类型级访问（未来）
- `T.Zero` / `T.Parse(s)` 表达式支持
- runtime type_args 传递（L3-R 子集）
- static abstract **属性**（iter 1 仅支持方法 / operator；属性延后）

### 迭代 3 — 性能优化（未来）
- Primitive 泛型 `a + b` IR-level 特化（StaticCallViaIface → AddInstr）
- 避免运行时 dispatch 开销

---

## 7. 迁移影响

### 7.1 需要重写的 stdlib

**当前** (`z42.core/src/INumber.z42`):
```z42
public interface INumber<T> {
    T op_Add(T other);         // 实例方法
    // ...
}
```

**迁移后**:
```z42
public interface INumber<T> where T : INumber<T> {
    static abstract T operator +(T a, T b);
    // ...
}
```

**当前** (`z42.core/src/Int.z42`):
```z42
public struct int : ..., INumber<int> {
    public int op_Add(int other) { return this + other; }  // 实例
}
```

**迁移后**:
```z42
public struct int : ..., INumber<int> {
    public static int operator +(int a, int b) { return a + b; }
}
```

### 7.2 对 commit `12a3854` operator 重载的影响 ✅

`12a3854` 的 TypeChecker desugar 逻辑分两支：
1. 静态 operator → BoundCall(Static) —— **保留**
2. 实例 op_Add（泛型 / 类直接实现）→ BoundCall(Virtual) —— **已改写**

改写后（实施结果）：
- 泛型 T 约束 → `TryLookupInstanceOperator` 的 Z42GenericParamType 分支
  查 `iface.StaticMembers`（2 参静态）→ BoundCall(Virtual) → VCallInstr。
  **未引入新 IR 指令**：VCall 值驱动派发天然匹配 `(receiver, ...extras)` 调用
  约定与 `(T, T) -> T` 静态签名。
- 用户类直接 `a + b`（Vec2 等）→ 仍走静态 operator BoundCall
- 用户类实例方法形式 `left.op_Add(R)`（非 INumber）→ 仍经
  `TryLookupInstanceOperator` 的 Z42ClassType 分支 → BoundCall(Virtual)
- **已移除**（2026-04-24 cleanup）：泛型 T 约束的 1 参实例方法查询
  （Path A on Z42GenericParamType）。stdlib INumber 全静态后此路径永远不触发。

### 7.3 golden test 影响

- `generic_inumber`：依赖原 INumber 实例方法，需改写或标记废弃
- `operator_overload`：静态 operator，**不受影响**

---

## 8. 诊断与错误

| 错误码 | 触发 |
|-------|------|
| E0412 InterfaceMismatch | class 未提供接口的 static abstract 成员 |
| E0412 InterfaceMismatch | static abstract 方法签名不匹配 |
| E0412 InterfaceMismatch | class 提供了 instance 方法但接口要求 static（或反之）|
| E0201 ExpectedToken | `static abstract` 后缺少方法签名 |
| E0413 InvalidImpl | 接口体内静态抽象成员格式错误 |

---

## 9. 明确的 Scope

### iter 1 **包含**：
- 接口三档静态成员：`static abstract` / `static virtual` / `static`（无修饰）
- 实现端 `static override` 关键字（必显式）
- 5 个二元算术运算符（`+ - * / %`）的静态抽象 / 虚 / 具体形式
- 非运算符的静态抽象 / 虚 / 具体方法
- 实现者签名验证 + 拼写防护（override 目标必须存在）
- 泛型 `a + b` on `T: INumber<T>` 派发（InterfaceStaticCall IR + VM）
- 默认实现回退：VM 找不到 concrete class 的静态方法时回退接口 virtual 默认
- stdlib INumber / primitive struct 迁移
- TSIG 跨 zpkg（含默认实现元数据）

### iter 2（记录，不做）：
| 特性 | 说明 | 依赖 |
|------|------|------|
| **静态抽象 / 虚属性** | `T Zero { get; }` 在接口声明 | 需要 property AST + 静态属性 IR |
| **类型级访问** | `T.Zero` / `T.Parse(s)` / `T.MaxValue` 在泛型体内 | 需要 runtime type_args 传递（L3-R 子集） |
| **`sealed override`** | 实现者可 `public static sealed override ...` 终止进一步 override 链 | class 继承链 > 2 层 |
| **`new` 修饰符** | 实现者可 `public static new T M()` 声明同名但不参与 override | 需明确"隐藏 vs 覆盖"语义 |
| **接口继承** | `interface INumber<T> : IArithmetic<T>` | 接口多继承 + 静态成员传递 |
| **默认 body 中的类型级访问** | `static virtual T Sum(T[] xs) { T acc = T.Zero; ... }` | 依赖"类型级访问" |
| **静态抽象字段** | C# 不支持；z42 亦不 | — |
| **静态抽象构造器** | C# 不支持；z42 亦不 | — |

### iter 3+（更远）：
| 特性 | 说明 |
|------|------|
| **实例 DIMs（C# 8 风格）** | `interface I { void M() { default; } }` 实例方法默认实现 |
| **一元运算符重载** | `operator -x` / `!x` / `~x` |
| **比较 / 相等运算符** | `<` `<=` `>` `>=` `==` `!=`（走 IComparable / IEquatable）|
| **复合赋值运算符** | `+=` / `-=` 等（纯语法糖） |
| **自定义运算符** | 用户定义运算符符号（Scala / Haskell 风格），z42 **明确不做** |
| **Primitive 泛型 IR 特化** | `a + b` on T=int 编译期降为 AddInstr（当前走函数调用）|
| **关联类型** | `type Output;` + `where T: Add<Output = U>` 依赖 L3-G3c |

---

## 10. 关键设计权衡记录

### 为什么值驱动派发而非类型参数驱动？
- z42 的代码共享哲学：一份字节码服务所有 T
- 运算符场景下 receiver (args[0]) 始终可用 → 值驱动足够
- 类型参数传递需要 L3-R 完整支持，规模大 → 延后

### 为什么 iter 1 支持 `static virtual` 默认实现？
- 让接口"契约小而能力强"：实现者只提供原语，复合操作接口提供
- C# 11 有这个特性，与目标对齐
- 实现代价：只需在 VM 派发时多一步"回退接口默认"查询
- 限制明确：默认 body 只能用 operand-input 调用，不能用 T.Zero（留 iter 2）

### 为什么 override 必须显式（`static override`）？
- **对齐 z42 class 继承模型**：class 实例方法 override 已要求 `override` 关键字
- 一条规则覆盖实例和静态场景，无双标
- **防拼写漂移**：拼错方法名时 → 没有可 override 目标 → 编译错（防"静默变新方法"）
- **意图显式**：读代码者无需跳到接口看契约，立刻知道是 override

### 为什么三档不需要 `sealed` 关键字？
- C# 里 `sealed` 只与 `override` 组合（`sealed override`），表示"终止进一步 override 链"
- 无修饰的普通方法**本来就不可 override**（不在 override 链里）
- 所以档 3（无修饰 + body）=> 默认 sealed，无需显式
- 完全对齐 class 实例方法默认规则

### 为什么暂不支持实例 DIMs（C# 8 风格）？
- 钻石问题 / 可见性规则复杂
- z42 当前 interface 只有 instance 抽象方法，加 DIMs 影响面大
- 独立特性，未来单独迭代（iter 3+）

### 为什么暂不支持 `sealed override` / `new`？
- class 继承链 > 2 层的 override 情况 z42 目前极少
- 静态方法不形成传统的继承链（不同 class 的静态方法命名空间独立）
- 无真实用例触发 → 延后到 iter 2+
- **前兆**：一旦 class 继承深度增加 或 用户出现"隐藏 vs 覆盖"的真实场景，立刻加

### 为什么保留 `static abstract` 双关键字？
- 与 C# 11 一致
- 显式表达"这是契约"意图
- 未来加 DIM 时不需要新关键字

### 为什么不做关联类型（type Output）？
- C# 11 也没有
- 实现复杂（约束求解 + 统一）
- 对 operator / INumber 不是必需（同质算子够用）
- 延后到 L3-G3c

### 为什么要求 `where T : INumber<T>` 的 CRTP 约束？
- 让 `static abstract T operator +(T, T)` 里的 T 准确指向实现者自身
- 防止 `class Foo : INumber<Bar>` 这种错配
- 与 C# 11 一致

---

## 11. 相关文档

- `docs/book/src/language/generics.md` — L3-G 泛型总体设计
- `docs/design/philosophy.md` §8 — Simplicity First + Script-First
- `docs/spec/archive/2026-04-23-add-inumber-script-first/` — INumber iter 1（将被本设计替代）
- `docs/spec/archive/2026-04-24-add-operator-overload-csharp/` — operator 重载语法（本设计扩展）
