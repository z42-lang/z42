# 异常处理

程序总会遇到做不下去的情况：文件不存在、参数不合法、除数是 0。**异常**是把「我做不了」
这件事从出错的地方一路抛出去、直到有人接住并决定怎么办的机制。

这一章讲怎么抛（`throw`）、怎么接（`try` / `catch`）、怎么保证收尾（`finally`），
以及怎么写自己的异常类型。

## 抛出与接住

```z42
// examples/types/exceptions/basic/basic.z42
{{#include ../../../../examples/types/exceptions/basic/basic.z42:basic}}
```

```console
{{#include ../../../../examples/types/exceptions/basic/run.console:basic}}
```

三件事：

- **`throw` 之后那一行不会执行**——控制权立刻交给最近的匹配 `catch`，中间的代码全部跳过。
- **`catch (T e)`** 接住 `T` 及其子类的异常，`e` 是接住的那个异常对象。
- **`finally` 一定会跑**——不管是正常走完、被 `catch` 接住，还是异常继续往外抛。
  收尾动作（关文件、释放锁）放这里。

> 只要有人接住了，程序就继续往下跑；**没人接住才会终止**并打印
> `Error: uncaught exception: …`。

## 谁接住：匹配规则

`catch (T e)` 接住 `T` **及其子类**——所以接父类就等于把一整族都接了：

```z42
// examples/types/exceptions/order/order.z42
{{#include ../../../../examples/types/exceptions/order/order.z42:subclass}}
```

```console
{{#include ../../../../examples/types/exceptions/order/run.console:subclass}}
```

多个 `catch` 按**源码顺序**试，第一个匹配的胜。所以**范围大的要写在后面**，
否则后面那条永远进不来：

```z42
// examples/types/exceptions/order/order.z42
{{#include ../../../../examples/types/exceptions/order/order.z42:order}}
```

```console
{{#include ../../../../examples/types/exceptions/order/run.console:order}}
```

不需要那个异常对象时可以省掉变量名；连类型都不关心就写 `catch { }`：

```z42
// examples/types/exceptions/order/order.z42
{{#include ../../../../examples/types/exceptions/order/order.z42:catchall}}
```

```console
{{#include ../../../../examples/types/exceptions/order/run.console:catchall}}
```

`catch { }` 接住**任何**抛出物 —— 这一点比 `catch (Exception e)` 更宽，
本章最后会讲到差别在哪。

> 熟悉 C# 的读者请注意：z42 **没有**异常过滤器 `catch (T e) when (cond)`，
> 也**没有**裸 `throw;` 重抛（都见本章最后）。

## `Exception`：所有异常的基类

```text
class Exception {
    string    Message;           // 出了什么事
    string    StackTrace;        // 从哪儿抛出来的
    Exception  InnerException;    // 被它包着的原始异常（没有则 null）
}
```

自己抛的时候一般用标准子类，它们都在 `Std` 里、不用 `using` 就能用：

| 用它 | 什么时候 |
|---|---|
| `ArgumentException` | 参数不合法 |
| `ArgumentNullException` | 参数是 null 但不允许（它是 `ArgumentException` 的子类）|
| `InvalidOperationException` | 对象当前状态不允许这个操作（比如空队列出队）|
| `KeyNotFoundException` | 字典里没这个键 |
| `FormatException` | 解析 / 格式化失败 |
| `NotImplementedException` | 声明了还没写 |
| `NotSupportedException` | 这个场景不支持（比如往只读集合里加）|
| `OverflowException` | 数值超出目标类型 |
| `DivideByZeroException` | 整数除 0（**浮点除 0 不抛**，按 IEEE 754 给 `inf` / `nan`）|
| `InvalidCastException` | 硬转换 `(T)x` 失败（`as` 失败给 null，不抛）|

完整清单见
[异常](https://z42-lang.github.io/z42/reference/language/exceptions.html)。

## 写自己的异常类型

继承 `Exception`，用 `: base(...)` 把消息交给基类；想带额外信息就自己加字段：

```z42
// examples/types/exceptions/custom/custom.z42
{{#include ../../../../examples/types/exceptions/custom/custom.z42:decl}}
```

```z42
// examples/types/exceptions/custom/custom.z42
{{#include ../../../../examples/types/exceptions/custom/custom.z42:use}}
```

```console
{{#include ../../../../examples/types/exceptions/custom/run.console:use}}
```

**加字段是自定义异常的主要价值**：`e.Message` 只是一句话，而 `e.Key` 是接住的人可以拿去
继续处理的数据。

> 📜 **2026-09-26 之前 `ToString()` 会打错类名**：上面最后一行会打成
> `Exception: 找不到：bob` 而不是 `NotFoundException: ...`——基类里那个名字是写死的。
> 现在它按真实类型取名，自定义异常不必再自己重写 `ToString`。

## 换个说法抛出去，但别丢掉原因

底层的异常信息往往太细（「第 3 行少了一个引号」），调用方需要的是「配置文件读不了」。
两个都要，就把原来那个**包进去**：

```z42
// examples/types/exceptions/wrap/wrap.z42
{{#include ../../../../examples/types/exceptions/wrap/wrap.z42:wrap}}
```

```console
{{#include ../../../../examples/types/exceptions/wrap/run.console:wrap}}
```

`new Exception(消息, 原始异常)` 的第二个参数就存进 `InnerException`。
排查问题时顺着这条链往里看，能一直看到最初的原因。

## `StackTrace`：它从哪儿抛出来的

抛出时 `StackTrace` 会被自动填上调用栈，**最里层在最上面**：

```z42
// examples/types/exceptions/trace/trace.z42
{{#include ../../../../examples/types/exceptions/trace/trace.z42:trace}}
```

```console
{{#include ../../../../examples/types/exceptions/trace/run.console:trace}}
```

每行是「哪个函数（哪个文件:行:列）」。读法是**从上往下**：`Innermost` 抛的，
它被 `Middle` 调，`Middle` 被 `Main` 调。

- 把同一个异常对象再抛一次**不会**覆盖已经填好的调用栈——第一次抛出的位置被保留下来。
- `--release` 构建会把行号信息剥到旁边的 `.zsym` 文件里。**把 `.zsym` 和 `.zpkg` 放在一起，
  调用栈照常带行号**；分开了就只剩函数名。

## 🔴 当前实现的边界

### 数组越界：只有 `catch { }` 接得住

```z42
// examples/types/exceptions/gaps/oob.z42
{{#include ../../../../examples/types/exceptions/gaps/oob.z42}}
```

```z42
// examples/types/exceptions/gaps/oob2.z42
{{#include ../../../../examples/types/exceptions/gaps/oob2.z42}}
```

```console
{{#include ../../../../examples/types/exceptions/gaps/run.console:oob}}
```

下标越界时抛出的东西**不是 `Exception` 的实例**，所以 `catch (Exception e)` 认不出它，
程序照样终止；只有什么都接的 `catch { }` 能拦下来。

⚠️ 别把 `catch { }` 当成解决办法 —— 它会把**所有**意外都吞掉，包括你没预料到的。
正确做法还是**访问下标前自己确认范围**。

> 同样的规则适用于 `throw "一个字符串"` / `throw 42` 这类**不是异常对象**的抛出
> （语法上还允许，但新代码别这么写）：它们也只有 `catch { }` 接得住。

### 没有裸 `throw;` 重抛

```z42
// examples/types/exceptions/gaps/rethrow.z42
{{#include ../../../../examples/types/exceptions/gaps/rethrow.z42}}
```

```console
{{#include ../../../../examples/types/exceptions/gaps/run.console:rethrow}}
```

写 `throw e;`（把接住的那个对象再抛一次）—— 效果一样，而且调用栈不会丢，
因为重抛不覆盖已填的 `StackTrace`。

### 没有异常过滤器

```z42
// examples/types/exceptions/gaps/filter.z42
{{#include ../../../../examples/types/exceptions/gaps/filter.z42}}
```

```console
{{#include ../../../../examples/types/exceptions/gaps/run.console:filter}}
```

在 `catch` 体里用 `if` 判断，不满足条件就 `throw e;` 抛回去。

### 另外两条

- **`catch (e)` 这种写法不行**——括号里的单个标识符会被当成**类型名**（报「undefined type: e」）。
  要变量就写全 `catch (Exception e)`，不要变量就写 `catch { }`。
- **`catch` 的类型不要求是异常**——`catch (Foo f)` 里写个毫不相干的类也能编译通过，
  只是永远匹配不上。编译器目前不检查这件事，写错了不会有人提醒。

## 小结

- `throw` 抛、`catch` 接、`finally` 收尾（**一定会跑**）；没人接住程序才终止。
- `catch (T e)` 接 `T` **及其子类**；多个 `catch` 按源码顺序，**范围大的写后面**。
  `catch { }` 接住一切。
- 标准子类按语义挑（参数问题用 `ArgumentException`、状态问题用
  `InvalidOperationException`……）；整数除 0 抛 `DivideByZeroException`，**浮点除 0 不抛**。
- 自定义异常 = 继承 `Exception` + `: base(消息)` + **自己加字段**（字段才是价值所在）。
- 包装用 `new Exception(消息, 原始异常)`，顺 `InnerException` 链能找到最初的原因。
- `StackTrace` 自动填，最里层在最上面；重抛同一对象不会覆盖它。
- 🔴 记住四个边界：数组越界与非对象抛出**只有 `catch { }` 接得住**、没有裸 `throw;`、
  没有异常过滤器 `when`、`catch (e)` 不是合法写法。

下一章讲**组织代码**——`namespace`、`using`、访问控制与 `partial`。
