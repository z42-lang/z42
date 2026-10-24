<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.WeakHandle

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/GC/WeakHandle.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/WeakHandle.z42#L7)

不透明弱引用句柄。GC 不持目标的强引用。

所有强引用 dropped 后目标可被回收，此时 `Upgrade()` 返回 null。原子值
（int / string / bool / 委托 / null 等）不可弱化——`MakeWeak` 返回 null。

```z42
public class WeakHandle {
    static extern WeakHandle MakeWeak(object target)
    static extern object? Upgrade(WeakHandle handle)
}
```

成员：[`MakeWeak`](#Std.WeakHandle.MakeWeak(object)) · [`Upgrade`](#Std.WeakHandle.Upgrade(WeakHandle))

<a id="Std.WeakHandle.MakeWeak(object)"></a>
### static extern WeakHandle MakeWeak(object target)

[`src/libraries/z42.core/src/GC/WeakHandle.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/WeakHandle.z42#L10)

创建弱引用句柄；原子值 / null 返回 null。

<a id="Std.WeakHandle.Upgrade(WeakHandle)"></a>
### static extern object? Upgrade(WeakHandle handle)

[`src/libraries/z42.core/src/GC/WeakHandle.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/WeakHandle.z42#L14)

取目标对象；已被回收或 handle 为 null 时返回 null。

