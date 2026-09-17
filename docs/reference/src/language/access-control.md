# 访问权限控制

> 对齐：2026-09-15

z42 的访问修饰符（`public` / `private` / `protected` / `internal`）语义与 C# 一致，并在**编译期强制**：
违规的成员访问或类型引用报 **E0404**。

## 设计原则

**一条统一规则：默认可见性 = 最小封闭作用域。**

> 没有修饰符的声明，只对直接包含它的那层结构可见。

- `namespace` 纯粹用于类型隔离和组织，不参与访问控制，不绑定物理路径。
- `using` 只做名称解析（把类型名带入作用域），不授予任何访问权限。
- 修饰符只能写一个，**不允许组合**（无 `protected internal`、`private protected` 等）。
- 无 `friend` 关键字；需要模块内精细协作，通过拆分模块或 Capability Token 模式解决。

---

## 访问修饰符

从窄到宽共四级：

| 修饰符 | 可见范围 |
|--------|---------|
| `private` | 仅当前类内部（含同类其它实例；**派生类不可**访问基类 private） |
| `protected` | 当前类 + 所有直接/间接子类（跨包派生同样允许） |
| `internal` | 同一程序集（模块 / zpkg）内 |
| `public` | 所有人，无限制 |

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

## 顶层声明不接受 private / protected

顶层 `class` / `interface` / `struct` / `record` / `enum` / 函数标 `private` / `protected` 在模块作用域
无意义（默认已是 internal），**声明期即拒绝** → **E0442**：

```z42
private class Foo { }       // ❌ E0442
protected void bar() { }    // ❌ E0442
internal class Ok { }       // ✅
public class Ok2 { }        // ✅
```

嵌套类型不走此路径，仍可 `private` / `protected`。

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

---

## 相关文档

- [属性与索引器](properties-indexers.md)——属性 / 索引器的默认可见性与访问器修饰符
- [静态成员](static-members.md) / [构造函数](constructors.md)
- 实现机制（强制点、`CheckAccess` 判据、跨包 internal 的元数据链路）：
  [访问权限强制](../../../internals/src/compiler/access-control.md)
- [错误码体系](../../../internals/src/compiler/error-codes.md)
