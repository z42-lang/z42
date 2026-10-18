<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ObjectDisposedException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42#L4)

对已释放（Dispose / Close）的对象执行操作。镜像 .NET `System.ObjectDisposedException`。

```z42
public class ObjectDisposedException : InvalidOperationException {
    string ObjectName
    ObjectDisposedException(string objectName)
    ObjectDisposedException(string objectName, string message)
}
```

成员：[`ObjectName`](#Std.ObjectDisposedException.ObjectName) · [`ObjectDisposedException`](#Std.ObjectDisposedException.ctor(string)) · [`ObjectDisposedException`](#Std.ObjectDisposedException.ctor(string,string))

<a id="Std.ObjectDisposedException.ObjectName"></a>
### string ObjectName

[`src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42#L6)

被释放对象的类型名（可为 null）。

<a id="Std.ObjectDisposedException.ctor(string)"></a>
### ObjectDisposedException(string objectName)

[`src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42#L9)

用对象名构造，消息与 .NET 同形（`Cannot access a disposed object.\nObject name: '<name>'.`）。

<a id="Std.ObjectDisposedException.ctor(string,string)"></a>
### ObjectDisposedException(string objectName, string message)

[`src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/ObjectDisposedException.z42#L15)

用对象名与自定义消息构造。

