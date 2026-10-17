<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Directory

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/Directory.z42:12`

目录操作（mkdir / rmdir / ls），语义对标 .NET BCL `System.IO.Directory`： - `Create` = `mkdir -p`（递归建中间目录，已存在不报错） - `Delete` = `rmdir` / `rm -rf`（recursive=true 时后者） - `Enumerate` 只列直接子项（含文件 + 子目录），返回 basename - `EnumerateRecursive` 深度优先全展开，返回相对 path 的子路径

顺序不保证（OS-dependent）。失败按 .NET 口径抛 `DirectoryNotFoundException` /
`UnauthorizedAccessException` / `IOException` / `ArgumentException`。

```z42
public static class Directory {
    static extern bool Exists(string path)
    static extern void Create(string path)
    static extern void Delete(string path, bool recursive)
    static extern string[] Enumerate(string path)
    static extern string[] EnumerateRecursive(string path)
    static string CreateTempDir(string prefix)
    static void Copy(string src, string dst, bool recursive)
}
```

### static extern bool Exists(string path)

`src/libraries/z42.core/src/IO/Directory.z42:15`

存在**且是目录**才 true；路径是普通文件时 false。

### static extern void Create(string path)

`src/libraries/z42.core/src/IO/Directory.z42:19`

等价 `mkdir -p`：递归建中间目录，已存在不报错。

### static extern void Delete(string path, bool recursive)

`src/libraries/z42.core/src/IO/Directory.z42:23`

删目录；recursive=false 只能删空目录，true 相当于 `rm -rf`。

### static extern string[] Enumerate(string path)

`src/libraries/z42.core/src/IO/Directory.z42:27`

只列直接子项（含文件与子目录），返回 basename。

### static extern string[] EnumerateRecursive(string path)

`src/libraries/z42.core/src/IO/Directory.z42:31`

深度优先全展开，返回相对 path 的子路径。

### static string CreateTempDir(string prefix)

`src/libraries/z42.core/src/IO/Directory.z42:34`

在系统临时根下建唯一目录，返回全路径（等价 `File.CreateTempDir`）。

### static void Copy(string src, string dst, bool recursive)

`src/libraries/z42.core/src/IO/Directory.z42:40`

递归拷贝目录：dst 不存在则建；src 下的文件拷贝进 dst；src 下的子目录在 recursive=true 时递归拷贝，false 时跳过。dst 不应位于 src 内部（否则自指无限递归）。

