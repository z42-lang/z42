<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ArchKind

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Platform.z42:131`

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

### static int Unknown

`src/libraries/z42.core/src/Platform.z42:133`

未识别（0）。

### static int X64

`src/libraries/z42.core/src/Platform.z42:135`

x86_64（1）。

### static int Arm64

`src/libraries/z42.core/src/Platform.z42:137`

aarch64（2）。

### static int Wasm

`src/libraries/z42.core/src/Platform.z42:139`

wasm32（3）。

### static int X86

`src/libraries/z42.core/src/Platform.z42:141`

x86（4）。

