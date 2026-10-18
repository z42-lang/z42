<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IDisposable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Protocols/IDisposable.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IDisposable.z42#L7)

资源释放契约：持有非托管 / 外部资源的类型实现 Dispose() 做确定性释放。

供 FileStream / RegexEngine 等 stdlib 类型使用，并作为 IEnumerator<T> 的基接口，
使 foreach 能释放迭代器状态。

```z42
public interface IDisposable {
    void Dispose()
}
```

成员：[`Dispose`](#Std.IDisposable.Dispose())

<a id="Std.IDisposable.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Protocols/IDisposable.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IDisposable.z42#L9)

释放资源；调用后对象不再可用。

