<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MonitorNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

`src/libraries/z42.core/src/Native/ThreadingNative.z42:32`

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

### static extern object New()

`src/libraries/z42.core/src/Native/ThreadingNative.z42:35`

新建 monitor。

### static extern void Enter(object monitor)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:38`

阻塞获取；同线程重入抛异常。

### static extern bool TryEnter(object monitor)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:41`

非阻塞获取；被任何线程持有即 false。

### static extern void Exit(object monitor)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:44`

释放；非持有者抛异常。

### static extern void Wait(object monitor)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:47`

原子释放 → 等一次 Exit 信号 → 重新持有（允许伪唤醒）。

