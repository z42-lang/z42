<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ProcessNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/ProcessNative.z42:11`

子进程 spawn / wait / kill / stdio 的 native 原语，供 z42.io 的 Process 应用层调用。 失败一律由 VM 直接抛（.NET 口径）：启动失败 → ProcessStartException（: IOException）， 超时 → ProcessTimeoutException（: TimeoutException），句柄已失效 → ProcessHandleInvalidException （: InvalidOperationException），未重定向的 stdin → InvalidOperationException，管道 I/O 错 → IOException。成功结果数组：[exitCode, stdout, stderr, stdoutBytes, stderrBytes]。

```z42
public static class ProcessNative {
    static extern object[] RunSync(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath, long timeoutMs, bool ownProcessGroup)
    static extern long Start(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath)
    static extern string? Which(string name)
    static extern object[] Run(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath, long timeoutMs, bool ownProcessGroup)
    static extern object[] Spawn(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath)
}
```

### static extern object[] RunSync(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath, long timeoutMs, bool ownProcessGroup)

`src/libraries/z42.core/src/IO/ProcessNative.z42:14`

同步运行子进程，返回结果数组。

### static extern long Start(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath)

`src/libraries/z42.core/src/IO/ProcessNative.z42:33`

启动子进程，返回句柄槽号。

### static extern string? Which(string name)

`src/libraries/z42.core/src/IO/ProcessNative.z42:49`

按 PATH 解析程序全路径；找不到返回 null。

### static extern object[] Run(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath, long timeoutMs, bool ownProcessGroup)

`src/libraries/z42.core/src/IO/ProcessNative.z42:56`

（种子兼容，旧形状）运行子进程，返回判别数组。

### static extern object[] Spawn(string program, string[] args, string[] envKeys, string[] envVals, string[] envRemove, bool envClear, string? cwd, int stdinMode, byte[]? stdinBytes, int stdoutMode, string? stdoutPath, int stderrMode, string? stderrPath)

`src/libraries/z42.core/src/IO/ProcessNative.z42:75`

（种子兼容，旧形状）启动子进程，返回判别数组。

