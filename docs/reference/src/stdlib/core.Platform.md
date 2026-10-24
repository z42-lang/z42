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

[`src/libraries/z42.core/src/Platform.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L14)

OS 名：`"linux"` / `"macos"` / `"windows"` 等（或目标三元组里的其它 OS 名）。

已知名：`"linux"` / `"macos"` / `"windows"` / `"android"` / `"ios"` /
`"wasm"` / `"freebsd"`。

<a id="Std.Platform.Arch()"></a>
### static extern string Arch()

[`src/libraries/z42.core/src/Platform.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L17)

架构名：`"x86_64"` / `"aarch64"` / `"wasm32"` / `"x86"`，或目标三元组里的其它架构名。

<a id="Std.Platform.Family()"></a>
### static extern string Family()

[`src/libraries/z42.core/src/Platform.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L20)

Unix 系为 `"unix"`，Windows 为 `"windows"`。

<a id="Std.Platform.OSKindValue()"></a>
### static extern int OSKindValue()

[`src/libraries/z42.core/src/Platform.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L24)

OS 的整数 kind（常量见 `OSKind`）。

<a id="Std.Platform.ArchKindValue()"></a>
### static extern int ArchKindValue()

[`src/libraries/z42.core/src/Platform.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L27)

架构的整数 kind（常量见 `ArchKind`）。

<a id="Std.Platform.IsLinux()"></a>
### static bool IsLinux()

[`src/libraries/z42.core/src/Platform.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L33)

是否 Linux。

<a id="Std.Platform.IsMacOS()"></a>
### static bool IsMacOS()

[`src/libraries/z42.core/src/Platform.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L37)

是否 macOS。

<a id="Std.Platform.IsWindows()"></a>
### static bool IsWindows()

[`src/libraries/z42.core/src/Platform.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L41)

是否 Windows。

<a id="Std.Platform.IsAndroid()"></a>
### static bool IsAndroid()

[`src/libraries/z42.core/src/Platform.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L45)

是否 Android。

<a id="Std.Platform.IsIos()"></a>
### static bool IsIos()

[`src/libraries/z42.core/src/Platform.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L49)

是否 iOS。

<a id="Std.Platform.IsWasm()"></a>
### static bool IsWasm()

[`src/libraries/z42.core/src/Platform.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L53)

是否 wasm。

<a id="Std.Platform.IsFreeBSD()"></a>
### static bool IsFreeBSD()

[`src/libraries/z42.core/src/Platform.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L57)

是否 FreeBSD。

<a id="Std.Platform.IsUnix()"></a>
### static bool IsUnix()

[`src/libraries/z42.core/src/Platform.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L61)

是否 Unix 系（Linux / MacOS / Ios / Android / FreeBSD 任一）。

<a id="Std.Platform.Capabilities()"></a>
### static extern string[] Capabilities()

[`src/libraries/z42.core/src/Platform.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L77)

本二进制编进的能力集，如 `["jit", "native-interop", "threads", "socket", "fs", ...]`。

元素顺序稳定，可直接打印做诊断。

<a id="Std.Platform.ExecModes()"></a>
### static extern string[] ExecModes()

[`src/libraries/z42.core/src/Platform.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L83)

本二进制可派发的执行后端：恒含 `"interp"`，编进 JIT / AOT 时再加 `"jit"` / `"aot"`。

注意 `"aot"` 是「编进来了」，不是「能跑」。

<a id="Std.Platform.HasJit()"></a>
### static bool HasJit()

[`src/libraries/z42.core/src/Platform.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L88)

是否编进 JIT 后端（`Capabilities()` 含 `"jit"`）。

<a id="Std.Platform.HasNativeInterop()"></a>
### static bool HasNativeInterop()

[`src/libraries/z42.core/src/Platform.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L92)

是否编进 native FFI（`Capabilities()` 含 `"native-interop"`）。

<a id="Std.Platform.HasThreads()"></a>
### static bool HasThreads()

[`src/libraries/z42.core/src/Platform.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L96)

是否有真实 OS 线程（`Capabilities()` 含 `"threads"`）。

<a id="Std.Platform.HasAot()"></a>
### static bool HasAot()

[`src/libraries/z42.core/src/Platform.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L100)

是否编进 AOT 后端（`ExecModes()` 含 `"aot"`）。编进 ≠ 能跑。

