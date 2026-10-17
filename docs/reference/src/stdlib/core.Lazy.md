<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Lazy

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Lazy.z42:7`

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

### Lazy(Func<T> factory)

`src/libraries/z42.core/src/Lazy.z42:13`

以工厂构造。

### bool IsValueCreated { get; }

`src/libraries/z42.core/src/Lazy.z42:19`

值是否已被物化。

### T Value { get; }

`src/libraries/z42.core/src/Lazy.z42:24`

延迟计算的值。首次读调用工厂并缓存，之后返回缓存。

