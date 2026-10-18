<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ArchKind

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Platform.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L131)

架构的整数 kind 常量（与 `Platform.ArchKindValue()` 返回值一致）。命名对齐 .NET 的 `X64` / `Arm64` / `Wasm` / `X86`。

```z42
public static class ArchKind {
    static int Unknown
    static int X64
    static int Arm64
    static int Wasm
    static int X86
}
```

成员：[`Unknown`](#Std.ArchKind.Unknown) · [`X64`](#Std.ArchKind.X64) · [`Arm64`](#Std.ArchKind.Arm64) · [`Wasm`](#Std.ArchKind.Wasm) · [`X86`](#Std.ArchKind.X86)

<a id="Std.ArchKind.Unknown"></a>
### static int Unknown

[`src/libraries/z42.core/src/Platform.z42:133`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L133)

未识别（0）。

<a id="Std.ArchKind.X64"></a>
### static int X64

[`src/libraries/z42.core/src/Platform.z42:135`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L135)

x86_64（1）。

<a id="Std.ArchKind.Arm64"></a>
### static int Arm64

[`src/libraries/z42.core/src/Platform.z42:137`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L137)

aarch64（2）。

<a id="Std.ArchKind.Wasm"></a>
### static int Wasm

[`src/libraries/z42.core/src/Platform.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L139)

wasm32（3）。

<a id="Std.ArchKind.X86"></a>
### static int X86

[`src/libraries/z42.core/src/Platform.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Platform.z42#L141)

x86（4）。

