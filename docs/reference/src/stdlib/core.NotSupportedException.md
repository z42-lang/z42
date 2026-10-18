<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.NotSupportedException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/NotSupportedException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/NotSupportedException.z42#L4)

方法不支持当前场景（如对只读集合调 Add）。

```z42
public class NotSupportedException : Exception {
    NotSupportedException(string message)
}
```

成员：[`NotSupportedException`](#Std.NotSupportedException.ctor(string))

<a id="Std.NotSupportedException.ctor(string)"></a>
### NotSupportedException(string message)

[`src/libraries/z42.core/src/Exceptions/NotSupportedException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/NotSupportedException.z42#L6)

用消息构造。

