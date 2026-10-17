<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Char

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Char.z42:7`

内置 `char` 类型（32 位 Unicode 标量值）的包装结构体，实现 IComparable / IEquatable。

与 C# 的差异（有意）：z42 的 `char` 是 32 位 Unicode 标量值，上界 0x10FFFF；而 C# 的
`char` 是 16 位 UTF-16 码元（`Char.MaxValue == '\uFFFF'`）。

```z42
public struct Char : IComparable, IEquatable {
    static char MinValue
    static char MaxValue
    static char Parse(string s)
    static bool TryParse(string s, ref char v)
    extern string ToString()
    bool Equals(char other)
    int GetHashCode()
    int CompareTo(char other)
    extern bool IsWhiteSpace()
    char ToLower()
    char ToUpper()
    bool IsDigit()
    bool IsLetter()
    bool IsLetterOrDigit()
    bool IsUpper()
    bool IsLower()
    bool IsPunctuation()
    static bool IsDigit(char c)
    static bool IsLetter(char c)
    static bool IsLetterOrDigit(char c)
    static bool IsWhiteSpace(char c)
    static bool IsUpper(char c)
    static bool IsLower(char c)
    static bool IsPunctuation(char c)
    static char ToUpper(char c)
    static char ToLower(char c)
}
```

### static char MinValue

`src/libraries/z42.core/src/Primitives/Char.z42:10`

最小值：'\0'（0）。

### static char MaxValue

`src/libraries/z42.core/src/Primitives/Char.z42:12`

最大值：0x10FFFF（Unicode 标量上界）。

### static char Parse(string s)

`src/libraries/z42.core/src/Primitives/Char.z42:15`

解析恰含一个字符的字符串为 char；长度不为 1 抛 FormatException，null 抛 ArgumentNullException。

### static bool TryParse(string s, ref char v)

`src/libraries/z42.core/src/Primitives/Char.z42:24`

非抛出解析：null / 长度不为 1 返回 false 且 `v` 置 '\0'。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Char.z42:31`

返回单字符字符串表示。

### bool Equals(char other)

`src/libraries/z42.core/src/Primitives/Char.z42:34`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Char.z42:36`

返回哈希码（char 的码位值）。

### int CompareTo(char other)

`src/libraries/z42.core/src/Primitives/Char.z42:39`

比较大小，返回负 / 0 / 正。

### extern bool IsWhiteSpace()

`src/libraries/z42.core/src/Primitives/Char.z42:48`

是否为 Unicode 空白字符。

### char ToLower()

`src/libraries/z42.core/src/Primitives/Char.z42:53`

转小写（纯 ASCII；非 ASCII 字符原样返回）。

### char ToUpper()

`src/libraries/z42.core/src/Primitives/Char.z42:58`

转大写（纯 ASCII；非 ASCII 字符原样返回）。

### bool IsDigit()

`src/libraries/z42.core/src/Primitives/Char.z42:65`

是否为十进制数字 '0'..'9'（纯 ASCII）。

### bool IsLetter()

`src/libraries/z42.core/src/Primitives/Char.z42:69`

是否为字母 a..z 或 A..Z（纯 ASCII）。

### bool IsLetterOrDigit()

`src/libraries/z42.core/src/Primitives/Char.z42:76`

是否为字母或数字（纯 ASCII）。

### bool IsUpper()

`src/libraries/z42.core/src/Primitives/Char.z42:80`

是否为大写字母 A..Z（纯 ASCII）。

### bool IsLower()

`src/libraries/z42.core/src/Primitives/Char.z42:84`

是否为小写字母 a..z（纯 ASCII）。

### bool IsPunctuation()

`src/libraries/z42.core/src/Primitives/Char.z42:88`

是否为标点（Unicode P* 分类在 U+0000–U+007F 的 ASCII 标点；$ + < = > ^ ` | ~ 属符号 S 类，返回 false）。

### static bool IsDigit(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:100`

静态形式：`Char.IsDigit(c)`。

### static bool IsLetter(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:102`

静态形式：`Char.IsLetter(c)`。

### static bool IsLetterOrDigit(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:104`

静态形式：`Char.IsLetterOrDigit(c)`。

### static bool IsWhiteSpace(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:106`

静态形式：`Char.IsWhiteSpace(c)`。

### static bool IsUpper(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:108`

静态形式：`Char.IsUpper(c)`。

### static bool IsLower(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:110`

静态形式：`Char.IsLower(c)`。

### static bool IsPunctuation(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:112`

静态形式：`Char.IsPunctuation(c)`。

### static char ToUpper(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:114`

静态形式：`Char.ToUpper(c)`。

### static char ToLower(char c)

`src/libraries/z42.core/src/Primitives/Char.z42:116`

静态形式：`Char.ToLower(c)`。

