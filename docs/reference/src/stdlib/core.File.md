<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.File

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/File.z42:13`

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

### static extern string ReadAllText(string path)

`src/libraries/z42.core/src/IO/File.z42:16`

整读为 `string`（严格 UTF-8）。

### static extern void WriteAllText(string path, string content)

`src/libraries/z42.core/src/IO/File.z42:20`

覆盖写；文件不存在则创建。

### static extern void AppendAllText(string path, string content)

`src/libraries/z42.core/src/IO/File.z42:24`

追加写；文件不存在则创建。

### static extern byte[] ReadAllBytes(string path)

`src/libraries/z42.core/src/IO/File.z42:28`

一次性读取整个文件为 byte 数组。

### static extern void WriteAllBytes(string path, byte[] data)

`src/libraries/z42.core/src/IO/File.z42:32`

一次性写入 byte 数组到文件（已存在则覆盖；不存在则创建）。

### static extern void WriteAllTextAtomic(string path, string content)

`src/libraries/z42.core/src/IO/File.z42:38`

原子写文本 —— 崩溃安全：外部观察到的是旧内容或新内容，不会是半截。

### static extern void WriteAllBytesAtomic(string path, byte[] data)

`src/libraries/z42.core/src/IO/File.z42:42`

原子写 byte 数组 —— 同 `WriteAllTextAtomic` 但二进制 payload。

### static extern bool Exists(string path)

`src/libraries/z42.core/src/IO/File.z42:46`

路径上是否有东西（目录也返回 true）。

### static extern void Delete(string path)

`src/libraries/z42.core/src/IO/File.z42:50`

删文件；不存在抛 `FileNotFoundException`。

### static extern void Copy(string src, string dst)

`src/libraries/z42.core/src/IO/File.z42:54`

拷贝文件；dst 已存在时覆盖。

### static extern void Move(string src, string dst)

`src/libraries/z42.core/src/IO/File.z42:58`

移动文件；dst 已存在时覆盖。

### static DateTime GetLastWriteTime(string path)

`src/libraries/z42.core/src/IO/File.z42:62`

文件最后修改时间（mtime），包装成 `Std.Time.DateTime`。比较用 `IsAfter` / `IsBefore` / `UnixMs()`。文件不存在抛 `FileNotFoundException`。

### static extern string CreateTempDir(string prefix)

`src/libraries/z42.core/src/IO/File.z42:75`

创建唯一临时目录，返回全路径（`<sysTemp>/prefix.<nanos>.<pid>.<bump>`）。

### static extern string CreateTempFile(string prefix, string suffix)

`src/libraries/z42.core/src/IO/File.z42:80`

创建唯一空临时文件，返回全路径（`<sysTemp>/prefix.<nanos>.<pid>.<bump><suffix>`）。 suffix 可为 ""（无扩展名）。

### static extern void MakeExecutable(string path)

`src/libraries/z42.core/src/IO/File.z42:84`

Unix: `chmod u+x,g+x,o+x`；Windows: no-op（NTFS 由扩展名决定）。

### static extern void Link(string src, string dst)

`src/libraries/z42.core/src/IO/File.z42:88`

创建 hard link `dst → src`（跨设备 OS 错误透传）。

### static extern void SymLink(string src, string dst)

`src/libraries/z42.core/src/IO/File.z42:92`

创建 symbolic link `dst → src`（Windows 暂未支持）。

### static extern long GetSize(string path)

`src/libraries/z42.core/src/IO/File.z42:96`

文件字节数（path 是 directory 时抛错）。

