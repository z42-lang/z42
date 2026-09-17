# 嵌套类型

> 对齐：2026-07-25（change `add-nested-types` + `nested-types-followup`）

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

采 C# 约定：**namespace 用 `.`、嵌套用 `+`**，二者不混。于是反射的嵌套关系可以纯从名字派生
（找 `+`），类型元数据里不需要额外字段。

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

细节见标准库的反射 API。

## 支持范围

**支持**：嵌套 class / struct / interface / enum，任意深度；限定名引用；实例化 / 字段 /
实例方法 / `typeof` / 反射。嵌套类型的基类或接口可以是**另一个嵌套类型**
（`class Inner : Outer.Other`，兄弟裸名与限定名均可）——继承字段、虚派发、上转型、
`GetInterfaces` 全通。

**尚不支持**：

- **泛型外层的嵌套类型** `Outer<T>.Inner`：类型位置还不接受 `Generic<Args>.Nested` 这种写法。

  > ⚠️ 别与**嵌套的泛型实参**混淆——那是另一件事，**已支持**：`Box<Pair<int,string>>` 这类
  > 类型实参里再套泛型，可递归到任意深度，`GetGenericArguments()` 也逐层还原
  > （见 `src/tests/types/nested_generic_args.z42`，2026-07-23）。不支持的是**外层**本身带型参。

- 跨包用**限定名**引用嵌套类型（`geo.Shape.Corner`）；当前解析包内的 `Outer.Inner`。
- 嵌套类型自身标 `partial` —— 报 **E0435**（发射点
  `src/compiler/z42c.semantics/src/DeclEnforcer.z42:160`，`_checkNestedPartial`）。

## 相关

- [partial 类型](partial-types.md) —— partial 的一般规则（嵌套是其中的例外）
