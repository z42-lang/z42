<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ChildProcessNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/ProcessNative.z42:92`

`Spawn()` 出来的子进程句柄（槽号）上的操作。

```z42
public static class ChildProcessNative {
    static extern object[] Wait(long slotId)
    static extern object TryWait(long slotId)
    static extern void Kill(long slotId, bool force)
    static extern void WriteStdin(long slotId, byte[] bytes)
    static extern void CloseStdin(long slotId)
    static extern long Pid(long slotId)
    static extern void Close(long slotId)
    static extern int ReadStdout(long slotId, byte[] buffer, int offset, int count)
    static extern int ReadStderr(long slotId, byte[] buffer, int offset, int count)
}
```

### static extern object[] Wait(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:95`

等待退出并返回结果数组（消耗句柄）。

### static extern object TryWait(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:99`

还在跑 → null；已退出 → 结果数组（并消耗句柄）。

### static extern void Kill(long slotId, bool force)

`src/libraries/z42.core/src/IO/ProcessNative.z42:103`

终止子进程（force 决定 SIGKILL 还是 SIGTERM）。

### static extern void WriteStdin(long slotId, byte[] bytes)

`src/libraries/z42.core/src/IO/ProcessNative.z42:107`

写子进程 stdin。

### static extern void CloseStdin(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:111`

关闭子进程 stdin。

### static extern long Pid(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:115`

子进程 pid。

### static extern void Close(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:119`

幂等；子进程仍活着时 kill + reap。

### static extern int ReadStdout(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/ProcessNative.z42:123`

流式读 stdout（0 = EOF）。

### static extern int ReadStderr(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/ProcessNative.z42:127`

流式读 stderr（0 = EOF）。

