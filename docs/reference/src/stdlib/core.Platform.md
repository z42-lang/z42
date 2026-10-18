<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Platform

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Platform.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L8)

跨平台 OS / 架构标识与运行时能力查询。

同一件事有两种形态：字符串（`OS()` / `Arch()` / `Family()`）与整数 kind + 谓词
（`OSKindValue()` / `IsMacOS()`）；两者读同一来源。能力查询（`Capabilities()` /
`ExecModes()`）报告**这个 z42vm 二进制编进了什么**，不是从平台名猜的。

```z42
public static class Platform {
    static extern string OS()
    static extern string Arch()
    static extern string Family()
    static extern int OSKindValue()
    static extern int ArchKindValue()
    static bool IsLinux()
    static bool IsMacOS()
    static bool IsWindows()
    static bool IsAndroid()
    static bool IsIos()
    static bool IsWasm()
    static bool IsFreeBSD()
    static bool IsUnix()
    static extern string[] Capabilities()
    static extern string[] ExecModes()
    static bool HasJit()
    static bool HasNativeInterop()
    static bool HasThreads()
    static bool HasAot()
}
```

成员：[`OS`](#Std.Platform.OS()) · [`Arch`](#Std.Platform.Arch()) · [`Family`](#Std.Platform.Family()) · [`OSKindValue`](#Std.Platform.OSKindValue()) · [`ArchKindValue`](#Std.Platform.ArchKindValue()) · [`IsLinux`](#Std.Platform.IsLinux()) · [`IsMacOS`](#Std.Platform.IsMacOS()) · [`IsWindows`](#Std.Platform.IsWindows()) · [`IsAndroid`](#Std.Platform.IsAndroid()) · [`IsIos`](#Std.Platform.IsIos()) · [`IsWasm`](#Std.Platform.IsWasm()) · [`IsFreeBSD`](#Std.Platform.IsFreeBSD()) · [`IsUnix`](#Std.Platform.IsUnix()) · [`Capabilities`](#Std.Platform.Capabilities()) · [`ExecModes`](#Std.Platform.ExecModes()) · [`HasJit`](#Std.Platform.HasJit()) · [`HasNativeInterop`](#Std.Platform.HasNativeInterop()) · [`HasThreads`](#Std.Platform.HasThreads()) · [`HasAot`](#Std.Platform.HasAot())

<a id="Std.Platform.OS()"></a>
### static extern string OS()

[`src/libraries/z42.core/src/Platform.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L12)

OS 名：`"linux"` / `"macos"` / `"windows"` / `"android"` / `"ios"` / `"wasm"` / `"freebsd"`，或目标三元组里的其它 OS 名。

<a id="Std.Platform.Arch()"></a>
### static extern string Arch()

[`src/libraries/z42.core/src/Platform.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L15)

架构名：`"x86_64"` / `"aarch64"` / `"wasm32"` / `"x86"`，或目标三元组里的其它架构名。

<a id="Std.Platform.Family()"></a>
### static extern string Family()

[`src/libraries/z42.core/src/Platform.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L18)

Unix 系为 `"unix"`，Windows 为 `"windows"`。

<a id="Std.Platform.OSKindValue()"></a>
### static extern int OSKindValue()

[`src/libraries/z42.core/src/Platform.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L22)

OS 的整数 kind（常量见 `OSKind`）。

<a id="Std.Platform.ArchKindValue()"></a>
### static extern int ArchKindValue()

[`src/libraries/z42.core/src/Platform.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L25)

架构的整数 kind（常量见 `ArchKind`）。

<a id="Std.Platform.IsLinux()"></a>
### static bool IsLinux()

[`src/libraries/z42.core/src/Platform.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L31)

是否 Linux。

<a id="Std.Platform.IsMacOS()"></a>
### static bool IsMacOS()

[`src/libraries/z42.core/src/Platform.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L35)

是否 macOS。

<a id="Std.Platform.IsWindows()"></a>
### static bool IsWindows()

[`src/libraries/z42.core/src/Platform.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L39)

是否 Windows。

<a id="Std.Platform.IsAndroid()"></a>
### static bool IsAndroid()

[`src/libraries/z42.core/src/Platform.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L43)

是否 Android。

<a id="Std.Platform.IsIos()"></a>
### static bool IsIos()

[`src/libraries/z42.core/src/Platform.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L47)

是否 iOS。

<a id="Std.Platform.IsWasm()"></a>
### static bool IsWasm()

[`src/libraries/z42.core/src/Platform.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L51)

是否 wasm。

<a id="Std.Platform.IsFreeBSD()"></a>
### static bool IsFreeBSD()

[`src/libraries/z42.core/src/Platform.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L55)

是否 FreeBSD。

<a id="Std.Platform.IsUnix()"></a>
### static bool IsUnix()

[`src/libraries/z42.core/src/Platform.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L59)

是否 Unix 系（Linux / MacOS / Ios / Android / FreeBSD 任一）。

<a id="Std.Platform.Capabilities()"></a>
### static extern string[] Capabilities()

[`src/libraries/z42.core/src/Platform.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L74)

本二进制编进的能力集，如 `["jit", "native-interop", "threads", "socket", "fs", ...]`。 元素顺序稳定，可直接打印做诊断。

<a id="Std.Platform.ExecModes()"></a>
### static extern string[] ExecModes()

[`src/libraries/z42.core/src/Platform.z42:79`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L79)

本二进制可派发的执行后端：恒含 `"interp"`，编进 JIT / AOT 时再加 `"jit"` / `"aot"`。 注意 `"aot"` 是「编进来了」，不是「能跑」。

<a id="Std.Platform.HasJit()"></a>
### static bool HasJit()

[`src/libraries/z42.core/src/Platform.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L84)

是否编进 JIT 后端（`Capabilities()` 含 `"jit"`）。

<a id="Std.Platform.HasNativeInterop()"></a>
### static bool HasNativeInterop()

[`src/libraries/z42.core/src/Platform.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L88)

是否编进 native FFI（`Capabilities()` 含 `"native-interop"`）。

<a id="Std.Platform.HasThreads()"></a>
### static bool HasThreads()

[`src/libraries/z42.core/src/Platform.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L92)

是否有真实 OS 线程（`Capabilities()` 含 `"threads"`）。

<a id="Std.Platform.HasAot()"></a>
### static bool HasAot()

[`src/libraries/z42.core/src/Platform.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L96)

是否编进 AOT 后端（`ExecModes()` 含 `"aot"`）。编进 ≠ 能跑。

