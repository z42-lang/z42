<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OSKind

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Platform.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L114)

OS 的整数 kind 常量（与 `Platform.OSKindValue()` 返回值一致）。不是 enum，可直接参与 `==` 与 `switch`。

```z42
public static class OSKind {
    static int Unknown
    static int Linux
    static int MacOS
    static int Windows
    static int Android
    static int Ios
    static int Wasm
    static int FreeBSD
}
```

成员：[`Unknown`](#Std.OSKind.Unknown) · [`Linux`](#Std.OSKind.Linux) · [`MacOS`](#Std.OSKind.MacOS) · [`Windows`](#Std.OSKind.Windows) · [`Android`](#Std.OSKind.Android) · [`Ios`](#Std.OSKind.Ios) · [`Wasm`](#Std.OSKind.Wasm) · [`FreeBSD`](#Std.OSKind.FreeBSD)

<a id="Std.OSKind.Unknown"></a>
### static int Unknown

[`src/libraries/z42.core/src/Platform.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L116)

未识别（0）。

<a id="Std.OSKind.Linux"></a>
### static int Linux

[`src/libraries/z42.core/src/Platform.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L118)

Linux（1）。

<a id="Std.OSKind.MacOS"></a>
### static int MacOS

[`src/libraries/z42.core/src/Platform.z42:120`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L120)

macOS（2）。

<a id="Std.OSKind.Windows"></a>
### static int Windows

[`src/libraries/z42.core/src/Platform.z42:122`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L122)

Windows（3）。

<a id="Std.OSKind.Android"></a>
### static int Android

[`src/libraries/z42.core/src/Platform.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L124)

Android（4）。

<a id="Std.OSKind.Ios"></a>
### static int Ios

[`src/libraries/z42.core/src/Platform.z42:126`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L126)

iOS（5）。

<a id="Std.OSKind.Wasm"></a>
### static int Wasm

[`src/libraries/z42.core/src/Platform.z42:128`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L128)

wasm（6）。

<a id="Std.OSKind.FreeBSD"></a>
### static int FreeBSD

[`src/libraries/z42.core/src/Platform.z42:130`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L130)

FreeBSD（7）。

