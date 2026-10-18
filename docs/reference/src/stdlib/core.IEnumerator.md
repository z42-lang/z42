<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IEnumerator

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Protocols/IEnumerator.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEnumerator.z42#L13)

前向迭代器契约（C# 风格）。

典型用法：
    while (it.MoveNext()) {
        var x = it.Current;
        ...
    }
    it.Dispose();

继承 IDisposable，使 foreach 升级路径可统一调 Dispose 释放迭代状态。

```z42
public interface IEnumerator<T> : IDisposable {
    bool MoveNext()
    T Current { get; }
}
```

成员：[`MoveNext`](#Std.IEnumerator%3CT%3E.MoveNext()) · [`Current`](#Std.IEnumerator%3CT%3E.Current)

<a id="Std.IEnumerator<T>.MoveNext()"></a>
### bool MoveNext()

[`src/libraries/z42.core/src/Protocols/IEnumerator.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEnumerator.z42#L15)

推进到下一元素；成功返回 true，越过末尾返回 false。

<a id="Std.IEnumerator<T>.Current"></a>
### T Current { get; }

[`src/libraries/z42.core/src/Protocols/IEnumerator.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEnumerator.z42#L17)

当前元素；仅在最近一次 MoveNext 返回 true 后有效。

