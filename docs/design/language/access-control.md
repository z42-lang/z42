# 访问权限控制规范

> **Status**: ✅ 成员级全部实现（enforce-access-control #180 + default-member-private #181）+ **类级强制**
> （enforce-class-access ① + enforce-crosspkg-internal-class ②）——private / protected / internal（成员含跨包）+
> 默认成员 private / 顶层 internal + 组合修饰符拒绝（E0405）+ override 继承基类可见性 + record 定位字段 public +
> **类级：private/protected 嵌套类 + 跨包 internal 类的**类型引用**校验**（覆盖体引用 new/var/cast/is/as/typeof/catch
> + 声明签名 字段/参/返/基类·接口；跨包 internal 类可见性经 zbc 1.33/zpkg 0.38 TYPE 记录可见性字节序列化）。
> 机制页见 [`docs/book/src/compiler/access-control.md`](../../book/src/compiler/access-control.md)。
> **本规范是访问控制的语言 SoT**（2026-08-12：默认成员 = private 以本文档为准，实现已对齐）。
>
> 剩余 Deferred（独立后续）：不一致可访问性（public 签名暴露 internal 类型，C# CS0050–53）；顶层类标
> private/protected 的声明期拒绝；接口类型可见性（`Z42InterfaceType` 未建模可见性）；类可见性反射面
> （VM read-and-discard，`Type.IsPublic` 等未接）。嵌套类 `LinkedList.Node` 例现已强制（外部引用 → E0404）。

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
| `private` | 仅当前类内部 |
| `protected` | 当前类 + 所有直接/间接子类 |
| `internal` | 同一程序集（模块）内 |
| `public` | 所有人，无限制 |

---

## 默认可见性规则

| 声明位置 | 默认可见性 | 封闭层 |
|---------|-----------|-------|
| 顶层类 / 接口 / 顶层函数 | `internal` | 模块 |
| 类的字段 / 方法 / 构造器 | `private` | 类 |
| 嵌套类 | `private` | 外部类 |
| 枚举成员 | 跟随枚举本身 | 不可单独指定 |

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

```z42
public enum Status {
    public Active,    // ❌ 编译错误：枚举成员不能有访问修饰符
    Inactive          // ✅ 跟随枚举，自动 public
}
```

---

## 不支持的组合写法

```z42
protected internal void Foo() { }   // ❌ 不允许组合修饰符
private protected int x;            // ❌ 不允许组合修饰符
```

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
    var n = new LinkedList.Node();  // ❌ Node 是 private
    var it = new LinkedList.Iterator();  // ✅ public
}
```

---

## Phase 1 实现范围

| 检查项 | Phase 1 | Phase 2 |
|--------|---------|---------|
| Parser 解析四个修饰符 | ✅ | — |
| AST 节点携带 Visibility 字段 | ✅ | — |
| `private` 成员越界访问 → 编译错误 | ✅ | — |
| 枚举成员带修饰符 → 编译错误 | ✅ | — |
| 组合修饰符 → 编译错误 | ✅ | — |
| `protected` 子类访问检查 | ✅ | — |
| `internal` 跨模块检查（含跨包） | ✅ | — |

### Phase 1 测试用例覆盖

**合法用例：**
- 类的 public 方法从外部调用 ✅
- 类的 private 字段通过 public getter 访问 ✅
- 同类内访问 private 成员 ✅
- 枚举成员无修饰符 ✅

**非法用例（必须报编译错误）：**
- 从类外访问 private 字段 → `error: field 'x' is private`
- 从类外调用 private 方法 → `error: method 'Foo' is private`
- 枚举成员带修饰符 → `error: enum member cannot have access modifier`
- 组合修饰符 → `error: cannot combine access modifiers`
