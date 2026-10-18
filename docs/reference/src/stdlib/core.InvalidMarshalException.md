<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.InvalidMarshalException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/InvalidMarshalException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/InvalidMarshalException.z42#L7)

z42 值无法 marshal 为 native ABI 类型时抛出（contract violation）。

常见原因：z42 string 含 interior NUL 作 `*const c_char` 传递；PinnedView 字段访问超出
{ptr, len}；`pinned p = expr { ... }` 的源类型不支持（当前仅 string / Array<u8>）。

```z42
public class InvalidMarshalException : Exception {
    InvalidMarshalException(string message)
}
```

成员：[`InvalidMarshalException`](#Std.InvalidMarshalException.ctor(string))

<a id="Std.InvalidMarshalException.ctor(string)"></a>
### InvalidMarshalException(string message)

[`src/libraries/z42.core/src/Exceptions/InvalidMarshalException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/InvalidMarshalException.z42#L9)

用消息构造。

