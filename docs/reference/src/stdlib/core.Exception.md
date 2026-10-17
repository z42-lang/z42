<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Exception

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/Exception.z42:8`

所有异常类型的推荐基类。

携带消息（`Message`）、调用栈快照（`StackTrace`）与包裹的原始异常（`InnerException`）。
throw 语法仍接受任意值（`throw "str"` 合法），但 stdlib 抛出的与新代码推荐统一继承本类，
为 catch 过滤 / Inner 链 / stack trace 打基础。

```z42
public class Exception {
    string Message
    string StackTrace
    Exception InnerException
    Exception(string message)
    Exception(string message, Exception inner)
}
```

### string Message

`src/libraries/z42.core/src/Exceptions/Exception.z42:10`

异常消息。

### string StackTrace

`src/libraries/z42.core/src/Exceptions/Exception.z42:12`

调用栈快照（interp 模式下由 throw 自动填充）。

### Exception InnerException

`src/libraries/z42.core/src/Exceptions/Exception.z42:14`

包裹的原始异常（无则 null）。

### Exception(string message)

`src/libraries/z42.core/src/Exceptions/Exception.z42:17`

用消息构造；StackTrace / InnerException 默认为 null。

### Exception(string message, Exception inner)

`src/libraries/z42.core/src/Exceptions/Exception.z42:23`

用消息与包裹的原始异常构造（wrapping pattern）。

