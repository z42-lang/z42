# 控制流

z42 有五种循环 / 分支语句（`if` / `while` / `do-while` / `for` / `foreach`）、一个
`switch` 语句和一个 `switch` 表达式。

## `if` / `else`

```z42
if (x > 0) {
    Console.WriteLine("positive");
} else if (x < 0) {
    Console.WriteLine("negative");
} else {
    Console.WriteLine("zero");
}
```

条件必须是 `bool`。花括号可省（单条语句），但建议保留。

## `while` / `do-while`

```z42
while (count < 10) {
    count++;
}

do {
    count--;
} while (count > 0);
```

`do-while` 的循环体至少执行一次。

## `for`

```z42
for (int i = 0; i < 10; i++) {
    Console.WriteLine(i);
}

for (;;) {          // 三段都可以留空
    if (done) break;
}
```

> z42 **没有** `for (x in 0..10)` 这类范围循环，`..` 只在集合字面量的展开语法里出现。

## `foreach`

```z42
foreach (var item in collection) { }
foreach (string s in names)      { }   // 也可以写显式元素类型
```

`foreach` 能迭代数组，以及满足特定协议的用户类型。**不能迭代 `string`**——字符串没有数组
布局也没有协议成员，写了会在运行期 trap。三条可迭代路径和判定顺序见[迭代](iteration.md)。

## `break` 与 `continue`

- `break` 跳出**最近一层**的循环或 `switch`。
- `continue` 进入**最近一层循环**的下一轮；写在 `switch` 内部时会**穿透 `switch`**，作用于
  外层循环，而不是跳到下一个 `case`。

```z42
using Std.IO;

void Main() {
    int i = 0;
    while (i < 3) {
        i = i + 1;
        switch (i) {
            case 2: continue;      // 跳过 i=2 这一轮，回到 while
            default: break;        // 跳出 switch，继续往下执行
        }
        Console.WriteLine($"i={i}");   // 打印 i=1 和 i=3
    }
}
```

> z42 **没有** `goto`，也**没有**循环标签。要跳出多层循环，用一个标志变量或把内层循环抽成
> 一个方法提前 `return`。

## `switch` 语句

```z42
switch (subject) {
    case Pattern:             stmts
    case Pattern if guard:    stmts
    default:                  stmts
}
```

四条需要留意的规则，**每一条都会影响你怎么写**：

### 1. `break` 可以省，而且没有 fallthrough

每个 `case` 体执行完就隐式跳出 `switch`。下面这段只打印 `S`：

```z42
using Std.IO;

enum Direction { North, South, East, West }

void Main() {
    Direction d = Direction.South;
    switch (d) {
        case Direction.North: Console.WriteLine("N");
        case Direction.South: Console.WriteLine("S");
        default:              Console.WriteLine("other");
    }
}
```

写 `break` 也完全合法，效果相同——只是不再是必需的。

### 2. `default` 的位置不影响语义

`default` 只在**所有 `case` 都不匹配**时才走，写在哪一行都一样（与 C# 一致）：

```z42
int n = 2;
switch (n) {
    default: Console.WriteLine("other");   // 不会命中
    case 2:  Console.WriteLine("two");     // 命中这条
}
```

惯例仍是把 `default` 写在最后——读起来更顺——但这只是风格，不是要求。

> 2026-09 之前不是这样：`default` 会被就地当成最后一条，写在它后面的 `case` 永远不可达
> 且没有任何诊断。现已修正。

### 3. `case` 后面是完整的模式，不只是常量

常量、类型模式、关系模式、解构模式都可以：

```z42
using Std.IO;

class Rect { public int W; public int H; }

void Main() {
    Rect shape = new Rect();
    shape.W = 20;
    switch (shape) {
        case Rect { W: > 10 }: Console.WriteLine("wide"); break;   // 属性模式 + 关系子模式
        case Rect r:           Console.WriteLine("rect"); break;   // 类型模式 + 绑定
        default:               Console.WriteLine("other"); break;
    }
}
```

模式的完整形态见[模式匹配](pattern-matching.md)。

### 4. `case` 可以带守卫 `if`

守卫在模式匹配成功**之后**求值；为假则继续尝试下一个 `case`。分支按书写顺序逐一尝试，
**第一个匹配且守卫为真的分支胜出**：

```z42
using Std.IO;

void Main() {
    int n = 7;
    switch (n) {
        case int v if v > 5: Console.WriteLine($"big {v}"); break;   // 命中
        case 7:              Console.WriteLine("seven");   break;
        default:             Console.WriteLine("small");   break;
    }
}
```

## `switch` 表达式

```z42
string label = x switch {
    > 0  => "positive",      // 关系模式
    < 0  => "negative",
    _    => "zero"           // 丢弃模式兜底
};
```

`switch` 表达式的每个 arm 是 `模式 => 表达式`，用 `,` 分隔。

> ⚠️ **没有命中任何 arm 时结果是 `null`，不抛异常、不报警告。**
>
> ```z42
> int q = 9;
> string r = q switch { 1 => "one" };   // r == null
> ```
>
> 对 **enum** 和**封闭类层次**，编译器会做穷尽性检查并在缺分支时报警告 `W0700`
> （`switch on enum Color is not exhaustive: missing Color.Blue`），但对 `int` / `string`
> 这类开放类型不会。养成写 `_` 兜底的习惯。

穷尽性检查、封闭类层次、`when` 以外的所有模式形态见[模式匹配](pattern-matching.md)。

## 异常控制流

`throw` / `try` / `catch` / `finally` 见[异常](exceptions.md)。

> z42 **没有** `catch (E e) when (cond)` 这种 catch 过滤器——`when` 不是 `catch` 子句的一部分。
> 需要按条件区分时，在 `catch` 体内判断后重新 `throw`。

## 关联页面

- [迭代](iteration.md) — `foreach` 的三条路径与判定顺序
- [模式匹配](pattern-matching.md) — `case` / `switch` 表达式里能写的所有模式
- [异常](exceptions.md) — `try` / `catch` / `finally`
- [运算符](operators.md) — 条件表达式 `?:`、短路求值（`??` / `?.` 已移除）
