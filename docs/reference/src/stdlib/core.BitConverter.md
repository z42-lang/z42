<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.BitConverter

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/BitConverter.z42:9`

浮点与整数位模式之间的 IEEE-754 位重解释。

```z42
public static class BitConverter {
    static extern int SingleToBits(double v)
    static extern double SingleFromBits(int bits)
    static extern long DoubleToBits(double v)
    static extern double DoubleFromBits(long bits)
}
```

### static extern int SingleToBits(double v)

`src/libraries/z42.core/src/BitConverter.z42:12`

`float` 值的位模式（作为 `int`）。

### static extern double SingleFromBits(int bits)

`src/libraries/z42.core/src/BitConverter.z42:16`

由位模式还原 `float` 值。

### static extern long DoubleToBits(double v)

`src/libraries/z42.core/src/BitConverter.z42:20`

`double` 值的位模式（作为 `long`）。

### static extern double DoubleFromBits(long bits)

`src/libraries/z42.core/src/BitConverter.z42:24`

由位模式还原 `double` 值。

