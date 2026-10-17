<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ISubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Delegates/ISubscription.z42:5`

多播订阅策略包装器协议：`MulticastAction<T>.Subscribe(ISubscription<Action<T>>)` 走 advanced 路径；用户可实现自己的包装器加入新策略（throttle / debounce / dispatcher 等）。

```z42
public interface ISubscription<TD> {
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

### TD Get()

`src/libraries/z42.core/src/Delegates/ISubscription.z42:7`

返回被包装的委托；仅在 IsAlive() 为 true 时有定义，调用方先检查 IsAlive。

### bool IsAlive()

`src/libraries/z42.core/src/Delegates/ISubscription.z42:9`

是否仍活跃；invoke 循环据此短路。

### void OnInvoked()

`src/libraries/z42.core/src/Delegates/ISubscription.z42:11`

每次 invoke 后调用；包装器用它更新自身 latch 状态（无状态包装器为 nop）。

