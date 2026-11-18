# 访问权限控制

z42 的访问修饰符（`public` / `private` / `protected` / `internal`）在**成员位**语义与 C# 一致；
在**顶层位**按 z42 自己的作用域轴定义（文件 / 命名空间），并在**编译期强制**：
违规的成员访问或类型引用报 **E0404**。

## 设计原则

**一条统一规则：默认可见性 = 最小封闭作用域。**

> 没有修饰符的声明，只对直接包含它的那层结构可见。

- 顶层声明的封闭层就是**文件**：`private` = 本文件（REPL 会话内各轮共享同一文件作用域）。
- `namespace` 是 `protected` 的可见性边界（本包 ∩ 同名命名空间）；此外仍只用于类型隔离和组织，
  不绑定物理路径。
- `using` 只做名称解析（把类型名带入作用域），不授予任何访问权限。
- 修饰符只能写一个，**不允许组合**（无 `protected internal`、`private protected` 等）。
- 无 `friend` 关键字；需要模块内精细协作，通过拆分模块或 Capability Token 模式解决。

---

## 访问修饰符

从窄到宽共四级——顶层位恰好构成良序包含链 **文件 ⊂ 命名空间 ⊂ 模块 ⊂ 全部**：

| 修饰符 | 成员位（C# 一致） | 顶层位（z42 作用域轴） |
|--------|------------------|----------------------|
| `private` | 仅当前类内部（含同类其它实例，以及嵌套在它里面的类型；**派生类不可**访问基类 private） | **本文件**（声明所在的编译单元；REPL 会话各轮共享） |
| `protected` | 当前类 + 所有直接/间接子类（跨包派生同样允许） | **本包 ∩ 同名命名空间**（要求文件显式声明 `namespace`，否则 E0442） |
| `internal` | 同一程序集（模块 / zpkg）内 | 同左（顶层默认） |
| `public` | 所有人，无限制 | 同左 |

---

## 默认可见性规则

| 声明位置 | 默认可见性 | 封闭层 |
|---------|-----------|-------|
| 顶层类 / 接口 / 结构 / 记录 / 枚举 / 顶层函数 | `internal` | 模块 |
| 类的字段 / 方法 / 构造器 | `private` | 类 |
| 类的**属性 / 索引器** | `private` | 类 |
| 嵌套类 | `private` | 外部类 |
| 枚举成员 | 跟随枚举本身 | 不可单独指定 |

### 三处「不写修饰符也是 public」的特例

这三处若按默认规则判成 `private`，会让正常代码全线不可用，故按 C# 语义特判：

| 形态 | 实际可见性 | 原因 |
|------|-----------|------|
| 无显式修饰符的 `override` 方法 | `public` | 只能覆写 virtual/abstract 契约，通常 public；否则 `override ToString()` 一类跨类调用全断 |
| `record R(string A, …)` 的定位字段 | `public` | 镜像 C# record 定位参 → 公有属性 |
| 主构造器 `class P(int X)` / `[Record] struct R(…)` | `public` | 镜像 C#；否则类外无法 `new P(1)` |

⚠️ **普通构造器不写修饰符仍是 `private`**——只有 parser 合成的**主构造器**是 public：

```z42
class Point {
    public int X; public int Y;
    Point(int x, int y) { this.X = x; this.Y = y; }   // 无修饰符 → private
}
void Main() {
    var p = new Point(1, 2);        // ❌ E0404：构造器是 private
}

class Vec(int X, int Y) { }         // 主构造器 → public
var v = new Vec(1, 2);              // ✅
```

连带后果：元组 `(a, b)` 脱糖成 `[Record] struct ValueTuple2<…>(…)`，其主构造器是 public，
故元组在类外可构造；若主构造器改回 private，元组将在类外无法构造。

### 示例

```z42
// 顶层：默认 internal
class Engine { ... }            // internal
void helper() { ... }           // internal

public class PublicApi { ... }  // 显式 public，对外暴露

class Renderer {
    int width;                  // private（默认）
    string name;                // private（默认）

    public Renderer(int w) { this.width = w; }   // 显式 public
    public void Render() { ... }                  // 显式 public
    internal void Reset() { ... }                 // 显式 internal
    // width/name 只能通过 public 方法间接访问
}

// 枚举：成员跟随枚举，不可单独指定修饰符
internal enum Direction { North, South, East, West }   // 成员全部 internal
public enum Color { Red, Green, Blue }                  // 成员全部 public
```

---

## 规则详细说明

### private

```z42
class Counter {
    int count;                    // private

    public void Increment() { this.count++; }   // 可以，类内访问
    public int Get() { return this.count; }
}

void Main() {
    var c = new Counter();
    c.Increment();                // ✅ public 方法
    int x = c.count;              // ❌ 编译错误：count 是 private
}
```

### protected

```z42
class Animal {
    protected string name;

    protected Animal(string n) { this.name = n; }
    protected void Breathe() { ... }
}

class Dog : Animal {
    public Dog(string n) : base(n) { }
    public void Bark() {
        Console.WriteLine(this.name);   // ✅ 子类可访问 protected
        this.Breathe();                 // ✅
    }
}

void Main() {
    var d = new Dog("Rex");
    string n = d.name;            // ❌ 编译错误：name 是 protected，不是子类上下文
}
```

### internal

```z42
// 模块 A 内
class InternalHelper {
    internal void Help() { ... }
}
class ServiceA {
    void Foo() { new InternalHelper().Help(); }  // ✅ 同模块
}

// 模块 B 内
class ServiceB {
    void Bar() { new InternalHelper().Help(); }  // ❌ 跨模块，internal 不可见
}
```

### public

```z42
public class Logger {
    public void Log(string msg) { Console.WriteLine(msg); }
}
// 任何模块都可以使用
```

---

## 枚举成员不可单独指定修饰符

枚举成员位置**不接受**访问修饰符——修饰符会被当成成员名，报 **E0202**（expected enum member name）：

```z42
public enum Status {
    public Active,    // ❌ E0202：`public` 被当作成员名解析
    Inactive          // ✅ 跟随枚举，自动 public
}
```

---

## 不支持的组合写法

写两个及以上访问修饰符 → **E0405**：

```z42
protected internal void Foo() { }   // ❌ E0405：不允许组合修饰符
private protected int x;            // ❌ E0405
```

---

## 顶层作用域（access-top-scope）

顶层 `class` / `struct` / `interface` 支持 `private` / `protected`，语义按声明位置的**封闭作用域**定义：

- **`private class C { … }` = 文件作用域**：仅声明所在文件可引用（本命场景：单文件辅助类型——
  builder / 状态机 / 生成器产物；也是 C# 11 `file` 修饰符的角色，z42 不引入新关键字，复用
  「private = 最小封闭作用域」的统一语义，顶层的最小封闭层就是文件）。
- **`protected class C { … }` = 命名空间作用域**：**本包内声明了相同命名空间的文件**可访问
  （本命场景：partial 类跨文件碎片的共享 helper——各碎片 FQN 相同、必然同 ns）。判定是**本包 ∩
  同名精确匹配**：跨包一律拒绝（即使依赖包有同名 ns）；子 ns（`App.Big.Sub`）**不**自动看到
  `App.Big` 的 protected——与 internal 跨包「解析得到但拒绝」同形态，将来放宽为前缀域是非破坏升级。

```z42
// ── 文件 a.z42 ──
namespace App;
private class FileHelper { }        // 仅 a.z42 可用
protected class FamilyHelper { }    // App 内所有文件可用（含 partial 碎片所在文件）

// ── 文件 b.z42（namespace App;）──
var a = new FileHelper();           // ❌ E0404：file-scoped private，声明于另一文件
var b = new FamilyHelper();         // ✅ 同包同 ns

// ── 文件 c.z42（namespace Other;）──
var c = new App.FamilyHelper();     // ❌ E0404：namespace-scoped protected，当前 ns 是 Other
```

**顶层 `protected` 要求文件显式声明 `namespace`**（空 ns 恰是「一切 ns 的全局域」，等价 internal，
会静默骗过收窄意图）→ **E0442**：

```z42
protected class Foo { }             // ❌ E0442：本文件没有 namespace 声明
namespace App;
protected class Ok { }              // ✅（namespace 必须在文件顶部，见 namespaces）
```

配套规则：

- **E0441 边界不变**：file-private / ns-protected 类型出现在**任何越出其作用域的签名**（含同文件
  internal 类的 public 成员）→ E0441——模块元数据不得暴露别的文件不可名状的类型。同文件**体内**
  引用（局部 / `var` / `new` / 私有字段）不受限。
- **祖先链**：file-private 类的嵌套类型（哪怕自身 `public`）跨文件引用同样被拒——引用嵌套类型即引用
  其家族容器。
- **同名互斥**：两个文件各写一个同 ns 同名 file-private 类 → 既有 **E0458** 重复声明兜底。
- **导出**：file-private 类型不进 zpkg 导出元数据；protected 照常导出、跨包在引用点拒绝（与 internal
  同路径）。
- **REPL**：各轮是独立「文件」（`repl_r0`、`repl_r1`…），但**会话内共享同一文件作用域**——第 2 轮可见
  第 1 轮的 private 类；顶层 protected 在 REPL 无 ns，按 E0442 拒绝。
- **`enum` / `delegate` 与类一致**：顶层 `private enum` / `private delegate` = 文件作用域、
  `protected` = 本包 ∩ 同名命名空间（protected 同样要求 ns，否则 E0442）；嵌套 enum 显式
  `private` / `protected` 按嵌套类同语义强制。**顶层函数**仍不接受 `private` / `protected` →
  **E0442**（函数无引用点强制管线，不提供不设防的修饰符）。

---

## 嵌套类

嵌套类默认 `private`，仅外部类可见；可显式提升：

```z42
class LinkedList {
    class Node {                    // private，仅 LinkedList 可使用
        int value;
        Node next;
    }

    internal class Stats { ... }    // internal，同模块可见
    public class Iterator { ... }   // public，所有人可见

    Node head;

    public void Append(int v) {
        var n = new Node();         // ✅ 同类内可访问 private 嵌套类
        n.value = v;                // ✅ 同类内可访问 Node 的 private 字段
    }
}

void Main() {
    var n = new LinkedList.Node();  // ❌ E0404：Node 是 private
    var it = new LinkedList.Iterator();  // ✅ public
}
```

类型可见性在**每一处「命名该类型」的地方**都检查（镜像 C#）：`new T` / 局部 `T x` / `(T)e` /
`e is T` / `e as T` / `typeof(T)` / `default(T)` / `catch(T)`、泛型实参，以及声明签名（字段 /
属性 / 索引器类型 / 方法参与返回 / 基类·接口列表）。

---

嵌套类型可以访问外层类型的**全部**成员（含 `private` / `protected`），写法不限：`Outer.Secret`、外层实例上的
`o.v`，或在嵌套类型体内直接写外层的静态成员名 `Secret`。反过来不成立：外层类型访问不了嵌套类型的 `private` 成员。

## 接口类型可见性

接口与类对称携带可见性。跨包引用另一个包里的 `internal` 接口 → **E0404**。

---

## 不一致可访问性（E0441）

成员 / 类型签名不得暴露比它自己**更不可见**的类型（对标 C# CS0050 族）→ **E0441**。

判据用的是**有效可访问性**：

> 有效可访问性 = `min(成员声明的可见性, 外层类的可见性)`

可见性 rank：`public > internal > protected > private`。被暴露类型的可见性 rank 必须 ≥ 暴露它的
声明的有效可访问性。

```z42
internal class Hidden { }

public class Api {
    public Hidden Get() { ... }     // ❌ E0441：public 成员暴露 internal 类型
}

internal class Helper {
    public Hidden Get() { ... }     // ✅ 外层类是 internal ⇒ 有效可访问性 = internal，不算泄漏
}
```

检查递归穿透泛型实参与数组元素类型；非类 / 非接口类型（基元、泛型形参、函数类型）视作 public，
不触发。

---

## 反射：类可见性

`z42.core` 的 `Type` 暴露声明可见性，与 C# `System.Type` 逐一对齐：

```z42
public enum TypeVisibility { Public, Private, Protected, Internal }

public extern TypeVisibility Visibility { get; }   // 声明的四档可见性
public extern bool IsNested { get; }               // 顶层 vs 嵌套（正交的另一轴）
```

在这两者之上是 C# 那套 6 个 bool（顶层 vs 嵌套 × 四档）：

| 属性 | 等价于 |
|------|--------|
| `Type.IsPublic` | `Visibility == Public && !IsNested` |
| `Type.IsNotPublic` | `Visibility != Public && !IsNested` |
| `Type.IsNestedPublic` | `Visibility == Public && IsNested` |
| `Type.IsNestedPrivate` | `Visibility == Private && IsNested` |
| `Type.IsNestedFamily` | `Visibility == Protected && IsNested`（C# 的 "family"） |
| `Type.IsNestedAssembly` | `Visibility == Internal && IsNested`（C# 的 "assembly"） |

无类型句柄的基元与数组 → `Visibility = Public`、`IsNested = false` ⇒ `IsPublic == true`（与 C# 一致）。

> 注意：**无修饰符成员**的可见性字节是 `internal`（不是 public），故对它反射 `IsPublic` 返回
> `false`——与 C# 语义一致。

access-top-scope 之后，`Visibility == Protected && !IsNested`（顶层 protected）与
`Visibility == Private && !IsNested`（顶层 private，文件作用域）成为可达组合，`IsNotPublic` 均为
`true`——反射只报告**声明档位**，不区分文件 / 命名空间作用域的边界细节（那是编译期判定）。

---

## 相关文档

- [属性与索引器](properties-indexers.md)——属性 / 索引器的默认可见性与访问器修饰符
- [静态成员](static-members.md) / [构造函数](constructors.md)
- 实现机制（强制点、`CheckAccess` 判据、跨包 internal 的元数据链路）：
- [错误码全量表](../appendix/error-codes.md)
