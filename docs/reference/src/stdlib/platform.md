# `Std.Platform` / `Std.OperatingSystem` —— 宿主与运行时能力查询

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`（`src/Platform.z42`、`src/OperatingSystem.z42`）；
> 命名空间 `Std`
>
> VM 旋钮（`gc-mode` / `mode` / `log` …）的取值查询不在这里，见
> [`Std.Runtime.RuntimeConfig`](runtime-config.md)。

两个静态类，回答两类问题：

- **`Std.Platform`** —— 「这是什么平台、这个 z42vm 能干什么」：OS / 架构标识，以及
  **本二进制编进了哪些能力**（JIT、native interop、线程、socket…）。
- **`Std.OperatingSystem`** —— 「当前进程与机器的具体数值」：pid、可执行文件路径、
  工作目录、主机名、CPU 数、OS 版本串。

两者都在 `Std` 命名空间，`z42.core` 隐式加载，**不需要写 `using`**。

## `Std.Platform`

```z42
public static class Platform {
    public static extern string   OS();
    public static extern string   Arch();
    public static extern string   Family();

    public static extern int      OSKindValue();
    public static extern int      ArchKindValue();

    public static bool IsLinux();
    public static bool IsMacOS();
    public static bool IsWindows();
    public static bool IsAndroid();
    public static bool IsIos();
    public static bool IsWasm();
    public static bool IsFreeBSD();
    public static bool IsUnix();

    public static extern string[] Capabilities();
    public static extern string[] ExecModes();

    public static bool HasJit();
    public static bool HasNativeInterop();
    public static bool HasThreads();
    public static bool HasAot();
}
```

同一件事有**两种形态**：字符串（`OS()`）和整数 kind + 谓词（`OSKindValue()` /
`IsMacOS()`）。两者读的是同一个来源，任选其一。字符串适合直接打印或与外部约定比对，
kind 适合分支。

### 字符串形态

| 方法 | 取值 |
|---|---|
| `OS()` | `"linux"` `"macos"` `"windows"` `"android"` `"ios"` `"wasm"` `"freebsd"`，或目标三元组里的其它 OS 名 |
| `Arch()` | `"x86_64"` `"aarch64"` `"wasm32"` `"x86"`，或目标三元组里的其它架构名 |
| `Family()` | Unix 系为 `"unix"`，Windows 为 `"windows"` |

三者在同一个 z42vm 二进制里**恒定**，不随进程状态、工作目录、环境变量变化。

`OS()` 在 wasm 上返回 `"wasm"`。`Arch()` / `Family()` 不做这层归一——**判断是不是 wasm
一律用 `IsWasm()` 或 `OS()`，别用 `Family()`**。

`OS()` 的取值就是 `[Skip(platform: "…")]` 里要写的那个字符串（比较是整串相等）。

### kind 形态

`OSKindValue()` / `ArchKindValue()` 返回 int，常量列在 `OSKind` / `ArchKind` 两个静态类里。
它们是 `public static int` 字段，**不是 enum**，所以能直接参与 `==` 与 `switch`。

```z42
public static class OSKind {
    public static int Unknown = 0;
    public static int Linux   = 1;
    public static int MacOS   = 2;
    public static int Windows = 3;
    public static int Android = 4;
    public static int Ios     = 5;
    public static int Wasm    = 6;
    public static int FreeBSD = 7;
}

public static class ArchKind {
    public static int Unknown = 0;
    public static int X64     = 1;   // x86_64
    public static int Arm64   = 2;   // aarch64
    public static int Wasm    = 3;   // wasm32
    public static int X86     = 4;   // x86
}
```

识别不出的 OS / 架构落 `Unknown`（`0`），此时 `OS()` / `Arch()` 仍会给出原始的目标三元组
名字。**成员名是 `Ios` 不是 `IOS`**，谓词相应地是 `IsIos()`。

### OS 谓词

| 谓词 | 为 true 的 kind |
|---|---|
| `IsLinux()` | `Linux` |
| `IsMacOS()` | `MacOS` |
| `IsWindows()` | `Windows` |
| `IsAndroid()` | `Android` |
| `IsIos()` | `Ios` |
| `IsWasm()` | `Wasm` |
| `IsFreeBSD()` | `FreeBSD` |
| `IsUnix()` | `Linux` / `MacOS` / `Ios` / `Android` / `FreeBSD` 任一 |

前七个**互斥**：至多一个为真（`Unknown` 时全假）。`IsUnix()` 与它们重叠，不参与互斥。
`IsWindows()` 与 `IsWasm()` 不属于 `IsUnix()`。

### 能力查询

`Capabilities()` 与 `ExecModes()` 返回的是**这个 z42vm 二进制编进了什么**，不是从平台
名猜的。要写「有就用、没有就退化」的自适应逻辑，问这两个，别去猜 OS。

```z42
public static extern string[] Capabilities();   // 例：["jit", "native-interop", "threads", "socket"]
public static extern string[] ExecModes();      // 例：["interp", "jit"]
```

两者都返回 `string[]`（不是逗号串），元素顺序稳定，可以直接按下标打印做诊断。

| `Capabilities()` 元素 | 含义 |
|---|---|
| `"jit"` | 编进了 JIT 后端 |
| `"native-interop"` | 编进了 native FFI（`[Native]` extern 走动态库那一支） |
| `"bundled-compression"` | 编进了内置压缩编解码器 |
| `"threads"` | 有真实 OS 线程（`Std.Threading`）。wasm 上没有 |
| `"socket"` | 有真实 OS 网络（TCP / UDP / HTTP / WS）。wasm 上没有 |

| `ExecModes()` 元素 | 含义 |
|---|---|
| `"interp"` | 恒在 |
| `"jit"` | 编进了 JIT |
| `"aot"` | 编进了 AOT 后端 |

四个谓词是对这两个数组的一次线性扫描；反复判断时自己缓存数组：

| 谓词 | 等价于 |
|---|---|
| `HasJit()` | `Capabilities()` 含 `"jit"` |
| `HasNativeInterop()` | `Capabilities()` 含 `"native-interop"` |
| `HasThreads()` | `Capabilities()` 含 `"threads"` |
| `HasAot()` | **`ExecModes()`** 含 `"aot"`（不是 `Capabilities()`）|

> `"aot"` 出现在 `ExecModes()` 里的含义是「编进来了」，**不是「能跑」**——AOT 执行本身
> 尚未实现。`HasAot()` 为真也不代表 `--mode aot` 能用。

### 用法

```z42
using Std.IO;

void Main() {
    Console.WriteLine(Platform.OS() + "/" + Platform.Arch());   // macos/aarch64

    if (Platform.IsUnix()) { /* 走 POSIX 路径 */ }

    // 自适应：有线程才并行
    if (Platform.HasThreads()) { /* 起 worker */ } else { /* 串行回落 */ }

    foreach (string c in Platform.Capabilities()) {
        Console.WriteLine("cap: " + c);
    }
}
```

## `Std.OperatingSystem`

```z42
public static class OperatingSystem {
    public static extern int    CurrentPid();
    public static extern string ExecutablePath();
    public static extern string CurrentDirectory();
    public static extern void   SetCurrentDirectory(string path);
    public static extern string Hostname();
    public static extern int    CpuCount();
    public static extern string OsVersion();
}
```

| 成员 | 返回 | 失败时 |
|---|---|---|
| `CurrentPid()` | 当前进程 id | wasm 上恒为 `0` |
| `ExecutablePath()` | **正在运行的 z42vm 二进制**的绝对路径 | `""` |
| `CurrentDirectory()` | 当前工作目录绝对路径 | `""` |
| `SetCurrentDirectory(path)` | 无 | **抛 `Std.Exception`**（唯一会抛的成员）|
| `Hostname()` | 主机名 | `""` |
| `CpuCount()` | 可用并行度 | `1` |
| `OsVersion()` | OS 版本串 | `""` |

**除 `SetCurrentDirectory` 外，全部成员失败时返回空串 / 兜底值，不抛异常。** 在意失败与
否就判 `.Length == 0`。

`ExecutablePath()` 给的是 **VM 二进制**（`…/.z42/bin/z42vm` 之类），不是你的 `.z42`
源文件，也不是 `z42` 启动器。

`OsVersion()` 的格式**随 OS 变**，只适合打印，别去解析。Unix 上形如
`Darwin 24.6.0 Darwin Kernel Version 24.6.0: …`（三段依次是内核名 / release / version）。

`SetCurrentDirectory(path)` 改的是**整个进程**的工作目录，之后 `Std.IO` 的所有相对路径
都跟着变。路径不存在 / 无权限时抛 `Std.Exception`，消息带 OS 的 errno 文本（例如
`No such file or directory (os error 2)`）。空串同样抛。

跨平台取值差异：

| 成员 | Windows | wasm |
|---|---|---|
| `CurrentPid()` | 正常 | `0` |
| `ExecutablePath()` | 正常 | `""` |
| `CurrentDirectory()` | 正常 | `"/"` |
| `SetCurrentDirectory()` | 正常 | 静默不做事，也不抛 |
| `Hostname()` | `""` | `""` |
| `OsVersion()` | `""` | `"wasm"` |

### 与 `Environment` 的分工

`Std.IO.Environment` 也有 `GetCurrentDirectory()` / `SetCurrentDirectory()`，读写的是同一
个进程状态，两边可以混用。其余成员不重叠——环境变量、命令行参数、`Exit` 在
[`Environment`](io-file.md)，pid / 可执行文件路径 / 主机名 / CPU 数 / OS 版本只在
`Std.OperatingSystem`。

## 不支持

- **`Platform` 没有 setter**，也没有「假装成别的平台」的开关：返回值由编译这个二进制的
  目标三元组与 feature 决定，运行期不可改。
- **`OSKind` / `ArchKind` 不是 enum**，是 `public static int` 常量类。没有
  `OSKind.Parse(string)`、没有 `ToString()`、没有遍历全部取值的方法。
- **`Capabilities()` 没有「查询单个能力」的重载**，也没有集合类型；只有数组 + 自己扫。
- **能力名拼错不会报错**。`[Skip(feature: X)]` 是 deny-by-default：`X` 不在
  `Capabilities()` 里就判为「缺失」，于是该测试**被静默跳过**——拼错的能力名
  （`"thread"`、`"sockets"`）与「这台机器真没这个能力」不可区分，测试会一路绿着不跑。
  写 `feature:` 之前先用 `Capabilities()` 打一遍确认拼写。
- **`ExecModes()` 不回答「现在正在用哪种模式跑」**，只回答「编进来了哪几种」。当前模式
  查 `RuntimeConfig.Get("mode")`，见[运行时设置查询](runtime-config.md)。
- **没有 CPU 型号 / 内存总量 / 磁盘 / 电池 / 登录用户名**这类机器画像 API；
  `CpuCount()` 是唯一的硬件数值。
- **`Hostname()` 在 Windows 上恒为空串**（未接 Win32 调用），别拿它做 Windows 上的机器
  标识。
- **没有 FQDN / IP / 域名**：`Hostname()` 是短主机名，网络标识走
  [`z42.net`](net.md)。
