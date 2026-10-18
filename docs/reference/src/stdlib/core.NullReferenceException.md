<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.NullReferenceException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/NullReferenceException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/NullReferenceException.z42#L4)

解引用 null（字段访问 / 方法调用 receiver 为 null）。

```z42
public class NullReferenceException : Exception {
    NullReferenceException(string message)
}
```

成员：[`NullReferenceException`](#Std.NullReferenceException.ctor(string))

<a id="Std.NullReferenceException.ctor(string)"></a>
### NullReferenceException(string message)

[`src/libraries/z42.core/src/Exceptions/NullReferenceException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/NullReferenceException.z42#L6)

用消息构造。

