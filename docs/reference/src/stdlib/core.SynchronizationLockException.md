<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SynchronizationLockException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

[`src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42#L6)

调用方要求持有锁、却没有持有（非持有者 `Exit` / `Wait`）。

镜像 .NET `System.Threading.SynchronizationLockException`。

```z42
public class SynchronizationLockException : Exception {
    SynchronizationLockException(string message)
}
```

成员：[`SynchronizationLockException`](#Std.Threading.SynchronizationLockException.ctor(string))

<a id="Std.Threading.SynchronizationLockException.ctor(string)"></a>
### SynchronizationLockException(string message)

[`src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42#L8)

用消息构造。

