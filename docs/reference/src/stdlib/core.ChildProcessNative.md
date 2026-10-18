<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ChildProcessNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/ProcessNative.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L92)

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

成员：[`Wait`](#Std.IO.ChildProcessNative.Wait(long)) · [`TryWait`](#Std.IO.ChildProcessNative.TryWait(long)) · [`Kill`](#Std.IO.ChildProcessNative.Kill(long,bool)) · [`WriteStdin`](#Std.IO.ChildProcessNative.WriteStdin(long,byte[])) · [`CloseStdin`](#Std.IO.ChildProcessNative.CloseStdin(long)) · [`Pid`](#Std.IO.ChildProcessNative.Pid(long)) · [`Close`](#Std.IO.ChildProcessNative.Close(long)) · [`ReadStdout`](#Std.IO.ChildProcessNative.ReadStdout(long,byte[],int,int)) · [`ReadStderr`](#Std.IO.ChildProcessNative.ReadStderr(long,byte[],int,int))

<a id="Std.IO.ChildProcessNative.Wait(long)"></a>
### static extern object[] Wait(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L95)

等待退出并返回结果数组（消耗句柄）。

<a id="Std.IO.ChildProcessNative.TryWait(long)"></a>
### static extern object TryWait(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L99)

还在跑 → null；已退出 → 结果数组（并消耗句柄）。

<a id="Std.IO.ChildProcessNative.Kill(long,bool)"></a>
### static extern void Kill(long slotId, bool force)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L103)

终止子进程（force 决定 SIGKILL 还是 SIGTERM）。

<a id="Std.IO.ChildProcessNative.WriteStdin(long,byte[])"></a>
### static extern void WriteStdin(long slotId, byte[] bytes)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L107)

写子进程 stdin。

<a id="Std.IO.ChildProcessNative.CloseStdin(long)"></a>
### static extern void CloseStdin(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L111)

关闭子进程 stdin。

<a id="Std.IO.ChildProcessNative.Pid(long)"></a>
### static extern long Pid(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L115)

子进程 pid。

<a id="Std.IO.ChildProcessNative.Close(long)"></a>
### static extern void Close(long slotId)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:119`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L119)

幂等；子进程仍活着时 kill + reap。

<a id="Std.IO.ChildProcessNative.ReadStdout(long,byte[],int,int)"></a>
### static extern int ReadStdout(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L123)

流式读 stdout（0 = EOF）。

<a id="Std.IO.ChildProcessNative.ReadStderr(long,byte[],int,int)"></a>
### static extern int ReadStderr(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/ProcessNative.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/ProcessNative.z42#L127)

流式读 stderr（0 = EOF）。

