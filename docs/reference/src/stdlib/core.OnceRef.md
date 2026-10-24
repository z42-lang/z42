<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OnceRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L43)

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

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L48)

用 handler 构造一次性包装。

<a id="Std.OnceRef<TD>.Get()"></a>
### TD Get()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L54)

返回被包装的委托。

<a id="Std.OnceRef<TD>.IsAlive()"></a>
### bool IsAlive()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L59)

是否尚未被消费。

<a id="Std.OnceRef<TD>.OnInvoked()"></a>
### void OnInvoked()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L64)

标记为已消费。

