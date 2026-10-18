<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ArithmeticException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/ArithmeticException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ArithmeticException.z42#L4)

算术运算出错（溢出、除以 0）的基类。镜像 .NET `System.ArithmeticException`。

```z42
public class ArithmeticException : Exception {
    ArithmeticException(string message)
}
```

成员：[`ArithmeticException`](#Std.ArithmeticException.ctor(string))

<a id="Std.ArithmeticException.ctor(string)"></a>
### ArithmeticException(string message)

[`src/libraries/z42.core/src/Exceptions/ArithmeticException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ArithmeticException.z42#L6)

用消息构造。

