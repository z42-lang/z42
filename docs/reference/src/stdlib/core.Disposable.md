<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Disposable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Disposable.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Disposable.z42#L7)

通用 `IDisposable` 实现 + `From(Action)` 工厂。

把「释放时调一段无参回调」的常见模式封装成稳定 API，避免每个调用方自定义
IDisposable 子类。幂等：多次 `Dispose()` 安全，仅第一次触发 `onDispose`，之后 noop。

```z42
public sealed class Disposable : IDisposable {
    Disposable(Action onDispose)
    static IDisposable From(Action onDispose)
    void Dispose()
}
```

成员：[`Disposable`](#Std.Disposable.ctor(Action)) · [`From`](#Std.Disposable.From(Action)) · [`Dispose`](#Std.Disposable.Dispose())

<a id="Std.Disposable.ctor(Action)"></a>
### Disposable(Action onDispose)

[`src/libraries/z42.core/src/Disposable.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Disposable.z42#L12)

以释放回调构造。

<a id="Std.Disposable.From(Action)"></a>
### static IDisposable From(Action onDispose)

[`src/libraries/z42.core/src/Disposable.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Disposable.z42#L20)

工厂：`Disposable.From(() => ...)`。

比直接 `new Disposable(...)` 更贴近「创建一个一次性 dispose 句柄」的意图。

<a id="Std.Disposable.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Disposable.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Disposable.z42#L25)

触发 `onDispose`（仅第一次；幂等）。

