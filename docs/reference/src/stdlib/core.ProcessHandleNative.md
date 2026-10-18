<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ProcessHandleNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/ProcessNative.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L131)

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

成员：[`Wait`](#Std.IO.ProcessHandleNative.Wait(long)) · [`TryWait`](#Std.IO.ProcessHandleNative.TryWait(long)) · [`Kill`](#Std.IO.ProcessHandleNative.Kill(long,bool)) · [`WriteStdin`](#Std.IO.ProcessHandleNative.WriteStdin(long,byte[])) · [`CloseStdin`](#Std.IO.ProcessHandleNative.CloseStdin(long)) · [`Pid`](#Std.IO.ProcessHandleNative.Pid(long)) · [`Drop`](#Std.IO.ProcessHandleNative.Drop(long)) · [`ReadStdout`](#Std.IO.ProcessHandleNative.ReadStdout(long,byte[],int,int)) · [`ReadStderr`](#Std.IO.ProcessHandleNative.ReadStderr(long,byte[],int,int))

<a id="Std.IO.ProcessHandleNative.Wait(long)"></a>
### static extern object[] Wait(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L134)

（种子兼容）等待退出。

<a id="Std.IO.ProcessHandleNative.TryWait(long)"></a>
### static extern object TryWait(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L138)

（种子兼容）非阻塞等待。

<a id="Std.IO.ProcessHandleNative.Kill(long,bool)"></a>
### static extern object Kill(long slotId, bool force)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L142)

（种子兼容）终止子进程。

<a id="Std.IO.ProcessHandleNative.WriteStdin(long,byte[])"></a>
### static extern object WriteStdin(long slotId, byte[] bytes)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:146`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L146)

（种子兼容）写 stdin。

<a id="Std.IO.ProcessHandleNative.CloseStdin(long)"></a>
### static extern object CloseStdin(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L150)

（种子兼容）关闭 stdin。

<a id="Std.IO.ProcessHandleNative.Pid(long)"></a>
### static extern object Pid(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L154)

（种子兼容）子进程 pid。

<a id="Std.IO.ProcessHandleNative.Drop(long)"></a>
### static extern object Drop(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:158`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L158)

（种子兼容）释放句柄。

<a id="Std.IO.ProcessHandleNative.ReadStdout(long,byte[],int,int)"></a>
### static extern object ReadStdout(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L162)

（种子兼容）流式读 stdout。

<a id="Std.IO.ProcessHandleNative.ReadStderr(long,byte[],int,int)"></a>
### static extern object ReadStderr(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:166`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L166)

（种子兼容）流式读 stderr。

