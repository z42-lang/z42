<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastFuncSubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:287`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L287)

内部订阅 token —— Subscribe 返回它，Dispose 把 alive[idx] 置 false。

```z42
public sealed class MulticastFuncSubscription<TArg,TResult> : IDisposable {
    MulticastFuncSubscription(MulticastFunc<TArg,TResult> bus, int idx, bool isAdvanced)
    void Dispose()
}
```

成员：[`MulticastFuncSubscription`](#Std.MulticastFuncSubscription%3CTArg,TResult%3E.ctor(MulticastFunc%3CTArg,TResult%3E,int,bool)) · [`Dispose`](#Std.MulticastFuncSubscription%3CTArg,TResult%3E.Dispose())

<a id="Std.MulticastFuncSubscription<TArg,TResult>.ctor(MulticastFunc<TArg,TResult>,int,bool)"></a>
### MulticastFuncSubscription(MulticastFunc<TArg,TResult> bus, int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:293`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L293)

构造订阅 token。

<a id="Std.MulticastFuncSubscription<TArg,TResult>.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:300`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L300)

取消订阅：把对应通道的 alive[idx] 置 false。

