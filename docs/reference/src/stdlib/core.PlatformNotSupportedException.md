<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.PlatformNotSupportedException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/PlatformNotSupportedException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/PlatformNotSupportedException.z42#L6)

功能在当前平台上不可用（如 wasm32 上的 socket）。

镜像 .NET `System.PlatformNotSupportedException : NotSupportedException`。

```z42
public class PlatformNotSupportedException : NotSupportedException {
    PlatformNotSupportedException(string message)
}
```

成员：[`PlatformNotSupportedException`](#Std.PlatformNotSupportedException.ctor(string))

<a id="Std.PlatformNotSupportedException.ctor(string)"></a>
### PlatformNotSupportedException(string message)

[`src/libraries/z42.core/src/Exceptions/PlatformNotSupportedException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/PlatformNotSupportedException.z42#L8)

用消息构造。

