<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OnceRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:41`

一次性包装 —— 第一次 OnInvoked 后 IsAlive=false。

```z42
public sealed class OnceRef<TD> : ISubscription<TD> {
    OnceRef(TD h)
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

### OnceRef(TD h)

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:46`

用 handler 构造一次性包装。

### TD Get()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:52`

返回被包装的委托。

### bool IsAlive()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:57`

是否尚未被消费。

### void OnInvoked()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:62`

标记为已消费。

