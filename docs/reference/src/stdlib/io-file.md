# z42.core —— 控制台、文件、目录与路径

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/src/IO/`；命名空间 `Std.IO`

脚本层最常用的一组静态类：`Console` 读写标准流、`File` / `Directory` 做一次性文件与目录
操作、`Path` 做纯字符串的路径拼接与解析、`Environment` 读命令行参数与环境变量。
它们属于 `z42.core`，**隐式加载**，单文件 `z42 run x.z42` 里直接 `using Std.IO;` 就能用。

`Std.IO` 这个命名空间跨两个包：本页是 `z42.core` 那一半。同命名空间下的流体系
（`Stream` / `FileStream` / `MemoryStream` / 字符读写器）在 `z42.io`，见[流](io-stream.md)；
子进程与终端着色（`Process` / `Ansi`）也在 `z42.io`，见[子进程与终端](process.md)。
`z42.io` 的类型**只能在声明了依赖的工程里用**，单文件模式解析不到。

失败行为有一条贯穿全页的规则：**这里没有 `IOException` 类型**。文件 / 目录 / 环境类的底层
失败一律抛基类 `Std.Exception`，`Message` 是 OS 原文（`No such file or directory (os error 2)`
之类），只能按 message 区分。

## Console / ConsoleError

标准输出与标准错误。两个并列的静态类——`ConsoleError` 不是 `Console` 的嵌套成员。

```z42
public static class Console {
    public static extern void   WriteLine(object value);
    public static extern void   Write(object value);
    public static extern string ReadLine();
    public static extern bool   IsTerminal();
}

public static class ConsoleError {
    public static extern void WriteLine(object value);
    public static extern void Write(object value);
    public static extern bool IsTerminal();
}
```

| 成员 | 说明 |
|---|---|
| `WriteLine(value)` | 写 `value` 并换行。**只有 `object` 这一个重载**，任何类型都能直接传 |
| `Write(value)` | 同上，不换行 |
| `Console.ReadLine()` | 从 stdin 读一行，**去掉行尾的 `\n` / `\r`** |
| `IsTerminal()` | 对应流是否连着 tty；重定向到管道或文件时 `false` |

### 打印出来到底长什么样

- `null` 打印成 `null`；数组打印成 `[a, null]` 这种带方括号的形式。
- `double` 的整数值**不带小数点**：`Console.WriteLine(1.0)` 输出 `1`，`1.5` 输出 `1.5`。
- **`Write` / `WriteLine` 不走你写的 `ToString()` 覆写**：自定义类打印成 `TypeName{...}`。
  同样地 `"" + obj` 也不走覆写。要用自己的格式，就显式写 `obj.ToString()`，或者用**字符串插值**
  `$"{obj}"`——插值会调用覆写。

### ReadLine 与 EOF

`ReadLine()` 的返回类型是**非 nullable 的 `string`**，**EOF 返回 `""` 而不是 `null`**。
空行同样是 `""`，所以**无法区分「读到一个空行」和「输入已经结束」**。stdin 关闭后再读仍然
一直返回 `""`，用 `while (Console.ReadLine() != null)` 写的循环不会停。需要判 EOF 时，
只能自己按协议（行数、哨兵行）收敛，或者改用[流](io-stream.md)侧的读取器。

### 颜色与光标

`Console` 本身**没有**颜色 / 光标 / 清屏 API。ANSI 着色由 `z42.io` 的 `Ansi`
提供，见[子进程与终端](process.md#ansi)。`Console.IsTerminal()` 正是 `Ansi` 自动
检测所依赖的那个判断，你也可以自己拿它来决定要不要输出转义序列。

## File

一次性（whole-file）文件操作。需要按块读写或定位时用[流](io-stream.md)的 `FileStream`。

```z42
public static class File {
    // 文本
    public static extern string ReadAllText(string path);
    public static extern void   WriteAllText(string path, string content);
    public static extern void   AppendAllText(string path, string content);
    public static extern void   WriteAllTextAtomic(string path, string content);

    // 二进制
    public static extern byte[] ReadAllBytes(string path);
    public static extern void   WriteAllBytes(string path, byte[] data);
    public static extern void   WriteAllBytesAtomic(string path, byte[] data);

    // 存在性与增删改
    public static extern bool Exists(string path);
    public static extern void Delete(string path);
    public static extern void Copy(string src, string dst);
    public static extern void Move(string src, string dst);

    // 元信息
    public static extern long GetSize(string path);
    public static DateTime    GetLastWriteTime(string path);

    // 临时文件
    public static extern string CreateTempDir(string prefix);
    public static extern string CreateTempFile(string prefix, string suffix);

    // 权限与链接
    public static extern void MakeExecutable(string path);
    public static extern void Link(string src, string dst);       // hard link
    public static extern void SymLink(string src, string dst);    // symbolic link
}
```

| 成员 | 说明 |
|---|---|
| `ReadAllText(path)` | 整读为 `string`，**严格 UTF-8**：内容不是合法 UTF-8 时抛 `Std.Exception`，message `stream did not contain valid UTF-8`（不做 lossy 替换） |
| `WriteAllText(path, content)` | 覆盖写；文件不存在则创建。UTF-8 编码 |
| `AppendAllText(path, content)` | 追加写；文件不存在则创建 |
| `WriteAllTextAtomic` / `WriteAllBytesAtomic` | 崩溃安全写：外部观察到的要么是旧内容要么是新内容，不会是半截。代价是每次多一次 `fsync`，只对关键文件用 |
| `ReadAllBytes` / `WriteAllBytes` | 字节版，不做任何编码校验 |
| `Exists(path)` | ⚠ **目录也返回 `true`**（见下） |
| `Copy(src, dst)` / `Move(src, dst)` | `dst` 已存在时**静默覆盖**，不抛错 |
| `GetSize(path)` | 字节数。`path` 是目录时抛 `Std.Exception`，message 形如 `File.GetSize: '<path>' is a directory` |
| `GetLastWriteTime(path)` | mtime，包装成 [`Std.Time.DateTime`](time.md)；比较用 `IsAfter` / `IsBefore` / `UnixMs()` |
| `CreateTempDir(prefix)` | 在系统临时根下建唯一目录，返回全路径。basename 形如 `<prefix>.<16 位十六进制>.<pid>.<序号>` |
| `CreateTempFile(prefix, suffix)` | 建唯一空文件，返回全路径；basename 是上面的形式再加 `suffix`（可传 `""`） |
| `MakeExecutable(path)` | Unix 加三组执行位；Windows 上是 no-op |
| `Link` / `SymLink` | 建 `dst → src` 的硬链接 / 符号链接。`SymLink` 在 Windows 上不支持 |

**`File.Exists` 对目录返回 `true`**——它判的是「这个路径上有东西」，不是「这是一个普通文件」。
要区分，配合 `Directory.Exists`（那个反过来只对目录为 `true`）：

```z42
bool isRegularFile = File.Exists(p) && !Directory.Exists(p);
```

**临时文件要手动清理**：z42 没有 RAII / finalizer，`CreateTempFile` / `CreateTempDir`
建出来的东西不会自己消失，用完自己 `File.Delete` / `Directory.Delete(path, true)`。

## Directory

```z42
public static class Directory {
    public static extern bool     Exists(string path);
    public static extern void     Create(string path);
    public static extern void     Delete(string path, bool recursive);
    public static extern string[] Enumerate(string path);
    public static extern string[] EnumerateRecursive(string path);
    public static string          CreateTempDir(string prefix);
    public static void            Copy(string src, string dst, bool recursive);
}
```

| 成员 | 说明 |
|---|---|
| `Exists(path)` | 只有「存在**且是目录**」才 `true`；路径是普通文件时 `false` |
| `Create(path)` | 等价 `mkdir -p`：递归建中间目录，**目录已存在不报错**。但路径上已有同名**文件**时抛 `Std.Exception`（`File exists (os error 17)`） |
| `Delete(path, recursive)` | `recursive=false` 只能删空目录，非空时抛 `Std.Exception`（`Directory not empty (os error 66)`）；`recursive=true` 相当于 `rm -rf` |
| `Enumerate(path)` | **只列直接子项**，含文件和子目录，返回 **basename**（不是全路径） |
| `EnumerateRecursive(path)` | 深度全展开，返回**相对 `path` 的子路径**（`n1/n2/deep.txt`）。**中间目录本身也是一条**（`n1`、`n1/n2` 都会出现） |
| `CreateTempDir(prefix)` | 与 `File.CreateTempDir` 同义 |
| `Copy(src, dst, recursive)` | 递归拷贝。`dst` 不存在则创建；`recursive=false` 时跳过子目录只拷文件 |

**顺序不保证**（取决于 OS）。需要稳定顺序就自己排，或改用 `Path.Glob` / `Path.GlobRecursive`
（那两个返回排好序的结果）。

`Directory.Copy` 的 `dst` 不能位于 `src` 内部——会自指无限递归，调用方自己避开。

## Path

**纯字符串操作**，不碰文件系统：不解析符号链接，也不要求路径存在。

```z42
public static class Path {
    public static char Separator = '/';

    public static bool   IsRooted(string path);
    public static string Normalize(string path);
    public static string Join(string a, string b);
    public static string Join(params string[] parts);
    public static string GetFileName(string path);
    public static string GetExtension(string path);
    public static string GetFileNameWithoutExtension(string path);
    public static string GetDirectoryName(string path);

    public static extern string[] Glob(string dir, string pattern);
    public static string[]        GlobRecursive(string dir, string pattern);
}
```

`Separator` 恒为 `'/'`。解析类方法（`GetFileName` / `GetDirectoryName` 及其派生）
**同时把 `/` 和 `\` 当分隔符**，所以 Windows 风格路径也能拆。

### Join —— 纯拼接，不规范化

```z42
public static string Join(string a, string b);
public static string Join(params string[] parts);   // 逐段左折叠，语义同 2-arg
```

| 调用 | 结果 | 规则 |
|---|---|---|
| `Join("a", "b")` | `a/b` | 普通拼接 |
| `Join("a/", "b")` | `a/b` | `a` 已有尾分隔符则不重复 |
| `Join("a\\", "b")` | `a\b` | 尾 `\` 同样算分隔符，且**原样保留** |
| `Join("a//", "b")` | `a//b` | 只看最后一个字符，不折叠重复分隔符 |
| `Join("a", "/b")` | `/b` | ⚠ **`b` 是绝对路径就整个吃掉 `a`** |
| `Join("a", "C:/b")` | `C:/b` | Windows 盘符也算绝对 |
| `Join("a", "")` | `a` | 空段被忽略 |
| `Join("", "b")` | `b` | |
| `Join("a", ".")` | `a/.` | ⚠ **不做任何规范化**，`.` 和 `..` 原样留在结果里 |
| `Join("a", "b", "c")` | `a/b/c` | params 版逐段左折叠 |
| `Join("a", "/b", "c")` | `/b/c` | 中途的绝对段照样吃掉前面 |
| `Join(new string[0])` | `""` | 空数组返回空串 |

拼出来的路径带着 `.` / `..` 会一路渗进派生路径（`.../proj/./artifacts/...`），要干净结果就
显式再调一次 `Normalize`。

### Normalize —— 词法规范化

折叠重复分隔符、去掉 `.` 段、抵消 `..` 段、去掉尾随 `/`，并把 `\` 统一换成 `/`。

| 输入 | 输出 |
|---|---|
| `a/./b` | `a/b` |
| `a//b` | `a/b` |
| `a/b/../c` | `a/c` |
| `a/b/` | `a/b` |
| `a\b` | `a/b` |
| `/a/../..` | `/` （越过根的 `..` 在根处停住） |
| `../a` | `../a` （相对路径开头的 `..` 无可抵消，保留） |
| `../../a/..` | `../..` |
| `a/..` | `.` |
| `""` | `.` |
| `.` | `.` |
| `/` | `/` |
| `C:/a/../b` | `C:/b` （盘符前缀原样保留） |
| `C:` | `C:` |

### IsRooted

`true` 的情形：以 `/` 开头、以 `\` 开头、或「字母 + `:`」的 Windows 盘符前缀
（`C:\a`、`c:a` 都算）。空串是 `false`，`1:a` 这种非字母前缀也是 `false`。

### 拆解

| 输入 | `GetFileName` | `GetExtension` | `GetFileNameWithoutExtension` | `GetDirectoryName` |
|---|---|---|---|---|
| `/a/b/c.txt` | `c.txt` | `txt` | `c` | `/a/b` |
| `/a/b/` | `b` | `""` | `b` | `/a` |
| `/a` | `a` | `""` | `a` | `/` |
| `/` | `""` | `""` | `""` | `""` |
| `a` | `a` | `""` | `a` | `""` |
| `""` | `""` | `""` | `""` | `""` |
| `a\b.txt` | `b.txt` | `txt` | `b` | `a` |
| `a/b.tar.gz` | `b.tar.gz` | `gz` | `b.tar` | `a` |
| `.bashrc` | `.bashrc` | `""` | `.bashrc` | `""` |
| `foo.` | `foo.` | `""` | `foo` | `""` |

要点：

- **扩展名不含 `.`**，而且只取最后一段（`.tar.gz` 的扩展名是 `gz`）。
- **前导 `.` 不算扩展名分隔符**：`.bashrc` 整体是文件名，扩展名为空。
- **尾随 `.` 也不产出扩展名**：`foo.` 的扩展名是 `""`，而 stem 是 `foo`。
- `GetExtension` 只看最后一段，`a.b/c` 的扩展名是 `""` 而不是 `b`。
- 尾随分隔符被当作目录分隔符跳过：`GetFileName("/a/b/")` 是 `b` 不是 `""`。
- **根的父目录是 `""` 而不是 `/`**：`GetDirectoryName("/")` 返回空串；没有分隔符时也返回 `""`。

### Glob / GlobRecursive

```z42
public static extern string[] Glob(string dir, string pattern);
public static string[]        GlobRecursive(string dir, string pattern);
```

两者都返回**排好序的全路径**（把 `dir` 拼在前面），大小写敏感，通配符只有 `*`（任意序列）
和 `?`（单字符）。

| | `Glob` | `GlobRecursive` |
|---|---|---|
| 范围 | 只看直接子项 | 递归全展开 |
| pattern 里的 `/` | 不应出现 | 出现时匹配**相对 `dir` 的完整子路径**；不出现时只匹配 basename |
| `dir` 不存在 | 返回空数组，不抛错 | 返回空数组，不抛错 |
| 是否匹配到目录 | 会（目录条目也参与匹配） | 会 |

⚠ **`*` 会跨 `/`**：`GlobRecursive(root, "*/x.txt")` 能匹配到 `sub/deeper/x.txt`。
需要「只跨一层」的语义，得自己再过滤。

没有 `**` 通配符——递归版本本身就是全展开。

## Environment

```z42
public static class Environment {
    public static extern string? GetEnvironmentVariable(string name);
    public static extern void    SetEnvironmentVariable(string name, string value);
    public static extern void    UnsetEnvironmentVariable(string name);
    public static extern string[] GetEnvironmentVariables();

    public static extern string[] GetCommandLineArgs();

    public static extern string GetCurrentDirectory();
    public static extern void   SetCurrentDirectory(string path);

    public static extern void Exit(int code);
    public static long        GetCurrentTimeMs();
}
```

| 成员 | 说明 |
|---|---|
| `GetEnvironmentVariable(name)` | 返回 `string?`；变量不存在返回 `null` |
| `SetEnvironmentVariable(name, value)` | 设置本进程的环境变量（会被之后 spawn 的子进程继承） |
| `UnsetEnvironmentVariable(name)` | 删除 |
| `GetEnvironmentVariables()` | 平铺的 `string[]`，每项形如 `"KEY=VALUE"`；调用方自己按**第一个** `=` 切分（没有 `Map<string,string>` 形态） |
| `GetCommandLineArgs()` | 见下 |
| `GetCurrentDirectory()` | `pwd` |
| `SetCurrentDirectory(path)` | `cd`；路径不存在 / 无权限抛 `Std.Exception` |
| `Exit(code)` | 立即以 `code` 退出进程，之后的语句不执行；已写出的输出会刷出 |
| `GetCurrentTimeMs()` | 墙钟 Unix 毫秒 |

**`GetCommandLineArgs()` 只含程序自己的参数**：不含程序名，也不含把它们与 z42 命令行
分开的那个 `--`。

```console
$ z42 run app.z42 -- alpha beta --flag
# GetCommandLineArgs() == ["alpha", "beta", "--flag"]，长度 3
```

没给参数时长度就是 `0`，所以按 C# 习惯写 `args[0]` 当程序名会直接越界。要做
flag / option 解析，用 [`z42.cli` 的 `ArgParser`](cli.md)。

⚠ **非法的变量名会让 VM 直接终止**：`SetEnvironmentVariable("BAD=NAME", "x")` 这种名字里
带 `=` 的调用不会抛可捕获的异常，而是整个进程 abort。名字自己先校验干净。

## 异常

| 类型 | 抛出场景 |
|---|---|
| `Std.Exception`（基类本身） | 本页**全部**的 IO 失败：文件不存在、权限不足、目录非空、不是 UTF-8、`cd` 到不存在的路径…… `Message` 是 OS 原文 |

没有 `IOException` / `FileNotFoundException` / `DirectoryNotFoundException` /
`UnauthorizedAccessException` 这类细分类型，也没有错误码字段。要区分只能匹配 `Message`：

```z42
try {
    string s = File.ReadAllText(p);
} catch (Exception e) {
    if (e.Message.Contains("os error 2")) {
        // 文件不存在
    }
    throw;
}
```

更稳的做法是**先探再读**（`File.Exists` / `Directory.Exists`），把异常留给真正的意外。

## 用法

```z42
using Std;
using Std.IO;

void Main() {
    // 命令行参数（不含程序名）
    string[] args = Environment.GetCommandLineArgs();
    string dir = args.Length > 0 ? args[0] : Environment.GetCurrentDirectory();

    // 递归找源文件并按大小汇总
    long total = 0;
    foreach (var f in Path.GlobRecursive(dir, "*.z42")) {
        total = total + File.GetSize(f);
        Console.WriteLine($"{Path.GetFileName(f)}\t{File.GetSize(f)}");
    }
    ConsoleError.WriteLine($"{total} bytes total");

    // 建一棵输出目录并原子写一份清单
    string outDir = Path.Join(dir, "artifacts", "manifest");
    Directory.Create(outDir);                              // mkdir -p
    File.WriteAllTextAtomic(Path.Join(outDir, "sizes.txt"), $"{total}\n");

    // 临时工作区，用完自己清
    string tmp = File.CreateTempDir("build");
    try {
        File.WriteAllText(Path.Join(tmp, "scratch"), "…");
    } finally {
        Directory.Delete(tmp, true);
    }

    // 拼路径时当心绝对段会吃掉前缀
    Console.WriteLine(Path.Join("/base", "/etc/passwd"));   // => /etc/passwd
    Console.WriteLine(Path.Normalize(Path.Join(dir, ".")));  // => 去掉尾巴上的 "/."
}
```

## 不支持

- **没有 `IOException` 及其家族**：一切文件 / 目录失败都是基类 `Std.Exception`，靠 message 区分。
- **`Console.ReadLine()` 区分不出 EOF**：EOF 与空行都返回 `""`，没有 `null` 也没有别的信号。
- **`Console` 没有颜色 / 光标 / 清屏 / 窗口尺寸**：着色在 `z42.io` 的 `Ansi`（见[子进程与终端](process.md)）。
- **`Console.Write` / `WriteLine` 不调用 `ToString()` 覆写**，也没有 `Write(format, args...)` 重载；
  要格式化就自己用字符串插值。
- **没有 `Console.In` / `Console.Out` / `Console.Error` 这类 `TextWriter` 属性**，
  也不能把标准流重定向到自己的流对象；stderr 的入口是并列的 `ConsoleError` 静态类。
- **`Path` 不解析实际文件系统**：没有 `GetFullPath` / `GetRelativePath` / `realpath`，
  `Normalize` 是纯词法的，不展开符号链接。
- **`Path.Join` 不做规范化**，也不做「绝对路径参数需要显式允许」的保护——绝对段直接吃掉前缀。
- **glob 没有 `**` 通配符**，而且 `*` 不停在 `/` 边界上。
- **`Directory.Enumerate` 不区分文件与目录**，也没有「只列文件」「按模式过滤」「返回全路径」的重载；
  自己配合 `Directory.Exists` / `Path.Join` 处理。
- **没有目录监视（watch）**、没有文件锁、没有 `SetLastWriteTime`、没有权限位读写
  （只有单向的 `MakeExecutable`）。
- **没有 async**：不存在 `ReadAllTextAsync` 之类。
- **没有 `Environment.ProcessId` / `MachineName` / `OSVersion` / `NewLine` / `SpecialFolder`**。
- **环境变量拿不到 `Map<string,string>`**：`GetEnvironmentVariables()` 是 `"KEY=VALUE"` 平铺数组。
