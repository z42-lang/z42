<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastPredicate

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L10)

多播 Predicate：一次注册多个 `Predicate<T>` handler。

Invoke 收集所有返回值进 `bool[]`（FIFO）。`All(T)` / `Any(T)` 短路求值。

双 vec 通道（同 MulticastAction / MulticastFunc）。Invoke 默认 fail-fast；
`continueOnException = true` 时失败位置占 default(bool) = false，多失败聚合抛
MulticastException<bool>。

```z42
public sealed class MulticastPredicate<T> {
    MulticastPredicate()
    IDisposable Subscribe(Predicate<T> handler)
    IDisposable SubscribeAdvanced(ISubscription<Predicate<T>> wrapper)
    int Count()
    bool[] Invoke(T arg, bool continueOnException = false)
    bool All(T arg)
    bool Any(T arg)
    void Unsubscribe(Predicate<T> handler)
    void RemoveAt(int idx, bool isAdvanced)
}
```

成员：[`MulticastPredicate`](#Std.MulticastPredicate%3CT%3E.ctor()) · [`Subscribe`](#Std.MulticastPredicate%3CT%3E.Subscribe(Predicate%3CT%3E)) · [`SubscribeAdvanced`](#Std.MulticastPredicate%3CT%3E.SubscribeAdvanced(ISubscription%3CPredicate%3CT%3E%3E)) · [`Count`](#Std.MulticastPredicate%3CT%3E.Count()) · [`Invoke`](#Std.MulticastPredicate%3CT%3E.Invoke(T,bool)) · [`All`](#Std.MulticastPredicate%3CT%3E.All(T)) · [`Any`](#Std.MulticastPredicate%3CT%3E.Any(T)) · [`Unsubscribe`](#Std.MulticastPredicate%3CT%3E.Unsubscribe(Predicate%3CT%3E)) · [`RemoveAt`](#Std.MulticastPredicate%3CT%3E.RemoveAt(int,bool))

<a id="Std.MulticastPredicate<T>.ctor()"></a>
### MulticastPredicate()

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L22)

构造空的多播 Predicate。

<a id="Std.MulticastPredicate<T>.Subscribe(Predicate<T>)"></a>
### IDisposable Subscribe(Predicate<T> handler)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L35)

裸 handler 订阅 → strong 快路径（零包装开销）。

<a id="Std.MulticastPredicate<T>.SubscribeAdvanced(ISubscription<Predicate<T>>)"></a>
### IDisposable SubscribeAdvanced(ISubscription<Predicate<T>> wrapper)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L45)

ISubscription 包装器订阅 → advanced 慢路径（OnceRef / CompositeRef / 用户自定义包装器）。

<a id="Std.MulticastPredicate<T>.Count()"></a>
### int Count()

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L55)

当前活跃订阅总数（strong + advanced）。

<a id="Std.MulticastPredicate<T>.Invoke(T,bool)"></a>
### bool[] Invoke(T arg, bool continueOnException = false)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L76)

触发所有活跃 handler，收集返回值进 bool[]（strong-first / advanced-after 顺序）。

`continueOnException = false`（默认）走 fail-fast：首个抛出的异常直接传播。
`continueOnException = true` 时失败位置占 default(bool) = false，多失败聚合抛
MulticastException<bool>。

<a id="Std.MulticastPredicate<T>.All(T)"></a>
### bool All(T arg)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:225`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L225)

所有活跃 handler 都返回 true 时返回 true。

一旦遇 false 立即返回 false（短路），后续 handler 不调用。空 bus 返回 true
（vacuous truth）。

<a id="Std.MulticastPredicate<T>.Any(T)"></a>
### bool Any(T arg)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:270`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L270)

任一活跃 handler 返回 true 时返回 true。

一旦遇 true 立即返回 true（短路）。空 bus 返回 false（vacuous existential）。

<a id="Std.MulticastPredicate<T>.Unsubscribe(Predicate<T>)"></a>
### void Unsubscribe(Predicate<T> handler)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:313`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L313)

按 handler 取消订阅（仅扫 strong 通道；advanced 通道用 token-based dispose）。

<a id="Std.MulticastPredicate<T>.RemoveAt(int,bool)"></a>
### void RemoveAt(int idx, bool isAdvanced)

[`src/libraries/z42.core/src/Delegates/MulticastPredicate.z42:326`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/MulticastPredicate.z42#L326)

内部：dispose token 入口，把对应通道的 alive[idx] 置 false。

