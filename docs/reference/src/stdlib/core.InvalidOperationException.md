<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.InvalidOperationException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/InvalidOperationException.z42:4`

对象当前状态不允许此操作（如对空 Queue 调 Dequeue、对已 Dispose 对象操作）。

```z42
public class InvalidOperationException : Exception {
    InvalidOperationException(string message)
}
```

### InvalidOperationException(string message)

`src/libraries/z42.core/src/Exceptions/InvalidOperationException.z42:6`

用消息构造。

