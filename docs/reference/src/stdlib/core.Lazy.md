<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Lazy

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Lazy.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Lazy.z42#L7)

延迟的一次性初始化容器（C# System.Lazy<T> 对标）。

包装 `Func<T>` 工厂；首次读 `Value` 才运行工厂并缓存，之后返回缓存值。仅单线程
memoize（对应 C# LazyThreadSafetyMode.None）。

```z42
public sealed class Lazy<T> {
    Lazy(Func<T> factory)
    bool IsValueCreated { get; }
    T Value { get; }
}
```

成员：[`Lazy`](#Std.Lazy%3CT%3E.ctor(Func%3CT%3E)) · [`IsValueCreated`](#Std.Lazy%3CT%3E.IsValueCreated) · [`Value`](#Std.Lazy%3CT%3E.Value)

<a id="Std.Lazy<T>.ctor(Func<T>)"></a>
### Lazy(Func<T> factory)

[`src/libraries/z42.core/src/Lazy.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Lazy.z42#L13)

以工厂构造。

<a id="Std.Lazy<T>.IsValueCreated"></a>
### bool IsValueCreated { get; }

[`src/libraries/z42.core/src/Lazy.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Lazy.z42#L19)

值是否已被物化。

<a id="Std.Lazy<T>.Value"></a>
### T Value { get; }

[`src/libraries/z42.core/src/Lazy.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Lazy.z42#L24)

延迟计算的值。首次读调用工厂并缓存，之后返回缓存。

