<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DivideByZeroException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/DivideByZeroException.z42:5`

整数除法 / 取模的除数为 0。镜像 .NET BCL `System.DivideByZeroException`。 浮点除以 0 不抛此异常——IEEE 754 语义返回 ±∞ 或 NaN。

```z42
public class DivideByZeroException : ArithmeticException {
    DivideByZeroException(string message)
}
```

### DivideByZeroException(string message)

`src/libraries/z42.core/src/Exceptions/DivideByZeroException.z42:7`

用消息构造。

