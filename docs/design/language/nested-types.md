# 嵌套类型（Nested Types）

> 状态：v1 已落地（add-nested-types，2026-07-25）。反射侧见 [reflection.md](reflection.md)
> `reflection-future-nested-types`。

一个类型可以**声明在另一个类型体内**，作为其成员：

```z42
class Outer {
    public int X;

    public class Inner {           // 嵌套 class
        public int Y;
        public int twice() { return this.Y * 2; }
        public class Deep { }      // 任意深度
    }

    public struct Point { public int px; public int py; }   // 嵌套 struct
    public enum   Color { Red = 1, Green = 2 }              // 嵌套 enum
    public interface IShow { int show(); }                  // 嵌套 interface
}
```

嵌套类型是**独立类型**（不是外层的实例成员）：不持有外层的 `this`，用**限定名**从外部引用。

```z42
Outer.Inner ni = new Outer.Inner();      // 源码用 `.` 限定
ni.Y = 21;
int r = ni.twice();                       // 42

Outer.Inner.Deep d = new Outer.Inner.Deep();   // 多层
Outer.Point p = new Outer.Point();             // 嵌套 struct
```

## 命名：源码 `.` vs 元数据 `+`

| 面 | 写法 | 例 |
|----|------|----|
| 源码限定名 | `.` | `Outer.Inner`、`Outer.Inner.Deep` |
| `Type.Name`（简单名） | —— | `Inner` |
| `Type.FullName`（FQ 元数据名） | `+` 分隔嵌套、`.` 分隔 namespace | `Ns.Outer+Inner` |

采 C# 约定：**namespace 用 `.`、嵌套用 `+`**，二者不混。这样反射的嵌套关系可以**纯从名字派生**
（找 `+`），无需在类型元数据里加字段——因此**没有 zbc/zpkg 格式 bump**（与数组 `[]` 后缀、
构造泛型 `<>` 串同一设计路数）。

## 反射

```z42
Type ti = typeof(Outer.Inner);
ti.Name;               // "Inner"
ti.FullName;           // "Ns.Outer+Inner"
ti.IsNested;           // true
ti.GetDeclaringType(); // typeof(Outer)（顶层类型 → null）

typeof(Outer).GetNestedTypes();   // [Color, IShow, Inner, Point]（直接子嵌套，有序；不含更深/继承）
typeof(Outer).GetMembers();       // 含嵌套类型（MemberTypes.NestedType），与字段/方法/属性并列
```

细节与实现原理见 [reflection.md](reflection.md)。

## 实现原理（简）

1. **parser**：类成员位置遇 `class/struct/interface/enum/record` 关键字 → 按类型声明解析（其成员体
   递归解析成员 → 深层嵌套天然支持）。此前这些关键字在成员位置被误解析为属性。
2. **展平（NestedFlatten）**：一个语义前置 pass 把嵌套类型**提升为顶层声明**、名改 `Outer+Inner`
   （任意深度 `A+B+C`），之后符号收集 / IR 发射的各 pass 把它们当**普通顶层类型**处理——注册、
   名解析、TYPE/SIGS/FUNC 发射全部复用既有机制。幂等（每个编译单元只展平一次）。
3. **名解析**：类型位置的点串 `Outer.Inner` → 转 `+` 键（`Outer+Inner`）查表；namespace-qualified
   名（`Std.Console` → `Std+Console` 未注册）自然跳过。
4. **runtime 反射**：`GetNestedTypes` 扫已加载类型取 `<this>+<simple>`（直接子）；`GetDeclaringType`
   / `IsNested` 从 `+` 派生；`Type.Name` 的简单名同时按 `.` 和 `+` 取末段。

> 因 z42c / stdlib 源码**不使用**嵌套类型，NestedFlatten 对它们零改动 → **自举字节不动点零扰动**。

## v1 范围与延后

**支持**：嵌套 class / struct / interface / enum，任意深度；限定名引用；实例化 / 字段 / 实例方法 /
`typeof` / 反射。

**延后（Deferred，见 reflection.md `reflection-future-nested-types`）**：
- **泛型外层** `Outer<T>.Inner`：parser 类型位置尚不接受 `Generic<Args>.Nested` 语法；内层引用外层 `T`
  的嵌套类型需 generic instantiation 做 `T` 替换（0.5.x L3-R）。
- 嵌套类型的 base 为**另一嵌套类型**（`class Inner : Outer.Other`）。
- 跨包**限定名**引用嵌套（`geo.Shape.Corner`）——当前解析包内 `Outer.Inner`。
- 嵌套 `partial`（E0435 保留，design D9）。
