<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.File

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/File.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L13)

一次性（whole-file）文件操作；需要按块读写时用 z42.io 的 `FileStream`。

失败按 .NET 口径抛（都是 `IOException` 或 `Exception` 的子类）：文件不存在 →
`FileNotFoundException`；目录（路径的一部分）不存在 → `DirectoryNotFoundException`；
无权限 / 把目录当文件读写 → `UnauthorizedAccessException`；路径过长 → `PathTooLongException`；
其它 OS 错 → `IOException`（消息带路径）；null 实参 → `ArgumentNullException`；空路径 →
`ArgumentException`。与 .NET 的出入：`Delete` 删不存在的文件抛 `FileNotFoundException`
（.NET 静默）；`Copy` / `Move` 覆盖已存在的目标（.NET 两参版本抛 `IOException`）。

```z42
public static class File {
    static extern string ReadAllText(string path)
    static extern void WriteAllText(string path, string content)
    static extern void AppendAllText(string path, string content)
    static extern byte[] ReadAllBytes(string path)
    static extern void WriteAllBytes(string path, byte[] data)
    static extern void WriteAllTextAtomic(string path, string content)
    static extern void WriteAllBytesAtomic(string path, byte[] data)
    static extern bool Exists(string path)
    static extern void Delete(string path)
    static extern void Copy(string src, string dst)
    static extern void Move(string src, string dst)
    static DateTime GetLastWriteTime(string path)
    static extern string CreateTempDir(string prefix)
    static extern string CreateTempFile(string prefix, string suffix)
    static extern void MakeExecutable(string path)
    static extern void Link(string src, string dst)
    static extern void SymLink(string src, string dst)
    static extern long GetSize(string path)
}
```

成员：[`ReadAllText`](#Std.IO.File.ReadAllText(string)) · [`WriteAllText`](#Std.IO.File.WriteAllText(string,string)) · [`AppendAllText`](#Std.IO.File.AppendAllText(string,string)) · [`ReadAllBytes`](#Std.IO.File.ReadAllBytes(string)) · [`WriteAllBytes`](#Std.IO.File.WriteAllBytes(string,byte[])) · [`WriteAllTextAtomic`](#Std.IO.File.WriteAllTextAtomic(string,string)) · [`WriteAllBytesAtomic`](#Std.IO.File.WriteAllBytesAtomic(string,byte[])) · [`Exists`](#Std.IO.File.Exists(string)) · [`Delete`](#Std.IO.File.Delete(string)) · [`Copy`](#Std.IO.File.Copy(string,string)) · [`Move`](#Std.IO.File.Move(string,string)) · [`GetLastWriteTime`](#Std.IO.File.GetLastWriteTime(string)) · [`CreateTempDir`](#Std.IO.File.CreateTempDir(string)) · [`CreateTempFile`](#Std.IO.File.CreateTempFile(string,string)) · [`MakeExecutable`](#Std.IO.File.MakeExecutable(string)) · [`Link`](#Std.IO.File.Link(string,string)) · [`SymLink`](#Std.IO.File.SymLink(string,string)) · [`GetSize`](#Std.IO.File.GetSize(string))

<a id="Std.IO.File.ReadAllText(string)"></a>
### static extern string ReadAllText(string path)

[`src/libraries/z42.core/src/IO/File.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L16)

整读为 `string`（严格 UTF-8）。

<a id="Std.IO.File.WriteAllText(string,string)"></a>
### static extern void WriteAllText(string path, string content)

[`src/libraries/z42.core/src/IO/File.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L20)

覆盖写；文件不存在则创建。

<a id="Std.IO.File.AppendAllText(string,string)"></a>
### static extern void AppendAllText(string path, string content)

[`src/libraries/z42.core/src/IO/File.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L24)

追加写；文件不存在则创建。

<a id="Std.IO.File.ReadAllBytes(string)"></a>
### static extern byte[] ReadAllBytes(string path)

[`src/libraries/z42.core/src/IO/File.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L28)

一次性读取整个文件为 byte 数组。

<a id="Std.IO.File.WriteAllBytes(string,byte[])"></a>
### static extern void WriteAllBytes(string path, byte[] data)

[`src/libraries/z42.core/src/IO/File.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L32)

一次性写入 byte 数组到文件（已存在则覆盖；不存在则创建）。

<a id="Std.IO.File.WriteAllTextAtomic(string,string)"></a>
### static extern void WriteAllTextAtomic(string path, string content)

[`src/libraries/z42.core/src/IO/File.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L38)

原子写文本 —— 崩溃安全：外部观察到的是旧内容或新内容，不会是半截。

<a id="Std.IO.File.WriteAllBytesAtomic(string,byte[])"></a>
### static extern void WriteAllBytesAtomic(string path, byte[] data)

[`src/libraries/z42.core/src/IO/File.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L42)

原子写 byte 数组 —— 同 `WriteAllTextAtomic` 但二进制 payload。

<a id="Std.IO.File.Exists(string)"></a>
### static extern bool Exists(string path)

[`src/libraries/z42.core/src/IO/File.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L46)

路径上是否有东西（目录也返回 true）。

<a id="Std.IO.File.Delete(string)"></a>
### static extern void Delete(string path)

[`src/libraries/z42.core/src/IO/File.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L50)

删文件；不存在抛 `FileNotFoundException`。

<a id="Std.IO.File.Copy(string,string)"></a>
### static extern void Copy(string src, string dst)

[`src/libraries/z42.core/src/IO/File.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L54)

拷贝文件；dst 已存在时覆盖。

<a id="Std.IO.File.Move(string,string)"></a>
### static extern void Move(string src, string dst)

[`src/libraries/z42.core/src/IO/File.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L58)

移动文件；dst 已存在时覆盖。

<a id="Std.IO.File.GetLastWriteTime(string)"></a>
### static DateTime GetLastWriteTime(string path)

[`src/libraries/z42.core/src/IO/File.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L62)

文件最后修改时间（mtime），包装成 `Std.Time.DateTime`。比较用 `IsAfter` / `IsBefore` / `UnixMs()`。文件不存在抛 `FileNotFoundException`。

<a id="Std.IO.File.CreateTempDir(string)"></a>
### static extern string CreateTempDir(string prefix)

[`src/libraries/z42.core/src/IO/File.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L75)

创建唯一临时目录，返回全路径（`<sysTemp>/prefix.<nanos>.<pid>.<bump>`）。

<a id="Std.IO.File.CreateTempFile(string,string)"></a>
### static extern string CreateTempFile(string prefix, string suffix)

[`src/libraries/z42.core/src/IO/File.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L80)

创建唯一空临时文件，返回全路径（`<sysTemp>/prefix.<nanos>.<pid>.<bump><suffix>`）。 suffix 可为 ""（无扩展名）。

<a id="Std.IO.File.MakeExecutable(string)"></a>
### static extern void MakeExecutable(string path)

[`src/libraries/z42.core/src/IO/File.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L84)

Unix: `chmod u+x,g+x,o+x`；Windows: no-op（NTFS 由扩展名决定）。

<a id="Std.IO.File.Link(string,string)"></a>
### static extern void Link(string src, string dst)

[`src/libraries/z42.core/src/IO/File.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L88)

创建 hard link `dst → src`（跨设备 OS 错误透传）。

<a id="Std.IO.File.SymLink(string,string)"></a>
### static extern void SymLink(string src, string dst)

[`src/libraries/z42.core/src/IO/File.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L92)

创建 symbolic link `dst → src`（Windows 暂未支持）。

<a id="Std.IO.File.GetSize(string)"></a>
### static extern long GetSize(string path)

[`src/libraries/z42.core/src/IO/File.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/File.z42#L96)

文件字节数（path 是 directory 时抛错）。

