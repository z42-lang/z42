<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.CompositeRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L73)

组合包装 —— mode flags 累加；融合后只分配一个 CompositeRef 实例。

Mode.Weak 时 CompositeRef 不强持 handler，而是存 (WeakHandle, fnName)，Get 时重建
Closure；否则 weak 会失效。strong 路径继续直接持 handler 走快路径。

```z42
public sealed class CompositeRef<TD> : ISubscription<TD> {
    int Modes
    CompositeRef(TD h, int modes)
    CompositeRef<TD> WithMode(int additional)
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

成员：[`Modes`](#Std.CompositeRef%3CTD%3E.Modes) · [`CompositeRef`](#Std.CompositeRef%3CTD%3E.ctor(TD,int)) · [`WithMode`](#Std.CompositeRef%3CTD%3E.WithMode(int)) · [`Get`](#Std.CompositeRef%3CTD%3E.Get()) · [`IsAlive`](#Std.CompositeRef%3CTD%3E.IsAlive()) · [`OnInvoked`](#Std.CompositeRef%3CTD%3E.OnInvoked())

<a id="Std.CompositeRef<TD>.Modes"></a>
### int Modes

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L76)

组合的 mode flags 位掩码（`ModeFlags` 各值的按位或）。

<a id="Std.CompositeRef<TD>.ctor(TD,int)"></a>
### CompositeRef(TD h, int modes)

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L83)

用 handler 与 mode flags 构造组合包装。

<a id="Std.CompositeRef<TD>.WithMode(int)"></a>
### CompositeRef<TD> WithMode(int additional)

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L109)

累加 mode flag 复用本实例，链式 .AsOnce().AsWeak() 不重复分配（融合）。

<a id="Std.CompositeRef<TD>.Get()"></a>
### TD Get()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L115)

返回 handler；Weak 活跃时经 weakHandle 重建 Closure。

<a id="Std.CompositeRef<TD>.IsAlive()"></a>
### bool IsAlive()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L129)

是否仍活跃（考虑 consumed 与 weak target）。

<a id="Std.CompositeRef<TD>.OnInvoked()"></a>
### void OnInvoked()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L139)

Once flag 时标记为已消费。

