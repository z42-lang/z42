<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastAction

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L12)

多播 Action：一次注册多个 `Action<T>` handler，Invoke 时全部触发（与单播 `Action<T>` 分离）。

双 vec 通道：
  - strong 通道 —— 直接持 Action<T>，裸 handler `Subscribe(Action<T>)` 走此路径，零包装开销。
  - advanced 通道 —— 持 ISubscription<Action<T>> 接口（含 OnceRef / CompositeRef / 用户自定义
    包装器），invoke 慢路径走 IsAlive + Get + OnInvoked。

Invoke 时拷贝当前数组遍历副本（COW snapshot）：触发期间 Subscribe / Dispose 影响下次触发，
不影响本次。

```z42
public sealed class MulticastAction<T> {
    MulticastAction()
    IDisposable Subscribe(Action<T> handler)
    IDisposable SubscribeAdvanced(ISubscription<Action<T>> wrapper)
    int Count()
    void Invoke(T arg, bool continueOnException = false)
    void Unsubscribe(Action<T> handler)
    void RemoveAt(int idx, bool isAdvanced)
}
```

成员：[`MulticastAction`](#Std.MulticastAction%3CT%3E.ctor()) · [`Subscribe`](#Std.MulticastAction%3CT%3E.Subscribe(Action%3CT%3E)) · [`SubscribeAdvanced`](#Std.MulticastAction%3CT%3E.SubscribeAdvanced(ISubscription%3CAction%3CT%3E%3E)) · [`Count`](#Std.MulticastAction%3CT%3E.Count()) · [`Invoke`](#Std.MulticastAction%3CT%3E.Invoke(T,bool)) · [`Unsubscribe`](#Std.MulticastAction%3CT%3E.Unsubscribe(Action%3CT%3E)) · [`RemoveAt`](#Std.MulticastAction%3CT%3E.RemoveAt(int,bool))

<a id="Std.MulticastAction<T>.ctor()"></a>
### MulticastAction()

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L28)

构造空的多播 Action。

<a id="Std.MulticastAction<T>.Subscribe(Action<T>)"></a>
### IDisposable Subscribe(Action<T> handler)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L41)

裸 handler 订阅 → strong 快路径（零包装开销）。

<a id="Std.MulticastAction<T>.SubscribeAdvanced(ISubscription<Action<T>>)"></a>
### IDisposable SubscribeAdvanced(ISubscription<Action<T>> wrapper)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L51)

ISubscription 包装器订阅 → advanced 慢路径（OnceRef / CompositeRef / 用户自定义包装器）。

<a id="Std.MulticastAction<T>.Count()"></a>
### int Count()

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L61)

当前活跃订阅总数（strong + advanced）。

<a id="Std.MulticastAction<T>.Invoke(T,bool)"></a>
### void Invoke(T arg, bool continueOnException = false)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L82)

触发所有活跃 handler。

`continueOnException = false`（默认）走 fail-fast：首个抛出的异常直接传播。
`continueOnException = true` 时所有 handler 均尝试调用，失败者收入 Failures 数组，
最终抛 MulticastException 聚合诊断。

<a id="Std.MulticastAction<T>.Unsubscribe(Action<T>)"></a>
### void Unsubscribe(Action<T> handler)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:210`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L210)

按 handler 取消订阅（仅扫 strong 通道；advanced 通道用 token-based dispose）。

线性扫描 strong[]，每个 alive 槽与 handler 比较，命中置 alive=false；不 break ——
同 handler 多次 Subscribe 一次 Unsubscribe 全清。未订阅 handler 是 no-op。

<a id="Std.MulticastAction<T>.RemoveAt(int,bool)"></a>
### void RemoveAt(int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastAction.z42:223`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastAction.z42#L223)

内部：dispose token 入口，把对应通道的 alive[idx] 置 false。

