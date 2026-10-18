<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.WeakRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L150)

弱引用包装。

WeakRef 不强持原 handler（那会反向锁住 receiver，因为 Closure 自己强持 env=[receiver]，
weak 无效）。相反，构造时拆解 handler 为 WeakHandle + fnName；target 不可弱化时退化 strong。
Get 时若 target 还活：用 MakeClosure(fnName, [target]) 重建新 Closure；若 target 已死：
返回 null，IsAlive=false。

```z42
public sealed class WeakRef<TD> : ISubscription<TD> {
    WeakRef(TD h)
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

成员：[`WeakRef`](#Std.WeakRef%3CTD%3E.ctor(TD)) · [`Get`](#Std.WeakRef%3CTD%3E.Get()) · [`IsAlive`](#Std.WeakRef%3CTD%3E.IsAlive()) · [`OnInvoked`](#Std.WeakRef%3CTD%3E.OnInvoked())

<a id="Std.WeakRef<TD>.ctor(TD)"></a>
### WeakRef(TD h)

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:157`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L157)

用 handler 构造弱引用包装。

<a id="Std.WeakRef<TD>.Get()"></a>
### TD Get()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:173`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L173)

target 存活时重建 Closure 返回；否则返回 null（退化时返回 degradedHandler）。

<a id="Std.WeakRef<TD>.IsAlive()"></a>
### bool IsAlive()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:184`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L184)

是否仍活跃（退化时恒 true）。

<a id="Std.WeakRef<TD>.OnInvoked()"></a>
### void OnInvoked()

[`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:191`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42#L191)

无操作（weak 不维护本地状态）。

