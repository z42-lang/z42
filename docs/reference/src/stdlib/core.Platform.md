<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Platform

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Platform.z42:8`

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

### static extern string OS()

`src/libraries/z42.core/src/Platform.z42:12`

OS 名：`"linux"` / `"macos"` / `"windows"` / `"android"` / `"ios"` / `"wasm"` / `"freebsd"`，或目标三元组里的其它 OS 名。

### static extern string Arch()

`src/libraries/z42.core/src/Platform.z42:15`

架构名：`"x86_64"` / `"aarch64"` / `"wasm32"` / `"x86"`，或目标三元组里的其它架构名。

### static extern string Family()

`src/libraries/z42.core/src/Platform.z42:18`

Unix 系为 `"unix"`，Windows 为 `"windows"`。

### static extern int OSKindValue()

`src/libraries/z42.core/src/Platform.z42:22`

OS 的整数 kind（常量见 `OSKind`）。

### static extern int ArchKindValue()

`src/libraries/z42.core/src/Platform.z42:25`

架构的整数 kind（常量见 `ArchKind`）。

### static bool IsLinux()

`src/libraries/z42.core/src/Platform.z42:31`

是否 Linux。

### static bool IsMacOS()

`src/libraries/z42.core/src/Platform.z42:35`

是否 macOS。

### static bool IsWindows()

`src/libraries/z42.core/src/Platform.z42:39`

是否 Windows。

### static bool IsAndroid()

`src/libraries/z42.core/src/Platform.z42:43`

是否 Android。

### static bool IsIos()

`src/libraries/z42.core/src/Platform.z42:47`

是否 iOS。

### static bool IsWasm()

`src/libraries/z42.core/src/Platform.z42:51`

是否 wasm。

### static bool IsFreeBSD()

`src/libraries/z42.core/src/Platform.z42:55`

是否 FreeBSD。

### static bool IsUnix()

`src/libraries/z42.core/src/Platform.z42:59`

是否 Unix 系（Linux / MacOS / Ios / Android / FreeBSD 任一）。

### static extern string[] Capabilities()

`src/libraries/z42.core/src/Platform.z42:74`

本二进制编进的能力集，如 `["jit", "native-interop", "threads", "socket", "fs", ...]`。 元素顺序稳定，可直接打印做诊断。

### static extern string[] ExecModes()

`src/libraries/z42.core/src/Platform.z42:79`

本二进制可派发的执行后端：恒含 `"interp"`，编进 JIT / AOT 时再加 `"jit"` / `"aot"`。 注意 `"aot"` 是「编进来了」，不是「能跑」。

### static bool HasJit()

`src/libraries/z42.core/src/Platform.z42:84`

是否编进 JIT 后端（`Capabilities()` 含 `"jit"`）。

### static bool HasNativeInterop()

`src/libraries/z42.core/src/Platform.z42:88`

是否编进 native FFI（`Capabilities()` 含 `"native-interop"`）。

### static bool HasThreads()

`src/libraries/z42.core/src/Platform.z42:92`

是否有真实 OS 线程（`Capabilities()` 含 `"threads"`）。

### static bool HasAot()

`src/libraries/z42.core/src/Platform.z42:96`

是否编进 AOT 后端（`ExecModes()` 含 `"aot"`）。编进 ≠ 能跑。

