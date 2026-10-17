<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.AggregateException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/AggregateException.z42:4`

多个异常聚合的基类，携带 `InnerExceptions` 数组，便于通用 catch 处理批处理失败场景。

```z42
public class AggregateException : Exception {
    Exception[] InnerExceptions
    AggregateException(string message, Exception[] inners)
}
```

### Exception[] InnerExceptions

`src/libraries/z42.core/src/Exceptions/AggregateException.z42:6`

聚合的所有异常。

### AggregateException(string message, Exception[] inners)

`src/libraries/z42.core/src/Exceptions/AggregateException.z42:9`

用消息和聚合的异常数组构造。

