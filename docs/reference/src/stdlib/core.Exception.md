<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Exception

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/Exception.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L8)

所有异常类型的推荐基类。

携带消息（`Message`）、调用栈快照（`StackTrace`）与包裹的原始异常（`InnerException`）。
throw 语法仍接受任意值（`throw "str"` 合法），但 stdlib 抛出的与新代码推荐统一继承本类，
为 catch 过滤 / Inner 链 / stack trace 打基础。

```z42
public class Exception {
    string Message
    string StackTrace
    Exception InnerException
    Exception(string message)
    Exception(string message, Exception inner)
}
```

成员：[`Message`](#Std.Exception.Message) · [`StackTrace`](#Std.Exception.StackTrace) · [`InnerException`](#Std.Exception.InnerException) · [`Exception`](#Std.Exception.ctor(string)) · [`Exception`](#Std.Exception.ctor(string,Exception))

<a id="Std.Exception.Message"></a>
### string Message

[`src/libraries/z42.core/src/Exceptions/Exception.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L10)

异常消息。

<a id="Std.Exception.StackTrace"></a>
### string StackTrace

[`src/libraries/z42.core/src/Exceptions/Exception.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L12)

调用栈快照（interp 模式下由 throw 自动填充）。

<a id="Std.Exception.InnerException"></a>
### Exception InnerException

[`src/libraries/z42.core/src/Exceptions/Exception.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L14)

包裹的原始异常（无则 null）。

<a id="Std.Exception.ctor(string)"></a>
### Exception(string message)

[`src/libraries/z42.core/src/Exceptions/Exception.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L17)

用消息构造；StackTrace / InnerException 默认为 null。

<a id="Std.Exception.ctor(string,Exception)"></a>
### Exception(string message, Exception inner)

[`src/libraries/z42.core/src/Exceptions/Exception.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/Exception.z42#L23)

用消息与包裹的原始异常构造（wrapping pattern）。

