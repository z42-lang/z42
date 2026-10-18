<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastSubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:266`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L266)

内部订阅 token —— Subscribe 返回它，Dispose 把对应通道的 alive[idx] 置 false。

```z42
public sealed class MulticastSubscription<T> : IDisposable {
    MulticastSubscription(MulticastAction<T> bus, int idx, bool isAdvanced)
    void Dispose()
}
```

成员：[`MulticastSubscription`](#Std.MulticastSubscription%3CT%3E.ctor(MulticastAction%3CT%3E,int,bool)) · [`Dispose`](#Std.MulticastSubscription%3CT%3E.Dispose())

<a id="Std.MulticastSubscription<T>.ctor(MulticastAction<T>,int,bool)"></a>
### MulticastSubscription(MulticastAction<T> bus, int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:272`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L272)

构造订阅 token。

<a id="Std.MulticastSubscription<T>.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:279`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L279)

取消订阅：把对应通道的 alive[idx] 置 false。

