<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastFuncSubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:284`

内部订阅 token —— Subscribe 返回它，Dispose 把 alive[idx] 置 false。

```z42
public sealed class MulticastFuncSubscription<TArg,TResult> : IDisposable {
    MulticastFuncSubscription(MulticastFunc<TArg,TResult> bus, int idx, bool isAdvanced)
    void Dispose()
}
```

### MulticastFuncSubscription(MulticastFunc<TArg,TResult> bus, int idx, bool isAdvanced)

`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:290`

构造订阅 token。

### void Dispose()

`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:297`

取消订阅：把对应通道的 alive[idx] 置 false。

