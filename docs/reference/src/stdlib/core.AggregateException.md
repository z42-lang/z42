<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.AggregateException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/AggregateException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/AggregateException.z42#L4)

多个异常聚合的基类，携带 `InnerExceptions` 数组，便于通用 catch 处理批处理失败场景。

```z42
public class AggregateException : Exception {
    Exception[] InnerExceptions
    AggregateException(string message, Exception[] inners)
}
```

成员：[`InnerExceptions`](#Std.AggregateException.InnerExceptions) · [`AggregateException`](#Std.AggregateException.ctor(string,Exception[]))

<a id="Std.AggregateException.InnerExceptions"></a>
### Exception[] InnerExceptions

[`src/libraries/z42.core/src/Exceptions/AggregateException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/AggregateException.z42#L6)

聚合的所有异常。

<a id="Std.AggregateException.ctor(string,Exception[])"></a>
### AggregateException(string message, Exception[] inners)

[`src/libraries/z42.core/src/Exceptions/AggregateException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/AggregateException.z42#L9)

用消息和聚合的异常数组构造。

