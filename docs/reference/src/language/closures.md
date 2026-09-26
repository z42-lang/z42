# 闭包与捕获语义

> 对齐：2026-09-26

lambda 的写法、函数类型 `(T) -> R` 的写法、局部函数的声明形式见[函数与方法](functions.md)。
本页只讲一件事：**lambda 引用了外层的变量时，它到底看见什么。**

一句话：**值类型按快照捕获，引用类型按对象身份共享。**

## 值类型按快照捕获

创建闭包时**拷贝**值类型变量的当前值；此后外部再改那个变量，闭包看不见。

```z42
var x = 5;
var f = () => Console.WriteLine(x);
x = 10;
f();   // 5 —— 不是 10
```

这与 C# **不同**：C# 把被捕获的局部变量提升进 display class、按引用共享，所以 C# 里这段会打印 10。
z42 选快照，是为了一次性消掉 C# 两类经典陷阱——「循环变量晚绑定」和值类型幻读。

## 引用类型按对象身份共享

捕获的是**对象身份**，闭包内外是同一个对象：

```z42
class Counter { public int n = 0; }

var c = new Counter();
var inc = () => c.n = c.n + 1;
inc(); inc();
Console.WriteLine(c.n);   // 2
```

外部把变量重新指向另一个对象，**不影响**已创建的闭包——闭包握着的是创建时那个对象：

```z42
var c = new Counter();
var inc = () => c.n = c.n + 1;
c = new Counter();   // c 指向新对象
inc();               // 仍然改的是原来那个
```

## 循环变量每次迭代是新绑定

`for` / `foreach` / `while` 都一样，每轮迭代捕获到的是那一轮的值：

```z42
var fns = new List<() -> int>();
foreach (var i in new int[] { 1, 2, 3 }) {
    fns.Add(() => i);
}
foreach (() -> int f in fns) { Console.WriteLine(f()); }   // 1 2 3 —— 不是 3 3 3
```

这是值快照规则（上一节）的直接后果，不是循环的特殊规则。C 风格 `for` 同样成立——这一点比 C# 走得更远，
C# 5 只修了 `foreach`。

> 上面第二个 `foreach` 的循环变量必须写出 `() -> int` 而不能用 `var`：函数值必须先落到一个
> **写明函数类型**的变量上才能调用，理由见[函数与方法](functions.md)。

## 要在闭包里改外部状态：用引用类型

因为值类型是快照，「在闭包里赋值给捕获来的变量」只改到闭包自己的副本：

```z42
bool seen = false;
var w = () => { seen = true; };
w();
Console.WriteLine(seen);   // false —— 写的是闭包的局部副本
```

这在「用闭包把结果发布出来」的写法里最容易踩到（回调、线程体、事件处理器）。两条修法：

```z42
// 路 A —— 用一个 class 装（推荐，语义最清楚）
class Cell { public bool seen = false; }
var cell = new Cell();
var w1 = () => { cell.seen = true; };
w1();
Console.WriteLine(cell.seen);   // true

// 路 B —— 单元素数组当 cell（轻量惯用法，数组是引用类型）
bool[] box = new bool[1];
var w2 = () => { box[0] = true; };
w2();
Console.WriteLine(box[0]);      // true
```

写**引用类型**变量的字段（`cell.seen = ...`）或数组元素（`box[0] = ...`）是在改已捕获对象的内部状态，
正是上面「对象身份共享」那条规则；只有「赋值给捕获来的变量本身」才是写副本。

z42 **不提供** Rust 风格的 `Ref<T>` / `Box<T>` 包装类型——共享可变状态的推荐路径就是 class。

> 编译器目前**不会**对「在闭包里赋值给捕获来的值类型变量」发出任何警告。诊断码 `W0604`
> 只在码表里存在，没有任何发射点。

## 捕获的边界

- `ref` 形参**可以**被 lambda 捕获，但捕获到的是一份**值快照**（与其它值类型一致），
  不是调用方那个存储位置。想经闭包改到调用方的变量，走上一节的 class / 数组 cell。
  （`out` / `in` 已随 add-single-ref-keyword 移除，z42 只有 `ref`。）
- 局部函数与 lambda 的捕获规则完全一致。

## 单目标：没有多播

一个函数类型的值最多绑定**一个**目标，不支持 C# 的 `+=` / `-=` 组合：

```z42
(int) -> void f = (int x) => Console.WriteLine(x);
f += (int x) => Console.WriteLine(x * 2);
```

> ⚠️ 这行**当前编译得过**，运行到才炸：
> `type mismatch in arithmetic: FuncRef(...) vs FuncRef(...)`。编译期拦截未实现。

要多个订阅者就用 stdlib 的 `MulticastAction<T>` / `MulticastFunc<TArg,TResult>` /
`MulticastPredicate<T>`，退订用 `Subscribe` 返回的 `IDisposable` token。完整规则见
[委托与事件](delegates-events.md)。

## 比较与序列化

- **不要用 `==` 比较闭包或函数引用**。`f == f` 当前返回 **`false`**，两个指向同一个函数的引用
  相比也返回 `false`——`==` 没有接到委托相等语义上。要比较引用身份用
  `DelegateOps.ReferenceEquals(a, b)`（见[委托与事件](delegates-events.md)）。
- 闭包不实现任何序列化契约，不能被序列化。

## 并发

闭包跨线程用 `Std.Threading.Thread.Start(Action)`。跨线程时**值快照捕获仍然生效**——线程体里
给捕获来的值类型变量赋值，主线程读不到，必须用 class 或数组 cell 把结果传回来（见上）。

z42 目前**没有** `spawn` / `task` 语法，也没有自动 move 捕获或 `Send` 派生；跨线程共享的安全性由
你自己用引用类型与同步原语保证。

## 相关

- [函数与方法](functions.md)——lambda 字面量、`(T) -> R` 函数类型、局部函数的写法
- [委托与事件](delegates-events.md)——单播 / 多播委托、`event`、`+=` / `-=`、引用相等
- [所有权与内存模型](memory-model.md)——值类型 / 引用类型的复制规则
- [迭代（foreach）](iteration.md)——接收谓词的高阶 API
