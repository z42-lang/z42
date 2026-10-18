<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Environment

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/Environment.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L4)

读命令行参数与环境变量、当前工作目录、进程退出等。

```z42
public static class Environment {
    static extern string? GetEnvironmentVariable(string name)
    static string GetEnvironmentVariable(string name, string fallback)
    static extern void SetEnvironmentVariable(string name, string value)
    static extern string[] GetCommandLineArgs()
    static extern void Exit(int code)
    static long GetCurrentTimeMs()
    static extern void UnsetEnvironmentVariable(string name)
    static extern string[] GetEnvironmentVariables()
    static extern string GetCurrentDirectory()
    static extern void SetCurrentDirectory(string path)
}
```

成员：[`GetEnvironmentVariable`](#Std.IO.Environment.GetEnvironmentVariable(string)) · [`GetEnvironmentVariable`](#Std.IO.Environment.GetEnvironmentVariable(string,string)) · [`SetEnvironmentVariable`](#Std.IO.Environment.SetEnvironmentVariable(string,string)) · [`GetCommandLineArgs`](#Std.IO.Environment.GetCommandLineArgs()) · [`Exit`](#Std.IO.Environment.Exit(int)) · [`GetCurrentTimeMs`](#Std.IO.Environment.GetCurrentTimeMs()) · [`UnsetEnvironmentVariable`](#Std.IO.Environment.UnsetEnvironmentVariable(string)) · [`GetEnvironmentVariables`](#Std.IO.Environment.GetEnvironmentVariables()) · [`GetCurrentDirectory`](#Std.IO.Environment.GetCurrentDirectory()) · [`SetCurrentDirectory`](#Std.IO.Environment.SetCurrentDirectory(string))

<a id="Std.IO.Environment.GetEnvironmentVariable(string)"></a>
### static extern string? GetEnvironmentVariable(string name)

[`src/libraries/z42.core/src/IO/Environment.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L7)

取环境变量；变量不存在返回 null。

<a id="Std.IO.Environment.GetEnvironmentVariable(string,string)"></a>
### static string GetEnvironmentVariable(string name, string fallback)

[`src/libraries/z42.core/src/IO/Environment.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L11)

取环境变量；没设置（或设了个空串）时返回 `fallback`。需要区分「没设置」与 「设成了空串」的调用点，用上面那个返回 `string?` 的重载。

<a id="Std.IO.Environment.SetEnvironmentVariable(string,string)"></a>
### static extern void SetEnvironmentVariable(string name, string value)

[`src/libraries/z42.core/src/IO/Environment.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L19)

设置本进程的环境变量（会被之后 spawn 的子进程继承）。

<a id="Std.IO.Environment.GetCommandLineArgs()"></a>
### static extern string[] GetCommandLineArgs()

[`src/libraries/z42.core/src/IO/Environment.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L23)

程序自己的命令行参数（不含程序名）。

<a id="Std.IO.Environment.Exit(int)"></a>
### static extern void Exit(int code)

[`src/libraries/z42.core/src/IO/Environment.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L27)

立即以 `code` 退出进程。

<a id="Std.IO.Environment.GetCurrentTimeMs()"></a>
### static long GetCurrentTimeMs()

[`src/libraries/z42.core/src/IO/Environment.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L31)

墙钟 Unix 毫秒。

<a id="Std.IO.Environment.UnsetEnvironmentVariable(string)"></a>
### static extern void UnsetEnvironmentVariable(string name)

[`src/libraries/z42.core/src/IO/Environment.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L35)

删除环境变量。

<a id="Std.IO.Environment.GetEnvironmentVariables()"></a>
### static extern string[] GetEnvironmentVariables()

[`src/libraries/z42.core/src/IO/Environment.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L39)

平铺的 `"KEY=VALUE"` 数组；调用方按**第一个** `=` 切分。

<a id="Std.IO.Environment.GetCurrentDirectory()"></a>
### static extern string GetCurrentDirectory()

[`src/libraries/z42.core/src/IO/Environment.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L43)

当前工作目录（`pwd`）。

<a id="Std.IO.Environment.SetCurrentDirectory(string)"></a>
### static extern void SetCurrentDirectory(string path)

[`src/libraries/z42.core/src/IO/Environment.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Environment.z42#L48)

改变当前工作目录（`cd path`）。目录不存在抛 `DirectoryNotFoundException`， 无权限抛 `UnauthorizedAccessException`。

