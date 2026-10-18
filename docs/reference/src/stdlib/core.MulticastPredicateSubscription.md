<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastPredicateSubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:365`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L365)

内部订阅 token —— Subscribe 返回它，Dispose 把 alive[idx] 置 false。

```z42
public sealed class MulticastPredicateSubscription<T> : IDisposable {
    MulticastPredicateSubscription(MulticastPredicate<T> bus, int idx, bool isAdvanced)
    void Dispose()
}
```

成员：[`MulticastPredicateSubscription`](#Std.MulticastPredicateSubscription%3CT%3E.ctor(MulticastPredicate%3CT%3E,int,bool)) · [`Dispose`](#Std.MulticastPredicateSubscription%3CT%3E.Dispose())

<a id="Std.MulticastPredicateSubscription<T>.ctor(MulticastPredicate<T>,int,bool)"></a>
### MulticastPredicateSubscription(MulticastPredicate<T> bus, int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:371`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L371)

构造订阅 token。

<a id="Std.MulticastPredicateSubscription<T>.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:378`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L378)

取消订阅：把对应通道的 alive[idx] 置 false。

