<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.OSKind

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Platform.z42:110`

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

### static int Unknown

`src/libraries/z42.core/src/Platform.z42:112`

未识别（0）。

### static int Linux

`src/libraries/z42.core/src/Platform.z42:114`

Linux（1）。

### static int MacOS

`src/libraries/z42.core/src/Platform.z42:116`

macOS（2）。

### static int Windows

`src/libraries/z42.core/src/Platform.z42:118`

Windows（3）。

### static int Android

`src/libraries/z42.core/src/Platform.z42:120`

Android（4）。

### static int Ios

`src/libraries/z42.core/src/Platform.z42:122`

iOS（5）。

### static int Wasm

`src/libraries/z42.core/src/Platform.z42:124`

wasm（6）。

### static int FreeBSD

`src/libraries/z42.core/src/Platform.z42:126`

FreeBSD（7）。

