<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.GCHandle

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/GC/GCHandle.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L20)

统一 GC 句柄，对齐 C# `System.Runtime.InteropServices.GCHandle`。

与 `Std.WeakHandle`（轻量、自动回收、无 Free）的区别：GCHandle 双模
（Strong / Weak）+ 显式 Free。原子值（int / string / bool / 委托 / null 等）只能
`AllocStrong`（slot 持值拷贝）；`AllocWeak` 得到未分配句柄（`IsAllocated` =
false，原子值不可弱化）。

```z42
public struct GCHandle {
    static extern GCHandle Alloc(object target, GCHandleType type)
    static GCHandle AllocWeak(object target)
    static GCHandle AllocStrong(object target)
    extern object Target { get; }
    extern bool IsAllocated { get; }
    extern GCHandleType Kind { get; }
    extern void Free()
}
```

成员：[`Alloc`](#Std.GCHandle.Alloc(object,GCHandleType)) · [`AllocWeak`](#Std.GCHandle.AllocWeak(object)) · [`AllocStrong`](#Std.GCHandle.AllocStrong(object)) · [`Target`](#Std.GCHandle.Target) · [`IsAllocated`](#Std.GCHandle.IsAllocated) · [`Kind`](#Std.GCHandle.Kind) · [`Free`](#Std.GCHandle.Free())

<a id="Std.GCHandle.Alloc(object,GCHandleType)"></a>
### static extern GCHandle Alloc(object target, GCHandleType type)

[`src/libraries/z42.core/src/GC/GCHandle.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L25)

占一个槽位并返回句柄。

<a id="Std.GCHandle.AllocWeak(object)"></a>
### static GCHandle AllocWeak(object target)

[`src/libraries/z42.core/src/GC/GCHandle.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L28)

分配弱引用句柄。

<a id="Std.GCHandle.AllocStrong(object)"></a>
### static GCHandle AllocStrong(object target)

[`src/libraries/z42.core/src/GC/GCHandle.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L33)

分配强引用句柄。

<a id="Std.GCHandle.Target"></a>
### extern object Target { get; }

[`src/libraries/z42.core/src/GC/GCHandle.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L39)

当前 slot 持有的 target；Free 后 / Weak 模式目标已回收 → `null`。

<a id="Std.GCHandle.IsAllocated"></a>
### extern bool IsAllocated { get; }

[`src/libraries/z42.core/src/GC/GCHandle.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L46)

slot 是否仍占用。

Weak 模式即使 target 已回收，IsAllocated 仍为 true（slot 由本 handle 拥有）；
调 Free 后才变 false。

<a id="Std.GCHandle.Kind"></a>
### extern GCHandleType Kind { get; }

[`src/libraries/z42.core/src/GC/GCHandle.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L50)

分配时的模式（Weak / Strong）。Free 后值未定义——先查 IsAllocated。

<a id="Std.GCHandle.Free()"></a>
### extern void Free()

[`src/libraries/z42.core/src/GC/GCHandle.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GCHandle.z42#L54)

释放 slot；之后所有共享 _slot 的 handle alias 同步失效。幂等。

