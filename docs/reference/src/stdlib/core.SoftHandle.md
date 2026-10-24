<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SoftHandle

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/GC/SoftHandle.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/SoftHandle.z42#L8)

可被 GC 清除的引用（比 weak 强一档）。

堆压力低于阈值（`Z42_GC_SOFT_THRESHOLD`，默认 80%）时保活，越过阈值后 GC 可
清除。没设堆上限时永不被清除。原子值（int / string / bool 等）不可软引用——
`Create()` 返回的句柄 `Get()` 恒为 null。

```z42
public class SoftHandle {
    static extern SoftHandle Create(object target)
    extern object Get()
}
```

成员：[`Create`](#Std.SoftHandle.Create(object)) · [`Get`](#Std.SoftHandle.Get())

<a id="Std.SoftHandle.Create(object)"></a>
### static extern SoftHandle Create(object target)

[`src/libraries/z42.core/src/GC/SoftHandle.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/SoftHandle.z42#L13)

创建软引用句柄；从不返回 null（失败体现为 `Get()` 恒空）。

<a id="Std.SoftHandle.Get()"></a>
### extern object Get()

[`src/libraries/z42.core/src/GC/SoftHandle.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/SoftHandle.z42#L17)

取目标对象；已被清除时 null。

