<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.WeakRef

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:150`

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

### WeakRef(TD h)

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:157`

用 handler 构造弱引用包装。

### TD Get()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:173`

target 存活时重建 Closure 返回；否则返回 null（退化时返回 degradedHandler）。

### bool IsAlive()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:184`

是否仍活跃（退化时恒 true）。

### void OnInvoked()

`src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42:191`

无操作（weak 不维护本地状态）。

