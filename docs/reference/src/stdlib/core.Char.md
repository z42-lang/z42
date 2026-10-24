<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Char

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Char.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L7)

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

成员：[`MinValue`](#Std.Char.MinValue) · [`MaxValue`](#Std.Char.MaxValue) · [`Parse`](#Std.Char.Parse(string)) · [`TryParse`](#Std.Char.TryParse(string,char)) · [`ToString`](#Std.Char.ToString()) · [`Equals`](#Std.Char.Equals(char)) · [`GetHashCode`](#Std.Char.GetHashCode()) · [`CompareTo`](#Std.Char.CompareTo(char)) · [`IsWhiteSpace`](#Std.Char.IsWhiteSpace()) · [`ToLower`](#Std.Char.ToLower()) · [`ToUpper`](#Std.Char.ToUpper()) · [`IsDigit`](#Std.Char.IsDigit()) · [`IsLetter`](#Std.Char.IsLetter()) · [`IsLetterOrDigit`](#Std.Char.IsLetterOrDigit()) · [`IsUpper`](#Std.Char.IsUpper()) · [`IsLower`](#Std.Char.IsLower()) · [`IsPunctuation`](#Std.Char.IsPunctuation()) · [`IsDigit`](#Std.Char.IsDigit(char)) · [`IsLetter`](#Std.Char.IsLetter(char)) · [`IsLetterOrDigit`](#Std.Char.IsLetterOrDigit(char)) · [`IsWhiteSpace`](#Std.Char.IsWhiteSpace(char)) · [`IsUpper`](#Std.Char.IsUpper(char)) · [`IsLower`](#Std.Char.IsLower(char)) · [`IsPunctuation`](#Std.Char.IsPunctuation(char)) · [`ToUpper`](#Std.Char.ToUpper(char)) · [`ToLower`](#Std.Char.ToLower(char))

<a id="Std.Char.MinValue"></a>
### static char MinValue

[`src/libraries/z42.core/src/Primitives/Char.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L10)

最小值：'\0'（0）。

<a id="Std.Char.MaxValue"></a>
### static char MaxValue

[`src/libraries/z42.core/src/Primitives/Char.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L12)

最大值：0x10FFFF（Unicode 标量上界）。

<a id="Std.Char.Parse(string)"></a>
### static char Parse(string s)

[`src/libraries/z42.core/src/Primitives/Char.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L15)

解析恰含一个字符的字符串为 char；长度不为 1 抛 FormatException，null 抛 ArgumentNullException。

<a id="Std.Char.TryParse(string,char)"></a>
### static bool TryParse(string s, ref char v)

[`src/libraries/z42.core/src/Primitives/Char.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L24)

非抛出解析：null / 长度不为 1 返回 false 且 `v` 置 '\0'。

<a id="Std.Char.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Char.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L31)

返回单字符字符串表示。

<a id="Std.Char.Equals(char)"></a>
### bool Equals(char other)

[`src/libraries/z42.core/src/Primitives/Char.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L34)

值相等比较。

<a id="Std.Char.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Char.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L36)

返回哈希码（char 的码位值）。

<a id="Std.Char.CompareTo(char)"></a>
### int CompareTo(char other)

[`src/libraries/z42.core/src/Primitives/Char.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L39)

比较大小，返回负 / 0 / 正。

<a id="Std.Char.IsWhiteSpace()"></a>
### extern bool IsWhiteSpace()

[`src/libraries/z42.core/src/Primitives/Char.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L48)

是否为 Unicode 空白字符。

<a id="Std.Char.ToLower()"></a>
### char ToLower()

[`src/libraries/z42.core/src/Primitives/Char.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L53)

转小写（纯 ASCII；非 ASCII 字符原样返回）。

<a id="Std.Char.ToUpper()"></a>
### char ToUpper()

[`src/libraries/z42.core/src/Primitives/Char.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L58)

转大写（纯 ASCII；非 ASCII 字符原样返回）。

<a id="Std.Char.IsDigit()"></a>
### bool IsDigit()

[`src/libraries/z42.core/src/Primitives/Char.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L65)

是否为十进制数字 '0'..'9'（纯 ASCII）。

<a id="Std.Char.IsLetter()"></a>
### bool IsLetter()

[`src/libraries/z42.core/src/Primitives/Char.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L69)

是否为字母 a..z 或 A..Z（纯 ASCII）。

<a id="Std.Char.IsLetterOrDigit()"></a>
### bool IsLetterOrDigit()

[`src/libraries/z42.core/src/Primitives/Char.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L76)

是否为字母或数字（纯 ASCII）。

<a id="Std.Char.IsUpper()"></a>
### bool IsUpper()

[`src/libraries/z42.core/src/Primitives/Char.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L80)

是否为大写字母 A..Z（纯 ASCII）。

<a id="Std.Char.IsLower()"></a>
### bool IsLower()

[`src/libraries/z42.core/src/Primitives/Char.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L84)

是否为小写字母 a..z（纯 ASCII）。

<a id="Std.Char.IsPunctuation()"></a>
### bool IsPunctuation()

[`src/libraries/z42.core/src/Primitives/Char.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L90)

是否为标点（Unicode P* 分类在 U+0000–U+007F 的 ASCII 标点）。

$ + < = > ^ ` | ~ 属符号 S 类，返回 false。

<a id="Std.Char.IsDigit(char)"></a>
### static bool IsDigit(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L102)

静态形式：`Char.IsDigit(c)`。

<a id="Std.Char.IsLetter(char)"></a>
### static bool IsLetter(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L104)

静态形式：`Char.IsLetter(c)`。

<a id="Std.Char.IsLetterOrDigit(char)"></a>
### static bool IsLetterOrDigit(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L106)

静态形式：`Char.IsLetterOrDigit(c)`。

<a id="Std.Char.IsWhiteSpace(char)"></a>
### static bool IsWhiteSpace(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L108)

静态形式：`Char.IsWhiteSpace(c)`。

<a id="Std.Char.IsUpper(char)"></a>
### static bool IsUpper(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:110`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L110)

静态形式：`Char.IsUpper(c)`。

<a id="Std.Char.IsLower(char)"></a>
### static bool IsLower(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:112`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L112)

静态形式：`Char.IsLower(c)`。

<a id="Std.Char.IsPunctuation(char)"></a>
### static bool IsPunctuation(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L114)

静态形式：`Char.IsPunctuation(c)`。

<a id="Std.Char.ToUpper(char)"></a>
### static char ToUpper(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L116)

静态形式：`Char.ToUpper(c)`。

<a id="Std.Char.ToLower(char)"></a>
### static char ToLower(char c)

[`src/libraries/z42.core/src/Primitives/Char.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Char.z42#L118)

静态形式：`Char.ToLower(c)`。

