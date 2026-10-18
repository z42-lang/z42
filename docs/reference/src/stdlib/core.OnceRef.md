<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OnceRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L41)

一次性包装 —— 第一次 OnInvoked 后 IsAlive=false。

```z42
public sealed class OnceRef<TD> : ISubscription<TD> {
    OnceRef(TD h)
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

成员：[`OnceRef`](#Std.OnceRef%3CTD%3E.ctor(TD)) · [`Get`](#Std.OnceRef%3CTD%3E.Get()) · [`IsAlive`](#Std.OnceRef%3CTD%3E.IsAlive()) · [`OnInvoked`](#Std.OnceRef%3CTD%3E.OnInvoked())

<a id="Std.OnceRef<TD>.ctor(TD)"></a>
### OnceRef(TD h)

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L46)

用 handler 构造一次性包装。

<a id="Std.OnceRef<TD>.Get()"></a>
### TD Get()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L52)

返回被包装的委托。

<a id="Std.OnceRef<TD>.IsAlive()"></a>
### bool IsAlive()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L57)

是否尚未被消费。

<a id="Std.OnceRef<TD>.OnInvoked()"></a>
### void OnInvoked()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L62)

标记为已消费。

