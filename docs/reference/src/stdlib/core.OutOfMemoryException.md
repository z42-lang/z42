<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OutOfMemoryException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/OutOfMemoryException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/OutOfMemoryException.z42#L4)

堆内存不足（strict OOM 模式下 alloc 超过 max_heap_bytes）。

```z42
public class OutOfMemoryException : Exception {
    OutOfMemoryException(string message)
}
```

成员：[`OutOfMemoryException`](#Std.OutOfMemoryException.ctor(string))

<a id="Std.OutOfMemoryException.ctor(string)"></a>
### OutOfMemoryException(string message)

[`src/libraries/z42.core/src/Exceptions/OutOfMemoryException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/OutOfMemoryException.z42#L6)

用消息构造。

