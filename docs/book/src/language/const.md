# const 编译期常量

> 对齐：2026-08-07（change `add-const-keyword`）

`const` 声明一个**编译期常量**：值在编译期即已确定，**没有存储**，每处引用都被编译器直接
替换为对应字面量。它既是意图声明，也是给优化器的最强"燃料"——替换出的字面量喂给
[常量折叠 pass](../runtime/optimization-pipeline.md#机制--实现)，并驱动
[常量条件死分支消除](../runtime/optimization-pipeline.md#机制--实现)。

与 [`readonly`](readonly-fields.md) 的区别：`readonly` 是**运行期**不可变（每实例有存储、
构造时赋一次）；`const` 是**编译期**常量（无存储、无实例、值内联到使用点）。

## 语法

const 可用于**静态常量字段**（类成员，隐式 static、无存储）与**局部常量**（方法体内）：

```z42
class Config {
    const int Max = 100;
    const int Doubled = Max * 2;         // ✓ 引用同类已定义 const
    const string Tag = "cfg-" + "v1";    // ✓ 编译期字符串拼接
    const bool Debug = false;
}

void Main() {
    const int n = 8;
    const int m = n * 4;                 // ✓ 引用已定义局部 const
    int y = m + Config.Max;              // 132（全部替换为字面量后折叠）
}
```

## 初始化器：编译期常量表达式

const 的初始化器必须是**编译期常量表达式**——由以下构成：

- 字面量：`int` / `bool` / `char` / `double` / `string` / `null`；
- 一元 `- + ! ~`、二元 算术 / 比较 / 逻辑 / 位运算、字符串 `+` 拼接；
- 对**已定义** const 的引用（同类字段裸名 / `Class.FIELD` 限定名 / 在作用域内的局部 const）。

"已定义"= 声明在前（按声明序求值，不允许前向引用或循环依赖）。

## 强制规则（编译期诊断）

| 情形 | 诊断 |
|------|------|
| const 声明缺初始化器（`const int X;`） | `E0416` |
| 初始化器不是编译期常量（如含函数调用） | `E0417` |
| 对 const 赋值（`Config.Max = 5;` / `n = 3;`） | `E0418` |
| 初始化器引用了非 const / 未定义的符号 | `E0419` |

```z42
class C {
    const int X;              // ✗ E0416：缺初始化器
    const int Y = f();        // ✗ E0417：非编译期常量
    void set() { C.X = 5; }   // ✗ E0418：不可对 const 赋值
}
void g() {
    int z = 3;
    const int W = z;          // ✗ E0419：z 不是 const
}
```

## 语义与优化

- **无存储**：const 字段不进对象布局、不产生静态字段槽、不发静态初始化；const 局部不占运行期变量
  （其存储若被发射，也因引用全替换成字面量而由 DCE 清理）。
- **字面量替换**：`Config.Max` / 局部 `n` 的每处引用在 codegen 阶段直接发射
  `const.i64 100` / `const.bool …` 等，而非 `static_get` / 局部加载。
- **喂常量折叠**：替换出的字面量参与既有 ConstFold——`const int N=3; i < N` 中 `N` 变字面量 `3`，
  比较随之折叠；`Config.Max * 5` 折成 `500`。
- **死分支消除**：`const bool Debug=false; if (Debug) { … }` 的条件变常量 `false`，
  [dead-branch pass](../runtime/optimization-pipeline.md#机制--实现) 把 `br.cond` 折成无条件 `br`
  并移除不可达的 then 块。

## 当前边界（Deferred）

- **跨 zpkg const**：v1 仅同模块（const 无字段元数据 → 别的包看不到其值）。跨包需 zbc/zpkg 格式 bump
  把 const 值写进导出元数据。
- **跨类 / 跨作用域的 const 初始化器引用**：v1 中 const 初始化器只引用**同类**已定义 const 字段（字段侧）
  或**在作用域内**的局部 const（局部侧）；跨类 `Other.X` 作为初始化器引用留待后续。
- **const 引用 enum 成员 / const 数组 / const 对象**：v1 仅原始类型常量。

## 关联文档

- 机制 / 优化：[优化管线](../runtime/optimization-pipeline.md)（const 传播 + dead-branch）
- 对比：[readonly 字段](readonly-fields.md)（运行期不可变）
- 引入：change `add-const-keyword`（`docs/spec/archive/`）
