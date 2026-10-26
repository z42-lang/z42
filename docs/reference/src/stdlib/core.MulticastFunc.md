<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastFunc

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L14)

多播 Func：一次注册多个 `Func<TArg, TResult>` handler。

Invoke 收集所有返回值进 `TResult[]`（FIFO，注册顺序 = 调用顺序）。与单播
`Func<TArg, TResult>` 分离。

双 vec 通道：
  - strong 通道 —— 直接持 Func<TArg, TResult>，裸 handler `Subscribe(...)` 走此路径。
  - advanced 通道 —— 持 ISubscription<Func<TArg, TResult>> 包装器（OnceRef / CompositeRef /
    自定义），invoke 慢路径走 IsAlive + Get + OnInvoked。

Invoke 时拷贝当前数组遍历副本（COW snapshot）。

```z42
public sealed class MulticastFunc<TArg,TResult> {
    MulticastFunc()
    IDisposable Subscribe(Func<TArg,TResult> handler)
    IDisposable SubscribeAdvanced(ISubscription<Func<TArg,TResult>> wrapper)
    int Count()
    TResult[] Invoke(TArg arg, bool continueOnException = false)
    void Unsubscribe(Func<TArg,TResult> handler)
    void RemoveAt(int idx, bool isAdvanced)
}
```

成员：[`MulticastFunc`](#Std.MulticastFunc%3CTArg,TResult%3E.ctor()) · [`Subscribe`](#Std.MulticastFunc%3CTArg,TResult%3E.Subscribe(Func%3CTArg,TResult%3E)) · [`SubscribeAdvanced`](#Std.MulticastFunc%3CTArg,TResult%3E.SubscribeAdvanced(ISubscription%3CFunc%3CTArg,TResult%3E%3E)) · [`Count`](#Std.MulticastFunc%3CTArg,TResult%3E.Count()) · [`Invoke`](#Std.MulticastFunc%3CTArg,TResult%3E.Invoke(TArg,bool)) · [`Unsubscribe`](#Std.MulticastFunc%3CTArg,TResult%3E.Unsubscribe(Func%3CTArg,TResult%3E)) · [`RemoveAt`](#Std.MulticastFunc%3CTArg,TResult%3E.RemoveAt(int,bool))

<a id="Std.MulticastFunc<TArg,TResult>.ctor()"></a>
### MulticastFunc()

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L28)

构造空的多播 Func。

<a id="Std.MulticastFunc<TArg,TResult>.Subscribe(Func<TArg,TResult>)"></a>
### IDisposable Subscribe(Func<TArg,TResult> handler)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L41)

裸 handler 订阅 → strong 快路径（零包装开销）。

<a id="Std.MulticastFunc<TArg,TResult>.SubscribeAdvanced(ISubscription<Func<TArg,TResult>>)"></a>
### IDisposable SubscribeAdvanced(ISubscription<Func<TArg,TResult>> wrapper)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L51)

ISubscription 包装器订阅 → advanced 慢路径（OnceRef / CompositeRef / 用户自定义包装器）。

<a id="Std.MulticastFunc<TArg,TResult>.Count()"></a>
### int Count()

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L61)

当前活跃订阅总数（strong + advanced）。

<a id="Std.MulticastFunc<TArg,TResult>.Invoke(TArg,bool)"></a>
### TResult[] Invoke(TArg arg, bool continueOnException = false)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L82)

触发所有活跃 handler，收集返回值进 TResult[]（strong-first / advanced-after 顺序）。

`continueOnException = false`（默认）走 fail-fast：首个抛出的异常直接传播。
`continueOnException = true` 时失败位置占 default(TResult)，多失败聚合抛
MulticastException<TResult>（携带 Results 与失败索引 / 异常并行数组）。

<a id="Std.MulticastFunc<TArg,TResult>.Unsubscribe(Func<TArg,TResult>)"></a>
### void Unsubscribe(Func<TArg,TResult> handler)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:230`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L230)

按 handler 取消订阅（仅扫 strong 通道；advanced 通道用 token-based dispose）。

<a id="Std.MulticastFunc<TArg,TResult>.RemoveAt(int,bool)"></a>
### void RemoveAt(int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastFunc.z42:243`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastFunc.z42#L243)

内部：dispose token 入口，把对应通道的 alive[idx] 置 false。

