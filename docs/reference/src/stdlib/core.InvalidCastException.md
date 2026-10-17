<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.InvalidCastException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/InvalidCastException.z42:6`

硬转换 `(T)x` 失败：`x` 非 null 但不是 `T`（也不是 `T` 的子类型 / 未实现 `T`）。

与 `as` 的分工：`x as T` 失配返回 null、不抛；`(T)x` 断言「我知道它是 T」，断言不成立就抛。

```z42
public class InvalidCastException : Exception {
    InvalidCastException(string message)
}
```

### InvalidCastException(string message)

`src/libraries/z42.core/src/Exceptions/InvalidCastException.z42:8`

用消息构造。

