# 源文件的顶层结构

一个 `.z42` 源文件（编译单元）由**可选的 namespace 声明 → using 指令 → 顶层声明**三段构成，
顺序固定。本页只讲这三段的排布规则；namespace / using 的解析语义见
[Namespace 与 Using](namespaces.md)。

## 骨架

```z42
namespace Geometry;          // ① 可选，至多一条，必须在任何声明之前

using Std;                   // ② 导入
using Std.IO;

void Main() {                // ③ 顶层声明（此处是顶层函数）
    Console.WriteLine("hello");
}
```

> **注意导入的是 `Std.*`，不是 `System.*`。** z42 的根命名空间是 `Std`（标准库）与 `Z42`
> （编译器自身），**没有 `System`**。`using System;` 带不进任何类型：写 `Console.WriteLine`
> 仍会报 `E0436`（namespace `Std.IO` is used but not imported in this file）。
> `Console` 在 `Std.IO`，`List<T>` / `Dictionary<K,V>` 在 `Std.Collections`，`Math` 在 `Std`。

## ① namespace

只有**文件级**形式 `namespace X;`。块形式 `namespace X { ... }` 不是 z42 语法，会在 `{` 处
报语法错误。

| 规则 | 违反时 |
|------|--------|
| 一个文件至多一条 `namespace` | `E0457`：a file may declare only one namespace |
| `namespace` 必须在所有类型 / 函数声明之前 | `E0457`：`namespace` must appear before any type or function declaration |

不写 `namespace` 时，声明落在默认命名空间（IR 模块名为 `main`）。

## ② using

三种形态，都是**文件级**作用域（写在哪个文件只对哪个文件生效）：

```z42
using Std.IO;                       // 导入命名空间
global using Std.Collections;       // 包级导入：注入本包每个文件
using Row = Dictionary<string,int>; // 类型别名
```

- `global` 是**上下文标识符**而非关键字——只有紧跟 `using` 时才被特殊识别。
- 类型别名的判别条件是 `using` 后面紧跟 `标识符 =`。别名与目标类型完全互通
  （`using UserId = int;` 之后 `UserId` 就是 `int`）。

三者的完整语义（包级激活、file-scope 强制导入、`E0436` / `E0601` / `E0602`）见
[Namespace 与 Using](namespaces.md)。

## ③ 顶层声明

顶层可以直接写：`class` / `struct` / `interface` / `enum` / `delegate` / `impl` 块，以及
**顶层函数**（不必包在类里）：

```z42
int Add(int a, int b) { return a + b; }
int Twice(int a) => a * 2;              // 表达式体
```

两条顶层专有规则：

- **顶层声明不能标 `private` / `protected`**。模块作用域下这两个修饰符没有意义；默认可见性
  是 `internal`，要跨包可见写 `public`。见[访问控制](access-control.md)。
- **顶层函数参与重载**。同名顶层函数按参数类型序列区分，与类型成员方法同一套重载决议：

  ```z42
  int Add(int a, int b)       { return a + b; }
  int Add(double a, double b) { return 0; }   // ✓ 合法重载（参数类型不同）
  ```

  只有**签名完全相同**才是重复声明，报 `E0408: duplicate top-level function`。

## 入口函数

驱动按顺序查找 `{Namespace}.Main` → `Main` → `{Namespace}.main` → `main`，找不到则报错退出。

## 局部诊断抑制 `#suppress` / `#restore`

```
suppress_directive ::= "#" "suppress" RuleId [ StringLiteral ]
restore_directive  ::= "#" "restore"  RuleId
```

在**语句列表**或**顶层声明列表**的边界上，可以用一对指令圈出一段区域，抑制其中某一条规则的
诊断（analyzer lint 与 `deprecated` 提示）：

```z42
namespace Demo;
using Std.IO;

void Main() {
    #suppress Z9002 "generated code, empty catch is intentional"
    try { Risky(); } catch (Exception e) { }
    #restore Z9002
}
```

规则：

- `RuleId` 是一个裸标识符（如 `Z9002`、`deprecated`），**精确匹配，不支持通配符**。
  `#suppress Z9999` 不会抑制 `Z9002`。
- 可选的字符串是给人看的理由，编译器接受后丢弃。
- 抑制区间是 `[#suppress 起点, #restore 起点)` 的**字节区间**——指令本身的位置决定边界，
  而不是语法结构。写在 `try` 之前才圈得住 `try` 内的诊断。
- 同一 Id 可以嵌套，`#restore` 按 **LIFO** 关掉最近一个同 Id 的开区间。
- **缺 `#restore` 不报错**，该抑制区间一直延伸到文件末尾。
- 多余的 `#restore`（没有匹配的开区间）被安静忽略。
- 单独的 `#`（后面不是 `suppress` / `restore`）仍然是语法错误。

> 抑制只对**可抑制的诊断**（analyzer 规则、废弃提示）生效；语法错误和类型错误不受它影响。

## 注释

```z42
// 行注释
/* 块注释，
   可以跨行 */
```

## 关联页面

- [Namespace 与 Using](namespaces.md) — 导入解析、包激活、`global using`、类型别名
- [访问控制](access-control.md) — `public` / `internal` / `private` / `protected` 的适用位置
- [基本类型](types.md) — 顶层声明里能用的类型
