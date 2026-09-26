# Lambda、闭包与委托

前面的函数都是有名字的。这一章讲**把行为当值传递**：写一个没有名字的小函数（lambda）、
让它记住外层的变量（闭包）、用委托类型接住它、再用事件把一串处理器挂到一个对象上。

## lambda 与函数类型

lambda 是**没有名字的函数字面量**，用粗箭头 `=>` 写：

```z42
// examples/types/lambdas/basic/basic.z42
{{#include ../../../../examples/types/lambdas/basic/basic.z42:forms}}
```

```console
{{#include ../../../../examples/types/lambdas/basic/run.console:forms}}
```

- **表达式体**：`=>` 右边是一个表达式，它的值就是返回值。
- **语句体**：`=>` 右边是花括号，里面写多条语句，用 `return` 给出结果。
- 装 lambda 的变量要有类型，这个类型叫**函数类型**，写成 `(参数类型…) -> 返回类型`。

> 两个箭头别弄混：**函数类型用细箭头 `->`，lambda 用粗箭头 `=>`**。
> `(int) -> int sq = (int x) => x * x;` 一行里两个都有。

没有返回值的写成 `(int) -> void`。

函数类型也能当形参类型——这就是「把行为传进去」：

```z42
// examples/types/lambdas/basic/basic.z42
{{#include ../../../../examples/types/lambdas/basic/basic.z42:param}}
```

形参类型已知时，lambda 的参数类型可以省略：

```z42
// examples/types/lambdas/basic/basic.z42
{{#include ../../../../examples/types/lambdas/basic/basic.z42:infer}}
```

```console
{{#include ../../../../examples/types/lambdas/basic/run.console:infer}}
```

## 闭包：lambda 记得住外层的变量

lambda 里可以直接用外层的局部变量，这样的 lambda 叫**闭包**。关键问题只有一个：
**它看见的到底是什么？** 一句话——**值类型按快照，引用类型按对象身份。**

```z42
// examples/types/lambdas/capture/capture.z42
{{#include ../../../../examples/types/lambdas/capture/capture.z42:snapshot}}
```

```console
{{#include ../../../../examples/types/lambdas/capture/run.console:snapshot}}
```

创建闭包的那一刻，`x` 的值被**拷走**了；之后外面怎么改都与闭包无关。

> 熟悉 C# 的读者请注意：这里**和 C# 不一样**。C# 把被捕获的局部变量提升成共享存储，
> 所以同样的代码在 C# 里打印 `10`。z42 选快照，是为了一次消掉 C# 里两类经典陷阱
> ——「循环变量晚绑定」和值类型的幻读。

引用类型捕获的是**那个对象**，闭包内外是同一个：

```z42
// examples/types/lambdas/capture/capture.z42
{{#include ../../../../examples/types/lambdas/capture/capture.z42:shared}}
```

```console
{{#include ../../../../examples/types/lambdas/capture/run.console:shared}}
```

### 循环变量每轮都是新的

```z42
// examples/types/lambdas/loopvar/loopvar.z42
{{#include ../../../../examples/types/lambdas/loopvar/loopvar.z42:loop}}
```

```console
{{#include ../../../../examples/types/lambdas/loopvar/run.console:loop}}
```

打出 `1 2 3` 而不是 `3 3 3`。这不是循环的特殊规则，而是上面「值类型按快照」的直接后果——
每轮捕获的就是那一轮的值。`for` / `foreach` / `while` 一视同仁。

> 上面第二个 `foreach` 的循环变量写的是 `() -> int f` 而不是 `var`：函数值必须先落到一个
> **写明函数类型**的变量上才能调用。

### 想在闭包里改外部状态，就用引用类型

因为值类型是快照，「在闭包里给捕获来的变量赋值」只改到闭包自己那份副本：

```z42
// examples/types/lambdas/capture/capture.z42
{{#include ../../../../examples/types/lambdas/capture/capture.z42:write}}
```

```console
{{#include ../../../../examples/types/lambdas/capture/run.console:write}}
```

`seen` 还是 `false`——写的是副本。而改一个**引用类型对象的字段**就真的改到了，因为闭包
握着的是那个对象本身。这条在回调、线程体、事件处理器里最容易踩到：**要把结果从闭包里
传出来，就装进一个类的字段**（单元素数组也行，数组是引用类型）。

> 编译器目前**不会**对「在闭包里给捕获来的值类型变量赋值」给任何提示，得自己留意。

## 委托：给函数类型起个名字

标准库提供了三个常用的**委托类型**，它们就是起好名字的函数类型：

| 类型 | 意思 |
|---|---|
| `Action<T>` | 吃一个参数，**没有**返回值 |
| `Func<T, R>` | 吃一个参数，返回 `R` |
| `Predicate<T>` | 吃一个参数，返回 `bool` |

```z42
// examples/types/lambdas/delegates/delegates.z42
{{#include ../../../../examples/types/lambdas/delegates/delegates.z42:types}}
```

```console
{{#include ../../../../examples/types/lambdas/delegates/run.console:types}}
```

`Action` 与 `Func` 都覆盖 0–4 个参数（`Predicate` 只有 1 个）。超过 4 个参数就自己用
`delegate` 关键字声明一个具名的。

调用有两种写法，**完全等价**，挑顺眼的用：

```z42
// examples/types/lambdas/delegates/delegates.z42
{{#include ../../../../examples/types/lambdas/delegates/delegates.z42:invoke}}
```

```console
{{#include ../../../../examples/types/lambdas/delegates/run.console:invoke}}
```

委托类型和 `(T) -> R` 函数类型**是同一回事**，可以互相赋值：

```z42
// examples/types/lambdas/delegates/delegates.z42
{{#include ../../../../examples/types/lambdas/delegates/delegates.z42:interop}}
```

```console
{{#include ../../../../examples/types/lambdas/delegates/run.console:interop}}
```

### 直接把具名函数当值传

有现成的函数就不必再用 lambda 包一层：

```z42
// examples/types/lambdas/delegates/delegates.z42
{{#include ../../../../examples/types/lambdas/delegates/delegates.z42:methodgroup}}
```

```z42
// examples/types/lambdas/delegates/delegates.z42
{{#include ../../../../examples/types/lambdas/delegates/delegates.z42:square}}
```

```console
{{#include ../../../../examples/types/lambdas/delegates/run.console:methodgroup}}
```

函数重载了也没关系：编译器按**目标委托类型**挑出签名完全对上的那一个。

实例方法也可以这样取（`Func<int,int> f = obj.Method;`）。🔴 但**类的静态方法**不行——
写成 `C.F` 或在类内直接写 `F` 都报错：

```z42
// examples/types/lambdas/gaps/staticmg.z42
{{#include ../../../../examples/types/lambdas/gaps/staticmg.z42}}
```

```console
{{#include ../../../../examples/types/lambdas/gaps/run.console:staticmg}}
```

变通办法：包一层 lambda（`Func<int,int> f = (int x) => Api.Twice(x);`），或者改写成自由函数
（自由函数本来就能直接取）。

## 事件：让别人挂处理器

`event` 修饰的字段可以被外部用 `+=` 挂上处理器、用 `-=` 摘下来。**能挂几个由字段类型决定**：

```z42
// examples/types/lambdas/events/events.z42
{{#include ../../../../examples/types/lambdas/events/events.z42:decl}}
```

| 字段类型 | 能挂几个 | 触发 |
|---|---|---|
| `MulticastAction<T>` / `MulticastFunc<..>` / `MulticastPredicate<T>` | 任意多个 | `X.Invoke(args)`，没人挂就什么都不做 |
| `Action<T>` / `Func<..>` / `Predicate<T>` | 至多一个 | 先取到局部变量再查空 |

> 熟悉 C# 的读者请注意：这是与 C# **最大的差别**。C# 的 `Action<T>` 本身就是多播的，
> 而 z42 的 `Action<T>` 永远只有一个处理器；要多播就写出 `MulticastAction<T>`。
> **类型说的就是实话**，不用猜 `event` 背后是几个。

多播字段由编译器自动初始化，所以**不用查空**：

```z42
// examples/types/lambdas/events/events.z42
{{#include ../../../../examples/types/lambdas/events/events.z42:multi}}
```

```console
{{#include ../../../../examples/types/lambdas/events/run.console:multi}}
```

单播字段默认是 `null`，触发前得先取到局部变量再查空（`var h = this.KeyDown;`
`if (h != null) { h.Invoke(k); }`）——**不能**写成 `this.KeyDown?.Invoke(k)`，z42 没有 `?.`。
已经有主了再挂一个会抛：

```z42
// examples/types/lambdas/events/events.z42
{{#include ../../../../examples/types/lambdas/events/events.z42:uni}}
```

```console
{{#include ../../../../examples/types/lambdas/events/run.console:uni}}
```

## 多播还能做的四件事

### 一、拿着退订票退订

`Subscribe` 返回一张 `IDisposable` 票，`Dispose()` 就是退订——**不依赖比较两个 lambda 相等**
（那件事做不到，见本章最后）：

```z42
// examples/types/lambdas/multicast/multicast.z42
{{#include ../../../../examples/types/lambdas/multicast/multicast.z42:token}}
```

```console
{{#include ../../../../examples/types/lambdas/multicast/run.console:token}}
```

### 二、用 `using` 管住订阅的生命周期

订阅票是 `IDisposable`，所以可以交给 `using`：块的**任何**退出路径（`return`、`break`、
抛异常）都会退订。

```z42
// examples/types/lambdas/multicast/multicast.z42
{{#include ../../../../examples/types/lambdas/multicast/multicast.z42:scoped}}
```

```console
{{#include ../../../../examples/types/lambdas/multicast/run.console:scoped}}
```

### 三、一个处理器抛异常，别的照样跑

默认是**首抛即停**（原异常直接抛出来）。传 `continueOnException: true` 则全部跑完，
最后用一个 `MulticastException` 把账报清楚：

```z42
// examples/types/lambdas/multicast/multicast.z42
{{#include ../../../../examples/types/lambdas/multicast/multicast.z42:continue}}
```

```console
{{#include ../../../../examples/types/lambdas/multicast/run.console:continue}}
```

`Failures` 与 `FailureIndices` 是两条**平行数组**：用同一个下标去两边取，就知道第几号
处理器抛了什么。`MulticastFunc` / `MulticastPredicate` 抛的是带值的版本，`Results` 里
失败的位置放的是默认值。

### 四、只响一次 / 弱引用

订阅时可以包一层**策略**，走 `SubscribeAdvanced`：

```z42
// examples/types/lambdas/multicast/multicast.z42
{{#include ../../../../examples/types/lambdas/multicast/multicast.z42:once}}
```

```console
{{#include ../../../../examples/types/lambdas/multicast/run.console:once}}
```

`OnceRef` 是**这一个订阅**响一次就失活，别的订阅不受影响。另外还有 `WeakRef`（处理器的
宿主对象被回收后自动失活，用来避免「忘了退订导致对象一直活着」）和 `CompositeRef`
（把两种叠加）。

> 注意 `Count` 是**方法** `Count()` 不是属性；包装策略走的是 `SubscribeAdvanced` 而不是
> `Subscribe` 的重载。完整 API 见
> [委托与事件](https://z42-lang.github.io/z42/reference/language/delegates-events.html)。

## `methodof`：精确指代一个方法

`typeof(T)` 指代一个类型，`methodof` 指代一个**方法**，求值得到一个 `MethodInfo`。
括号里写参数类型列表，重载也能指得一清二楚：

```z42
// examples/types/lambdas/methodof/methodof.z42
{{#include ../../../../examples/types/lambdas/methodof/methodof.z42:decl}}
```

```z42
// examples/types/lambdas/methodof/methodof.z42
{{#include ../../../../examples/types/lambdas/methodof/methodof.z42:use}}
```

```console
{{#include ../../../../examples/types/lambdas/methodof/run.console:use}}
```

它的价值是**把运行期的静默失效变成编译期报错**：写字符串 `"Handle"` 的话，方法改名、
签名变了都只能等到运行期才发现；`methodof` 会在引用点当场报错。没有重载时参数列表可以省
（`methodof(Api.Solo)`），但**一旦有重载就必须写**，否则报错——它绝不替你猜一个。

必须写成 `类型.成员` 的形式：自由函数指不了。

## 🔴 当前实现的边界

这些不是写法问题，是**当前实现的洞**，踩到时别怀疑自己。

### 从 `List` 里取出来的函数值不能就地调用

```z42
// examples/types/lambdas/gaps/listcall.z42
{{#include ../../../../examples/types/lambdas/gaps/listcall.z42}}
```

```console
{{#include ../../../../examples/types/lambdas/gaps/run.console:listcall}}
```

先赋给一个写明类型的局部变量再调（`Action<int> h = hs[0]; h(9);`）。
**数组不受影响**——`Action<int>[] arr; arr[0](9)` 是可以的；只有 `List` 的下标不行。
另外函数类型**不能**当数组元素类型（`((int) -> void)[]` 连声明都过不去），要么用委托类型
`Action<int>[]`，要么用 `List<(int) -> void>`。

### 不能用 `==` 比较两个函数值

```z42
// examples/types/lambdas/gaps/eq.z42
{{#include ../../../../examples/types/lambdas/gaps/eq.z42}}
```

```console
{{#include ../../../../examples/types/lambdas/gaps/run.console:eq}}
```

`f == f` 都是 `false`——`==` 没有接到委托上。要判「是不是同一个处理器」用
`DelegateOps.ReferenceEquals`。这也是退订要用 `IDisposable` 票而不是靠比较的原因。

### 实参个数写错

函数类型的调用会校验实参个数：

```z42
// examples/types/lambdas/gaps/arity.z42
{{#include ../../../../examples/types/lambdas/gaps/arity.z42}}
```

```console
{{#include ../../../../examples/types/lambdas/gaps/run.console:arity}}
```

多传报 `E1006`，少传报 `E1005`。⚠️ 用 `delegate` 声明时给形参写默认值**不生效**
（声明能过，调用时那个参数拿到的不是默认值），所以别指望靠默认值省略它。

### 调用 `null` 委托会直接终止程序

```z42
// examples/types/lambdas/gaps/nulldelegate.z42
{{#include ../../../../examples/types/lambdas/gaps/nulldelegate.z42}}
```

```console
{{#include ../../../../examples/types/lambdas/gaps/run.console:nulldelegate}}
```

`try` / `catch` **接不住**它——它不是可捕获的异常，而是直接终止执行。单播 event 字段默认
就是 `null`，所以触发前那一步查空是必须的。

### 没有「把两个处理器加起来」

z42 的一个函数值最多绑**一个**目标，`f += g` 这种组合写法不支持（写了能编过，运行到才炸）。
要多个处理器就用上面的 `MulticastAction<T>`。

## 小结

- lambda 用粗箭头 `=>`，装它的**函数类型**用细箭头 `->`：`(int) -> int sq = (int x) => x * x;`
- 闭包**值类型按快照、引用类型按对象身份**；循环变量每轮是新的（与 C# 不同）。
  要把结果传出闭包，写进一个引用类型对象的字段。
- `Action` / `Func` / `Predicate` 就是起好名字的函数类型，两者可以互相赋值；
  `f(x)` 与 `f.Invoke(x)` 等价；具名函数可以直接当值传。
- `event` 能挂几个处理器**由字段类型决定**：`MulticastAction<T>` 任意多个（自动初始化、
  不用查空），`Action<T>` 至多一个（默认 null、必须查空，挂第二个会抛）。
- 多播还给了退订票（配 `using` 更省事）、`continueOnException` 的异常聚合、
  `OnceRef` / `WeakRef` 订阅策略。
- `methodof(Type.Member(参数类型))` 精确指代一个方法，把运行期的静默失效变成编译期报错。
- 🔴 记住五个边界：`List` 取出的函数值要先赋给局部变量才能调、**类的静态方法**不能直接取引用
  （包一层 lambda）、`==` 比不了函数值（用 `DelegateOps.ReferenceEquals`）、调 `null` 委托
  直接终止程序、没有 `+=` 组合多播。

下一章讲**异常处理**——`try` / `catch` / `finally` 与自定义异常。
