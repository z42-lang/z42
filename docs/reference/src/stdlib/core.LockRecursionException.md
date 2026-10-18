<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.LockRecursionException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

[`src/libraries/z42.core/src/Exceptions/LockRecursionException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/LockRecursionException.z42#L6)

不可重入的锁被同一线程再次获取。镜像 .NET `System.Threading.LockRecursionException`。

z42 的 Monitor 不可重入：同线程再次 `Enter` 抛本异常，而不是死锁。

```z42
public class LockRecursionException : Exception {
    LockRecursionException(string message)
}
```

成员：[`LockRecursionException`](#Std.Threading.LockRecursionException.ctor(string))

<a id="Std.Threading.LockRecursionException.ctor(string)"></a>
### LockRecursionException(string message)

[`src/libraries/z42.core/src/Exceptions/LockRecursionException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Exceptions/LockRecursionException.z42#L8)

用消息构造。

