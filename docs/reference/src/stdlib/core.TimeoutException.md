<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TimeoutException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/TimeoutException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/TimeoutException.z42#L4)

操作在给定时限内没有完成。镜像 .NET `System.TimeoutException`。

```z42
public class TimeoutException : Exception {
    TimeoutException(string message)
}
```

成员：[`TimeoutException`](#Std.TimeoutException.ctor(string))

<a id="Std.TimeoutException.ctor(string)"></a>
### TimeoutException(string message)

[`src/libraries/z42.core/src/Exceptions/TimeoutException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/TimeoutException.z42#L6)

用消息构造。

