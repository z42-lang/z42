<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ISubscription

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/ISubscription.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/ISubscription.z42#L5)

多播订阅策略包装器协议：`MulticastAction<T>.Subscribe(ISubscription<Action<T>>)` 走 advanced 路径；用户可实现自己的包装器加入新策略（throttle / debounce / dispatcher 等）。

```z42
public interface ISubscription<TD> {
    TD Get()
    bool IsAlive()
    void OnInvoked()
}
```

成员：[`Get`](#Std.ISubscription%3CTD%3E.Get()) · [`IsAlive`](#Std.ISubscription%3CTD%3E.IsAlive()) · [`OnInvoked`](#Std.ISubscription%3CTD%3E.OnInvoked())

<a id="Std.ISubscription<TD>.Get()"></a>
### TD Get()

[`src/libraries/z42.core/src/Delegates/ISubscription.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/ISubscription.z42#L7)

返回被包装的委托；仅在 IsAlive() 为 true 时有定义，调用方先检查 IsAlive。

<a id="Std.ISubscription<TD>.IsAlive()"></a>
### bool IsAlive()

[`src/libraries/z42.core/src/Delegates/ISubscription.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/ISubscription.z42#L9)

是否仍活跃；invoke 循环据此短路。

<a id="Std.ISubscription<TD>.OnInvoked()"></a>
### void OnInvoked()

[`src/libraries/z42.core/src/Delegates/ISubscription.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/ISubscription.z42#L11)

每次 invoke 后调用；包装器用它更新自身 latch 状态（无状态包装器为 nop）。

