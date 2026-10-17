<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.StrongRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:16`

强引用包装 —— 默认无状态。Subscribe(handler) 对应到内部 strong 通道走快路径； 用户显式构造 StrongRef 走 advanced 通道也合法。

```z42
public sealed class StrongRef<TD> : ISubscription<TD> {
    StrongRef(TD h)
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

### StrongRef(TD h)

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:20`

用 handler 构造强引用包装。

### TD Get()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:25`

返回被包装的委托。

### bool IsAlive()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:30`

恒为 true。

### void OnInvoked()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:35`

无操作（无状态包装器）。

