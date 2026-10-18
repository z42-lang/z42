<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Version

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Version.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L7)

点分四段版本号 `major.minor[.build[.revision]]`（C# System.Version 对标）。

build / revision 可选；未提供时存 -1 且 ToString 省略。按四段依次比较排序
（未设置的 -1 排在任意真实 0+ 之前，同 C#）。

```z42
public sealed class Version {
    Version(int major, int minor)
    Version(int major, int minor, int build)
    Version(int major, int minor, int build, int revision)
    int Major { get; }
    int Minor { get; }
    int Build { get; }
    int Revision { get; }
    static Version Parse(string s)
    static Version? TryParse(string s)
    int CompareTo(Version other)
    bool Equals(Version other)
    int GetHashCode()
    override string ToString()
}
```

成员：[`Version`](#Std.Version.ctor(int,int)) · [`Version`](#Std.Version.ctor(int,int,int)) · [`Version`](#Std.Version.ctor(int,int,int,int)) · [`Major`](#Std.Version.Major) · [`Minor`](#Std.Version.Minor) · [`Build`](#Std.Version.Build) · [`Revision`](#Std.Version.Revision) · [`Parse`](#Std.Version.Parse(string)) · [`TryParse`](#Std.Version.TryParse(string)) · [`CompareTo`](#Std.Version.CompareTo(Version)) · [`Equals`](#Std.Version.Equals(Version)) · [`GetHashCode`](#Std.Version.GetHashCode()) · [`ToString`](#Std.Version.ToString())

<a id="Std.Version.ctor(int,int)"></a>
### Version(int major, int minor)

[`src/libraries/z42.core/src/Version.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L14)

以 major + minor 构造（build / revision 视为未设置）。

<a id="Std.Version.ctor(int,int,int)"></a>
### Version(int major, int minor, int build)

[`src/libraries/z42.core/src/Version.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L24)

以 major + minor + build 构造（revision 视为未设置）。

<a id="Std.Version.ctor(int,int,int,int)"></a>
### Version(int major, int minor, int build, int revision)

[`src/libraries/z42.core/src/Version.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L35)

以四段全量构造。

<a id="Std.Version.Major"></a>
### int Major { get; }

[`src/libraries/z42.core/src/Version.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L56)

主版本号。

<a id="Std.Version.Minor"></a>
### int Minor { get; }

[`src/libraries/z42.core/src/Version.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L58)

次版本号。

<a id="Std.Version.Build"></a>
### int Build { get; }

[`src/libraries/z42.core/src/Version.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L60)

构建号（未设置时为 -1）。

<a id="Std.Version.Revision"></a>
### int Revision { get; }

[`src/libraries/z42.core/src/Version.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L62)

修订号（未设置时为 -1）。

<a id="Std.Version.Parse(string)"></a>
### static Version Parse(string s)

[`src/libraries/z42.core/src/Version.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L68)

解析 `"major.minor"` / `"major.minor.build"` / `"major.minor.build.revision"`。 段数不对或某段非整数 / 为负抛 FormatException。

<a id="Std.Version.TryParse(string)"></a>
### static Version? TryParse(string s)

[`src/libraries/z42.core/src/Version.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L99)

不抛异常的解析：成功返回 Version，任何非法输入返回 null。

<a id="Std.Version.CompareTo(Version)"></a>
### int CompareTo(Version other)

[`src/libraries/z42.core/src/Version.z42:110`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L110)

按 (major, minor, build, revision) 字典序比较，返回 -1 / 0 / 1。

<a id="Std.Version.Equals(Version)"></a>
### bool Equals(Version other)

[`src/libraries/z42.core/src/Version.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L125)

四段全等比较。

<a id="Std.Version.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Version.z42:133`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L133)

内容哈希码。

<a id="Std.Version.ToString()"></a>
### override string ToString()

[`src/libraries/z42.core/src/Version.z42:144`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Version.z42#L144)

`"major.minor[.build[.revision]]"` —— 未设置的尾部段（-1）省略。

