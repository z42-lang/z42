<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MonitorNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L32)

Mutex / RwLock / Channel 的唯一原生底座。

```z42
public static class MonitorNative {
    static extern object New()
    static extern void Enter(object monitor)
    static extern bool TryEnter(object monitor)
    static extern void Exit(object monitor)
    static extern void Wait(object monitor)
}
```

成员：[`New`](#Std.Threading.MonitorNative.New()) · [`Enter`](#Std.Threading.MonitorNative.Enter(object)) · [`TryEnter`](#Std.Threading.MonitorNative.TryEnter(object)) · [`Exit`](#Std.Threading.MonitorNative.Exit(object)) · [`Wait`](#Std.Threading.MonitorNative.Wait(object))

<a id="Std.Threading.MonitorNative.New()"></a>
### static extern object New()

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L35)

新建 monitor。

<a id="Std.Threading.MonitorNative.Enter(object)"></a>
### static extern void Enter(object monitor)

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L38)

阻塞获取；同线程重入抛异常。

<a id="Std.Threading.MonitorNative.TryEnter(object)"></a>
### static extern bool TryEnter(object monitor)

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L41)

非阻塞获取；被任何线程持有即 false。

<a id="Std.Threading.MonitorNative.Exit(object)"></a>
### static extern void Exit(object monitor)

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L44)

释放；非持有者抛异常。

<a id="Std.Threading.MonitorNative.Wait(object)"></a>
### static extern void Wait(object monitor)

[`src/libraries/z42.core/src/Native/ThreadingNative.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/ThreadingNative.z42#L47)

原子释放 → 等一次 Exit 信号 → 重新持有（允许伪唤醒）。

