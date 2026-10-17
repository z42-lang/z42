<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Version

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Version.z42:7`

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

### Version(int major, int minor)

`src/libraries/z42.core/src/Version.z42:14`

以 major + minor 构造（build / revision 视为未设置）。

### Version(int major, int minor, int build)

`src/libraries/z42.core/src/Version.z42:24`

以 major + minor + build 构造（revision 视为未设置）。

### Version(int major, int minor, int build, int revision)

`src/libraries/z42.core/src/Version.z42:35`

以四段全量构造。

### int Major { get; }

`src/libraries/z42.core/src/Version.z42:56`

主版本号。

### int Minor { get; }

`src/libraries/z42.core/src/Version.z42:58`

次版本号。

### int Build { get; }

`src/libraries/z42.core/src/Version.z42:60`

构建号（未设置时为 -1）。

### int Revision { get; }

`src/libraries/z42.core/src/Version.z42:62`

修订号（未设置时为 -1）。

### static Version Parse(string s)

`src/libraries/z42.core/src/Version.z42:68`

解析 `"major.minor"` / `"major.minor.build"` / `"major.minor.build.revision"`。 段数不对或某段非整数 / 为负抛 FormatException。

### static Version? TryParse(string s)

`src/libraries/z42.core/src/Version.z42:99`

不抛异常的解析：成功返回 Version，任何非法输入返回 null。

### int CompareTo(Version other)

`src/libraries/z42.core/src/Version.z42:110`

按 (major, minor, build, revision) 字典序比较，返回 -1 / 0 / 1。

### bool Equals(Version other)

`src/libraries/z42.core/src/Version.z42:125`

四段全等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Version.z42:133`

内容哈希码。

### override string ToString()

`src/libraries/z42.core/src/Version.z42:144`

`"major.minor[.build[.revision]]"` —— 未设置的尾部段（-1）省略。

