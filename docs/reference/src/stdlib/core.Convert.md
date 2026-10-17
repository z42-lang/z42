<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Convert

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Convert.z42:7`

字符串 ↔ 数值 / 布尔 / 字符等类型之间的转换。

核心转换（ToInt32 / ToInt64 / ToDouble / ToString）是 VM intrinsics 的 native 绑定；
其余窄化数值 / 布尔 / 字符转换委托到对应基元的 `Parse`，让范围判定只有一处。

```z42
public static class Convert {
    static extern int ToInt32(string s)
    static extern long ToInt64(string s)
    static extern double ToDouble(string s)
    static extern string ToString(object value)
    static sbyte ToSByte(string s)
    static byte ToByte(string s)
    static short ToInt16(string s)
    static ushort ToUInt16(string s)
    static uint ToUInt32(string s)
    static ulong ToUInt64(string s)
    static float ToSingle(string s)
    static bool ToBoolean(string s)
    static char ToChar(string s)
}
```

### static extern int ToInt32(string s)

`src/libraries/z42.core/src/Convert.z42:10`

解析为 `int`。

### static extern long ToInt64(string s)

`src/libraries/z42.core/src/Convert.z42:14`

解析为 `long`。

### static extern double ToDouble(string s)

`src/libraries/z42.core/src/Convert.z42:18`

解析为 `double`。

### static extern string ToString(object value)

`src/libraries/z42.core/src/Convert.z42:22`

任意值转字符串。

### static sbyte ToSByte(string s)

`src/libraries/z42.core/src/Convert.z42:28`

解析为 `sbyte`。

### static byte ToByte(string s)

`src/libraries/z42.core/src/Convert.z42:30`

解析为 `byte`。

### static short ToInt16(string s)

`src/libraries/z42.core/src/Convert.z42:32`

解析为 `short`。

### static ushort ToUInt16(string s)

`src/libraries/z42.core/src/Convert.z42:34`

解析为 `ushort`。

### static uint ToUInt32(string s)

`src/libraries/z42.core/src/Convert.z42:36`

解析为 `uint`。

### static ulong ToUInt64(string s)

`src/libraries/z42.core/src/Convert.z42:38`

解析为 `ulong`。

### static float ToSingle(string s)

`src/libraries/z42.core/src/Convert.z42:41`

解析为 `float`。

### static bool ToBoolean(string s)

`src/libraries/z42.core/src/Convert.z42:43`

解析为 `bool`。

### static char ToChar(string s)

`src/libraries/z42.core/src/Convert.z42:45`

解析为 `char`。

