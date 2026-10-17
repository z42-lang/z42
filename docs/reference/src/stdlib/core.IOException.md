<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IOException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/Exceptions/IOException.z42:4`

I/O 出错的基类。镜像 .NET `System.IO.IOException`。

```z42
public class IOException : Exception {
    IOException(string message)
    IOException(string message, Exception inner)
}
```

### IOException(string message)

`src/libraries/z42.core/src/Exceptions/IOException.z42:6`

用消息构造。

### IOException(string message, Exception inner)

`src/libraries/z42.core/src/Exceptions/IOException.z42:9`

用消息与底层原因构造（`NetworkStream` 把 `SocketException` 包成 IOException，同 .NET）。

