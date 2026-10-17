<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastSubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/MulticastAction.z42:266`

内部订阅 token —— Subscribe 返回它，Dispose 把对应通道的 alive[idx] 置 false。

```z42
public sealed class MulticastSubscription<T> : IDisposable {
    MulticastSubscription(MulticastAction<T> bus, int idx, bool isAdvanced)
    void Dispose()
}
```

### MulticastSubscription(MulticastAction<T> bus, int idx, bool isAdvanced)

`src/libraries/z42.core/src/Delegates/MulticastAction.z42:272`

构造订阅 token。

### void Dispose()

`src/libraries/z42.core/src/Delegates/MulticastAction.z42:279`

取消订阅：把对应通道的 alive[idx] 置 false。

