<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SynchronizationLockException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

`src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42:5`

调用方要求持有锁、却没有持有（非持有者 `Exit` / `Wait`）。镜像 .NET `System.Threading.SynchronizationLockException`。

```z42
public class SynchronizationLockException : Exception {
    SynchronizationLockException(string message)
}
```

### SynchronizationLockException(string message)

`src/libraries/z42.core/src/Exceptions/SynchronizationLockException.z42:7`

用消息构造。

