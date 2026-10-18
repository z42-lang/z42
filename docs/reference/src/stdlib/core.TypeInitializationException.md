<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TypeInitializationException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/TypeInitializationException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/TypeInitializationException.z42#L7)

类型的静态构造器抛出了异常，对标 C# `System.TypeInitializationException`。

静态构造器一旦抛出，该类型此后不可用——后续任何触发初始化的访问（读写其静态字段、创建实例、
调用其静态方法）都直接抛本异常，不会重试。

```z42
public class TypeInitializationException : Exception {
    TypeInitializationException(string message)
}
```

成员：[`TypeInitializationException`](#Std.TypeInitializationException.ctor(string))

<a id="Std.TypeInitializationException.ctor(string)"></a>
### TypeInitializationException(string message)

[`src/libraries/z42.core/src/Exceptions/TypeInitializationException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/TypeInitializationException.z42#L9)

用消息构造。

