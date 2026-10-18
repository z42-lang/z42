<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OperatingSystem

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/OperatingSystem.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L7)

当前进程与机器的具体数值：pid、可执行文件路径、工作目录、主机名、CPU 数、OS 版本串。

对标 .NET System.Environment 成员。除 `SetCurrentDirectory` 外，全部成员失败时返回空串 /
兜底值，不抛异常（在意失败与否就判 `.Length == 0`）。

```z42
public static class OperatingSystem {
    static extern int CurrentPid()
    static extern string ExecutablePath()
    static extern string CurrentDirectory()
    static extern void SetCurrentDirectory(string path)
    static extern string Hostname()
    static extern int CpuCount()
    static extern string OsVersion()
}
```

成员：[`CurrentPid`](#Std.OperatingSystem.CurrentPid()) · [`ExecutablePath`](#Std.OperatingSystem.ExecutablePath()) · [`CurrentDirectory`](#Std.OperatingSystem.CurrentDirectory()) · [`SetCurrentDirectory`](#Std.OperatingSystem.SetCurrentDirectory(string)) · [`Hostname`](#Std.OperatingSystem.Hostname()) · [`CpuCount`](#Std.OperatingSystem.CpuCount()) · [`OsVersion`](#Std.OperatingSystem.OsVersion())

<a id="Std.OperatingSystem.CurrentPid()"></a>
### static extern int CurrentPid()

[`src/libraries/z42.core/src/OperatingSystem.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L10)

当前进程 id。wasm 上恒为 0。

<a id="Std.OperatingSystem.ExecutablePath()"></a>
### static extern string ExecutablePath()

[`src/libraries/z42.core/src/OperatingSystem.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L14)

正在运行的 z42vm 二进制的绝对路径；失败为 `""`。

<a id="Std.OperatingSystem.CurrentDirectory()"></a>
### static extern string CurrentDirectory()

[`src/libraries/z42.core/src/OperatingSystem.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L18)

当前工作目录绝对路径；失败为 `""`。

<a id="Std.OperatingSystem.SetCurrentDirectory(string)"></a>
### static extern void SetCurrentDirectory(string path)

[`src/libraries/z42.core/src/OperatingSystem.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L23)

设置整个进程的工作目录（此后 Std.IO 的相对路径都跟着变）。失败抛 Std.Exception （唯一会抛的成员）。

<a id="Std.OperatingSystem.Hostname()"></a>
### static extern string Hostname()

[`src/libraries/z42.core/src/OperatingSystem.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L27)

主机名；失败为 `""`（Windows 上恒空）。

<a id="Std.OperatingSystem.CpuCount()"></a>
### static extern int CpuCount()

[`src/libraries/z42.core/src/OperatingSystem.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L31)

可用并行度；失败为 1。

<a id="Std.OperatingSystem.OsVersion()"></a>
### static extern string OsVersion()

[`src/libraries/z42.core/src/OperatingSystem.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/OperatingSystem.z42#L35)

OS 版本串（格式随 OS 变，只适合打印）；失败为 `""`（wasm 上为 `"wasm"`）。

