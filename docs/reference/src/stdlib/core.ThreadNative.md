<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ThreadNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Threading`

`src/libraries/z42.core/src/Native/ThreadingNative.z42:8`

Thread 应用层用的线程原语（__thread_spawn / __thread_sleep / __thread_wait）。

```z42
public static class ThreadNative {
    static extern long Spawn(object action)
    static extern void Sleep(long millis)
    static extern void Wait(long slotId)
    static extern object[] Join(long slotId)
}
```

### static extern long Spawn(object action)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:11`

开一条 OS 线程跑 action，返回句柄槽号。

### static extern void Sleep(long millis)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:15`

阻塞当前线程 millis 毫秒。

### static extern void Wait(long slotId)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:19`

等待线程结束；worker 抛过异常 / panic / 重复 Join → VM 直接抛 `Std.Threading.ThreadException`。

### static extern object[] Join(long slotId)

`src/libraries/z42.core/src/Native/ThreadingNative.z42:23`

（种子兼容，旧形状）等待线程，返回 [KIND, msg] 判别数组。

