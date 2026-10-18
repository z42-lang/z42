<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OverflowException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/OverflowException.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/OverflowException.z42#L5)

数值运算结果超出目标类型容量（如 `BigInt.ToInt32()` 超出 i32、`Int32.Parse` 数字过大、 checked arithmetic 溢出）。镜像 .NET BCL `System.OverflowException`。

```z42
public class OverflowException : ArithmeticException {
    OverflowException(string message)
}
```

成员：[`OverflowException`](#Std.OverflowException.ctor(string))

<a id="Std.OverflowException.ctor(string)"></a>
### OverflowException(string message)

[`src/libraries/z42.core/src/Exceptions/OverflowException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/OverflowException.z42#L7)

用消息构造。

