<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Convert

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Convert.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L7)

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
    static string ToHexStringLower(byte[] bytes)
    static extern string ToHexStringLower(string text)
}
```

成员：[`ToInt32`](#Std.Convert.ToInt32(string)) · [`ToInt64`](#Std.Convert.ToInt64(string)) · [`ToDouble`](#Std.Convert.ToDouble(string)) · [`ToString`](#Std.Convert.ToString(object)) · [`ToSByte`](#Std.Convert.ToSByte(string)) · [`ToByte`](#Std.Convert.ToByte(string)) · [`ToInt16`](#Std.Convert.ToInt16(string)) · [`ToUInt16`](#Std.Convert.ToUInt16(string)) · [`ToUInt32`](#Std.Convert.ToUInt32(string)) · [`ToUInt64`](#Std.Convert.ToUInt64(string)) · [`ToSingle`](#Std.Convert.ToSingle(string)) · [`ToBoolean`](#Std.Convert.ToBoolean(string)) · [`ToChar`](#Std.Convert.ToChar(string)) · [`ToHexStringLower`](#Std.Convert.ToHexStringLower(byte[])) · [`ToHexStringLower`](#Std.Convert.ToHexStringLower(string))

<a id="Std.Convert.ToInt32(string)"></a>
### static extern int ToInt32(string s)

[`src/libraries/z42.core/src/Convert.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L10)

解析为 `int`。

<a id="Std.Convert.ToInt64(string)"></a>
### static extern long ToInt64(string s)

[`src/libraries/z42.core/src/Convert.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L14)

解析为 `long`。

<a id="Std.Convert.ToDouble(string)"></a>
### static extern double ToDouble(string s)

[`src/libraries/z42.core/src/Convert.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L18)

解析为 `double`。

<a id="Std.Convert.ToString(object)"></a>
### static extern string ToString(object value)

[`src/libraries/z42.core/src/Convert.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L22)

任意值转字符串。

<a id="Std.Convert.ToSByte(string)"></a>
### static sbyte ToSByte(string s)

[`src/libraries/z42.core/src/Convert.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L28)

解析为 `sbyte`。

<a id="Std.Convert.ToByte(string)"></a>
### static byte ToByte(string s)

[`src/libraries/z42.core/src/Convert.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L30)

解析为 `byte`。

<a id="Std.Convert.ToInt16(string)"></a>
### static short ToInt16(string s)

[`src/libraries/z42.core/src/Convert.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L32)

解析为 `short`。

<a id="Std.Convert.ToUInt16(string)"></a>
### static ushort ToUInt16(string s)

[`src/libraries/z42.core/src/Convert.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L34)

解析为 `ushort`。

<a id="Std.Convert.ToUInt32(string)"></a>
### static uint ToUInt32(string s)

[`src/libraries/z42.core/src/Convert.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L36)

解析为 `uint`。

<a id="Std.Convert.ToUInt64(string)"></a>
### static ulong ToUInt64(string s)

[`src/libraries/z42.core/src/Convert.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L38)

解析为 `ulong`。

<a id="Std.Convert.ToSingle(string)"></a>
### static float ToSingle(string s)

[`src/libraries/z42.core/src/Convert.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L41)

解析为 `float`。

<a id="Std.Convert.ToBoolean(string)"></a>
### static bool ToBoolean(string s)

[`src/libraries/z42.core/src/Convert.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L43)

解析为 `bool`。

<a id="Std.Convert.ToChar(string)"></a>
### static char ToChar(string s)

[`src/libraries/z42.core/src/Convert.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L45)

解析为 `char`。

<a id="Std.Convert.ToHexStringLower(byte[])"></a>
### static string ToHexStringLower(byte[] bytes)

[`src/libraries/z42.core/src/Convert.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L50)

把字节编码为小写十六进制字符串（每字节 2 字符，无前缀、无分隔）。

<a id="Std.Convert.ToHexStringLower(string)"></a>
### static extern string ToHexStringLower(string text)

[`src/libraries/z42.core/src/Convert.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Convert.z42#L68)

把 `text` 的 UTF-8 编码字节编码为小写十六进制字符串。

等价于 `ToHexStringLower(Utf8.GetBytes(text))`，但不物化中间字节数组。

