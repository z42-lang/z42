<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ProcessHandleNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/ProcessNative.z42:131`

种子兼容（旧形状）：子进程句柄操作，返回判别数组。

```z42
public static class ProcessHandleNative {
    static extern object[] Wait(long slotId)
    static extern object TryWait(long slotId)
    static extern object Kill(long slotId, bool force)
    static extern object WriteStdin(long slotId, byte[] bytes)
    static extern object CloseStdin(long slotId)
    static extern object Pid(long slotId)
    static extern object Drop(long slotId)
    static extern object ReadStdout(long slotId, byte[] buffer, int offset, int count)
    static extern object ReadStderr(long slotId, byte[] buffer, int offset, int count)
}
```

### static extern object[] Wait(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:134`

（种子兼容）等待退出。

### static extern object TryWait(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:138`

（种子兼容）非阻塞等待。

### static extern object Kill(long slotId, bool force)

`src/libraries/z42.core/src/IO/ProcessNative.z42:142`

（种子兼容）终止子进程。

### static extern object WriteStdin(long slotId, byte[] bytes)

`src/libraries/z42.core/src/IO/ProcessNative.z42:146`

（种子兼容）写 stdin。

### static extern object CloseStdin(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:150`

（种子兼容）关闭 stdin。

### static extern object Pid(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:154`

（种子兼容）子进程 pid。

### static extern object Drop(long slotId)

`src/libraries/z42.core/src/IO/ProcessNative.z42:158`

（种子兼容）释放句柄。

### static extern object ReadStdout(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/ProcessNative.z42:162`

（种子兼容）流式读 stdout。

### static extern object ReadStderr(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/ProcessNative.z42:166`

（种子兼容）流式读 stderr。

