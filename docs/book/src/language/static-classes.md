# static 类

> 对齐：2026-09-01（change `fix-static-class-instance-members`）

`static class` 声明一个**只容纳静态成员**的类——它不能被实例化、不持有实例状态。语义与 C# 一致，
用途是把一组无状态的相关函数 / 常量组织到一个命名容器里（stdlib 的 `Std.Math`、`Std.Convert`、
`Std.Path`，以及 z42c 内部「无 enum → static class + int 常量」的 `TokenKind` / `SyntaxKind` 皆如此）。

## 语法

```z42
public static class Math {
    public static double Pi = 3.141592653589793;   // ✓ 静态字段
    public static double Abs(double x) {            // ✓ 静态方法
        return x < 0.0 ? -x : x;
    }
}
```

## 约束（E0451）

static 类**只能**含静态成员。以下写法皆报 **E0451**（对标 C# 的 CS0708 / CS0710 / CS0713 / CS0714）：

| 违规 | 例 | 说明 |
|------|----|----|
| 实例方法 | `static class S { public int M() {...} }` | 无实例 → 无 `this`，实例方法无意义 |
| 实例字段 | `static class S { public int X; }` | static 类不持有实例状态（`static` / `const` 字段合法） |
| 实例属性 | `static class S { public int P { get; } }` | 同上 |
| 实例构造器 | `static class S { public S() {} }` | static 类不可实例化 |
| 索引器 | `static class S { public int this[int i] {...} }` | 索引器天然是实例成员 |
| 声明基类 | `static class S : Base {}` | static 类不参与继承层次 |
| 实现接口 | `static class S : IFoo {}` | 接口是实例契约，static 类无实例可满足 |

```z42
static class Bad {
    public int Count;                 // ✗ E0451: static class `Bad` cannot declare instance field `Count`
    public int Next() { return 0; }   // ✗ E0451: static class `Bad` cannot declare instance method `Next`
}

static class Also : IComparable<int> {   // ✗ E0451: static class `Also` cannot implement interfaces
}
```

**合法成员**：`static` 方法 / 字段、`const` 字段（隐含静态）、嵌套类型（`class` / `struct` / `enum` /
`interface`——它们本身不是实例成员）。

## 实现原理

强制发生在 **符号收集** 阶段（`z42c.semantics` 的 `SymbolCollector._passSealedEnforce`——与 sealed
强制同遍遍历类声明 + 成员，见 [`compiler/source-compile.md`](../compiler/source-compile.md)）：

1. 判定 `c.Kind == "class"` 且 mods 含 `static` → 该类受约束；
2. 类级：`HasBase` 或 `InterfaceCount > 0` 各报一条 E0451；
3. 成员级：逐个成员，凡实例方法 / 字段（非 `static`·非 `const`）/ 属性 / 构造器 / 索引器各报一条。

> **历史坑（本 change 的动机）**：`static` 修饰在早期 z42c 里被 `StubCollector` **完全忽略**（只读
> `sealed` / `abstract` / `struct`），所以「`static class` + 实例成员」这种矛盾声明能静默通过。
> 标准库的 `Std.String`（primitive `string` 的包装类，`s.Length` / `s.Contains(...)` 皆为实例调用、
> 且实现 `IComparable` / `IEquatable`）就曾被误标 `static class`——它本该是 `sealed class`（对齐 C#
> `System.String`）。本 change 同时修正了 `Std.String` 并补上 E0451 强制，杜绝此类矛盾再次出现。

## 与 sealed 的区别

- `sealed class`：可实例化、可有实例成员，只是**不可被继承**（见 [sealed 修饰符](sealed.md)）。
- `static class`：**不可实例化**、只容纳静态成员。二者正交但不叠加使用（static 类天然不参与继承）。
