<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Path

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/Path.z42:8`

纯字符串的路径拼接与解析（`/` 分隔）。不解析符号链接、不要求路径存在。

```z42
public static class Path {
    static char Separator
    static bool IsRooted(string path)
    static string Normalize(string path)
    static string Join(string a, string b)
    static string Join(params string[] parts)
    static string GetFileName(string path)
    static string GetExtension(string path)
    static string GetFileNameWithoutExtension(string path)
    static string GetDirectoryName(string path)
    static extern string[] Glob(string dir, string pattern)
    static string[] GlobRecursive(string dir, string pattern)
}
```

### static char Separator

`src/libraries/z42.core/src/IO/Path.z42:10`

平台分隔符常量（恒为 '/'，对应 BCL `Path.DirectorySeparatorChar`）。

### static bool IsRooted(string path)

`src/libraries/z42.core/src/IO/Path.z42:13`

是否为绝对路径：Unix `/` 开头，或 Windows 根（盘符 `X:\`/`X:/`/`X:` 与 `\` 开头）。

### static string Normalize(string path)

`src/libraries/z42.core/src/IO/Path.z42:28`

词法规范化：折叠重复分隔符、去掉 `.` 段、抵消 `..` 段、去掉尾随 `/`，并把 `\` 统一换成 `/`。纯词法不碰文件系统（不解析符号链接、不要求路径存在）。越过根的 `..` 在根处停住； 相对路径开头保留不了的 `..` 原样保留；结果为空时返回 `.`（相对）或 `/`（绝对）。

### static string Join(string a, string b)

`src/libraries/z42.core/src/IO/Path.z42:70`

拼接两段路径：b 是绝对路径则替换 a；否则用 `/` 分隔（a 已有尾随分隔符时不重复）。

### static string Join(params string[] parts)

`src/libraries/z42.core/src/IO/Path.z42:80`

多段路径拼接：逐段左折叠复用 2-arg 语义（绝对段替换、尾分隔去重）。

### static string GetFileName(string path)

`src/libraries/z42.core/src/IO/Path.z42:96`

返回路径最后一段（filename）。末尾分隔符被跳过；路径全是分隔符时返回 ""。

### static string GetExtension(string path)

`src/libraries/z42.core/src/IO/Path.z42:112`

返回 filename 中最后一个 `.` 之后的部分（不含 `.`）。`.bashrc` / `foo` / "" → ""。

### static string GetFileNameWithoutExtension(string path)

`src/libraries/z42.core/src/IO/Path.z42:127`

返回 filename 去除最后扩展名（保留 leading dot 的整体名）。

### static string GetDirectoryName(string path)

`src/libraries/z42.core/src/IO/Path.z42:142`

返回路径父目录。无分隔符时返回 ""；根目录返回其分隔符段。

### static extern string[] Glob(string dir, string pattern)

`src/libraries/z42.core/src/IO/Path.z42:166`

在 dir 下匹配直接子项（不递归）。pattern 支持 `*`（任意序列）与 `?`（单字符）， 大小写敏感，不应含 `/`。返回排好序的全路径数组。

### static string[] GlobRecursive(string dir, string pattern)

`src/libraries/z42.core/src/IO/Path.z42:170`

在 dir 下递归找所有匹配 pattern 的文件，返回绝对路径排序数组。pattern 支持 `*`（跨 `/`） 与 `?`；不带 `/` 时只匹配 basename，带 `/` 时匹配相对 dir 的完整子路径。不支持 `**`。

