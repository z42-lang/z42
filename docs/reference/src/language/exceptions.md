# 异常 `throw` / `try` / `catch` / `finally`

## 抛出与捕获

```z42
try {
    if (x < 0)
        throw new ArgumentException("x must be non-negative");
    DoWork(x);
} catch (ArgumentException e) {
    Console.WriteLine(e.Message);
} catch {
    Console.WriteLine("其它任何东西");
} finally {
    Cleanup();
}
```

- `throw <expr>` 把值交给最近的匹配 `catch`。
- `catch (T e)` 只捕获 `T` 及其子类；`catch (T)` 同样，只是不绑定变量。
- `catch { }` 捕获**任意**抛出物，包括下面说的非对象抛出。
- 多个 `catch` 按**源码顺序**，第一个匹配者胜（与 C# / Java / Python 一致）。

### 匹配规则

| 形式 | 匹配 |
|---|---|
| `catch (T e)` / `catch (T)` | 抛出物是对象，且其类沿基类链能上溯到 `T` |
| `catch { }` | 任意抛出物 |

`catch (T e)` 中的 `T` **不要求派生自 `Std.Exception`**——写一个毫不相干的类也能编译，只是永远匹配不上：

```z42
class Foo { }
try { throw new Exception("x"); }
catch (Foo f) { /* 永远不会进来 */ }
catch          { /* 落到这里 */ }
```

> `E0420 InvalidCatchType` 只在错误码表里**定义**了，编译器里**没有任何发射点**——
> 既不校验 `T` 是否存在于 `Exception` 链上，也不因此报错。这是一条未实现的检查，不要依赖它。

### 泛型异常类型

`catch (MulticastException<int> e)` 透明工作：编译器把类型实参编进 mangled 名
（`Std.MulticastException$1`），运行期按同一个名字比对并沿基类链上溯。

### 非对象抛出

`throw "some string"` / `throw 42` 仍然合法。这类抛出物**只能被 `catch { }` 接住**，
typed `catch (Exception e)` 不会捕获它们。新代码请一律 `throw new <某个 Exception 子类>(...)`。

### 运行期自动抛出的异常

下面这些错误由运行期直接抛出标准异常，可以按类型 `catch`：

| 情形 | 异常 | `Message` 示例 |
|---|---|---|
| 读 / 写 null 引用的字段（含 `.Length`） | `NullReferenceException` | ``cannot read field `N` of a null reference`` |
| 读 / 写 null 数组的元素 | `NullReferenceException` | `cannot read an element of a null array` |
| 数组下标越界（含负数） | `IndexOutOfRangeException` | `index 5 is out of range for an array of length 1` |
| `new T[n]` 的 `n` 为负 | `OverflowException` | `array size cannot be negative (got -2)` |
| 整数除 / 取模的除数为 0 | `DivideByZeroException` | |
| 硬转换失败 | `InvalidCastException` / `NullReferenceException` | 见[类型转换](conversions.md) |

null 检查先于下标检查：`a[-1]` 在 `a` 为 null 时抛 `NullReferenceException`。

## `Exception` 基类

```z42
public class Exception {
    public string Message;           // 异常消息
    public string StackTrace;        // 调用栈快照，见下
    public Exception InnerException; // 包裹的原始异常（无则 null）

    public Exception(string message);
    public Exception(string message, Exception inner);   // wrapping

    override string ToString();      // "<运行期类名>: <Message>"
}
```

两个构造器都可用，wrapping 模式直接写：

```z42
var inner = new Exception("cause");
var outer = new Exception("wrap", inner);
outer.InnerException.Message;      // "cause"
```

事后赋值 `outer.InnerException = inner;` 同样可用。

### `StackTrace`

解释执行时，`throw <exception>` 会在 `StackTrace` 仍为 null 的情况下**自动填入**多行调用栈：

```
  at Inner() (demo.z42:2:16)
  at Outer() (demo.z42:3:16)
  at Main() (demo.z42:5:5)
```

- 重抛同一个对象**不会**覆盖已填的 trace。
- **JIT 路径同样填** trace —— `jit/helpers/control.rs` 的 throw helper 调的是同一个
  `populate_stack_trace`。
  ⚠️ **验这件事不能只看 `--mode jit` 跑通** —— 小用例里抛出的函数可能全程解释执行；
  判据是 `Z42_JIT_PROFILE=1` 里有没有该函数的 `lazy-compile` / `osr-compile`。
- 帧名带参数类型签名（如 `Greeter.greet(Greeter,str)`），实例方法含隐式 `this`。
- release 构建（strip）会把行号信息剥离到同目录的 `<name>.zsym` 旁挂文件；
  **把 `.zsym` 和 `.zpkg` 放在一起，trace 就照常带 `file:line:col`**，否则只剩函数名 + 偏移。
  归档了 `.zsym` 的崩溃栈可以事后用 `z42d symbolicate <trace> --syms <file|dir>...` 还原。

## 标准异常子类

全部位于 `Std` 命名空间，随 prelude 自动可用。

| 子类 | 继承自 | 何时用 |
|---|---|---|
| `ArgumentException` | `Exception` | 参数非法（值或组合不符合契约） |
| `ArgumentNullException` | `ArgumentException` | 参数为 null 但要求非空 |
| `InvalidOperationException` | `Exception` | 对象当前状态不允许此操作（如空 Queue 出队） |
| `NullReferenceException` | `Exception` | 解引用 null（字段 / 数组访问由运行期自动抛） |
| `IndexOutOfRangeException` | `Exception` | 索引越界（数组下标越界由运行期自动抛） |
| `KeyNotFoundException` | `Exception` | 字典 / Map 找不到键 |
| `FormatException` | `Exception` | 字符串解析 / 格式化失败 |
| `NotImplementedException` | `Exception` | 方法已声明但未实现 |
| `NotSupportedException` | `Exception` | 方法不支持当前场景（如对只读集合 `Add`） |
| `OverflowException` | `Exception` | 数值运算超出目标类型容量（`Int32.Parse` 溢出、checked 溢出）；数组长度为负 |
| `DivideByZeroException` | `Exception` | 整数除 / 取模的除数为 0（浮点除 0 按 IEEE 754 返回 ±∞ / NaN，不抛） |
| `InvalidCastException` | `Exception` | 硬转换 `(T)x` 失败：`x` 非 null 但不是 `T`（`as` 失配返 null、不抛） |
| `SwitchExpressionException` | `Exception` | `switch` **表达式**求值时无任何臂被采纳（没匹配上，或匹配了但守卫为假）。消息含落空的值；`switch` **语句**不抛。见 [模式匹配](pattern-matching.md) |
| `OutOfMemoryException` | `Exception` | 堆内存不足（strict OOM 模式下超 `max_heap_bytes`） |
| `TypeInitializationException` | `Exception` | 静态构造器抛出；该类型此后不可用，后续访问直接重抛，**不重试 cctor** |
| `MissingSymbolException` | `Exception` | 运行期解析不到符号，通常意味着依赖版本 skew |
| `InvalidMarshalException` | `Exception` | z42 值无法 marshal 成 native ABI 类型 |
| `AggregateException` | `Exception` | 聚合多个异常，携带 `InnerExceptions` 数组 |
| `MulticastException` | `AggregateException` | 多播委托 `Invoke(continueOnException: true)` 时聚合各 handler 的异常 |

每个子类只有 ctor 转发 + 一个 `override ToString()`（返回 `"<ClassName>: <Message>"`）——
⚠️ 那些 override 是**冗余**的：基类已按运行期类型取名，输出一字不差。
（`IOException` **尚不存在**。）

> `Exception.ToString()` 用 `this.GetType().Name` 取类名，所以**用户自定义的异常子类**
> （`class NotFoundException : Exception`）自动得到 `"NotFoundException: …"`，不必自己重写 `ToString`。

## 栈溢出是致命错误

递归太深、原生栈用完时，VM **不抛异常**，而是终止：`catch` 拦不住，`finally` 也不会执行。
`z42vm` 在 stderr 打印 `fatal error: stack overflow` 和 z42 调用栈（太深时保留最内层和最外层两段），
以退出码 **3** 结束；嵌入 API 返回 `Z42_HOST_ERR_FATAL`（见 [C ABI](../embedding/c-abi.md)）。

能递归多深取决于线程的原生栈：主线程由操作系统决定（桌面一般 8 MB），VM 创建的线程用运行时设置
[`thread-stack-bytes`](../toolchain/runtime-settings.md)（默认 16 MB）。

不做成可 catch 的原因：两次栈检查之间有些原生代码的栈用量没有上界，运行时在那里用完栈就只能崩溃，
无法保证每一次溢出都变成异常。与其提供一个有时能 catch、有时直接崩的异常，不如一律按致命错误处理。

## 当前限制

| 限制 | 说明 |
|---|---|
| `catch (e)` 无类型 + 绑定变量 | 不支持：单个标识符被当成类型名。要么 `catch (Exception e)`，要么 `catch { }` |
| Exception filter `catch (T e) when (cond)` | 不支持 |
| 裸 `throw;` 重抛 | **不支持**（解析错误）。写 `throw e;` —— 效果相同，且重抛不覆盖已填的 `StackTrace` |
| `E0420` catch 类型校验 | 定义了码，无发射点（见上） |

## 相关

- [错误码](../appendix/error-codes.md)
- [模式匹配](pattern-matching.md)
