<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IOException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/Exceptions/IOException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/IOException.z42#L4)

I/O 出错的基类。镜像 .NET `System.IO.IOException`。

```z42
public class IOException : Exception {
    IOException(string message)
    IOException(string message, Exception inner)
}
```

成员：[`IOException`](#Std.IO.IOException.ctor(string)) · [`IOException`](#Std.IO.IOException.ctor(string,Exception))

<a id="Std.IO.IOException.ctor(string)"></a>
### IOException(string message)

[`src/libraries/z42.core/src/Exceptions/IOException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/IOException.z42#L6)

用消息构造。

<a id="Std.IO.IOException.ctor(string,Exception)"></a>
### IOException(string message, Exception inner)

[`src/libraries/z42.core/src/Exceptions/IOException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/IOException.z42#L9)

用消息与底层原因构造（`NetworkStream` 把 `SocketException` 包成 IOException，同 .NET）。

