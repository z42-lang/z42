# 异常 `throw` / `try` / `catch` / `finally`

> 对齐：2026-09-26 ｜ 实测基准：`./artifacts/.z42/z42 run`

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

### 非对象抛出（Phase 1 遗留）

`throw "some string"` / `throw 42` 仍然合法。这类抛出物**只能被 `catch { }` 接住**，
typed `catch (Exception e)` 不会捕获它们。新代码请一律 `throw new <某个 Exception 子类>(...)`。

> **数组越界不是可 typed-catch 的异常。** `a[5]` 越界时 VM 直接抛出
> `array index 5 out of bounds (len=1)`，它不是 `IndexOutOfRangeException` 实例，
> `catch (Exception e)` **接不住**，只有 `catch { }` 能接。名字相同的
> `IndexOutOfRangeException` 类存在，但要由库或用户代码自己 `throw`。

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
  `populate_stack_trace`（`820e583ce` "feat(jit): stack trace parity with interp"，2026-05-10）。
  ⚠️ 本页此前写着「JIT 不填（留 follow-up）」，那条 follow-up 当天就做完了、文档没跟上。
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
| `NullReferenceException` | `Exception` | 解引用 null |
| `IndexOutOfRangeException` | `Exception` | 索引越界（**由库/用户代码抛**，VM 的数组越界不走它） |
| `KeyNotFoundException` | `Exception` | 字典 / Map 找不到键 |
| `FormatException` | `Exception` | 字符串解析 / 格式化失败 |
| `NotImplementedException` | `Exception` | 方法已声明但未实现 |
| `NotSupportedException` | `Exception` | 方法不支持当前场景（如对只读集合 `Add`） |
| `OverflowException` | `Exception` | 数值运算超出目标类型容量（`Int32.Parse` 溢出、checked 溢出） |
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
⚠️ 那些 override 如今是**冗余**的：基类已按运行期类型取名，输出一字不差。
（`IOException` **尚不存在**。）

> 📜 **2026-09-26 之前 `Exception.ToString()` 硬编码字面量 `"Exception: "`** ⇒
> **用户自定义的异常子类一律打错类名**（`class NotFoundException : Exception` 的实例得到
> `"Exception: …"`）；stdlib 的子类看起来没事，只是因为每一个都重复硬编码了自己的名字。
> 现在基类用 `this.GetType().Name`，自定义异常不必再自己重写 `ToString`。
> ⭐ 全仓没有任何 golden 打印**用户自定义**异常的 `ToString()`，所以这个缺陷一直没被测试抓到。

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
