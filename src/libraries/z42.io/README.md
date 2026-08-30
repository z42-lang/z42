# z42.io — IO 库

## 职责

z42 标准 IO 类型。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Console.z42` | `Console` | 标准输入/输出（`ReadLine`、`WriteLine` 等） |
| `Ansi.z42` | `Ansi` | ANSI SGR 包裹：`Ansi.Red(s)` / `Bold` / `BrightGreen` …；自动检测 TTY + `NO_COLOR`，不启用时透传；`Strip(s)` 去 escape 码 |
| `Stdio.z42` | `Stdio` | stdin/stdout/stderr 原语 (`IsTty` 等) |
| `File.z42` | `File` | 文件读写操作（`ReadAllText/Bytes` + `WriteAllText/Bytes` + atomic 写 `WriteAllTextAtomic/WriteAllBytesAtomic` + Link/SymLink/MakeExecutable/CreateTempDir/GetSize） |
| `Directory.z42` | `Directory` | 目录创建 / 列表 / 删除；`CreateTempDir(prefix)` 为 `File.CreateTempDir` 的 alias |
| `Path.z42` | `Path` | 路径拼接和解析；`Glob`（直接子项）+ `GlobRecursive(dir, pattern)` 递归 |
| `Environment.z42` | `Environment` | 环境变量、进程退出 |
| `Process.z42` / `ProcessHandle.z42` / `ProcessResult.z42` | 进程子系统 | 启动 / 等待 / kill / stdin 写入（`WriteStdin(byte[])` + `WriteStdinString(string)`）；静态 `Process.Which(name)` 在 `$PATH` 查可执行；`ShareProcessGroup()` 让子进程留在调用方进程组（交互式 tty 透传必需，如 `z42 repl`；默认独立进程组以便 run-timeout 树杀，fix-repl-launcher-process-group） |
| `ProcessStdinStream.z42` | `ProcessStdinStream` | write-only Stream over a live child stdin pipe (delegates to ProcessHandle.WriteStdin / CloseStdin) |
| `ProcessOutputStream.z42` | `ProcessOutputStream` | read-only Stream over child stdout/stderr (fd-parameterised；backed by `__process_handle_read_*` builtins) |
| `Stream.z42` | `Stream` | 流式 I/O base class（capability + Read/Write/Seek + ReadAllBytes / WriteAllBytes / ReadExactly） |
| `MemoryStream.z42` | `MemoryStream` | `byte[]`-backed Stream（writable + growable / read-only view + `ToArray()`） |
| `FileStream.z42` | `FileStream` | OS-file-backed Stream（Read / Write / Append mode，走 `VmCore.file_handles` slot table；`Seek` 拒绝 resulting position < 0 with `ArgumentException`） |
| `BufferedStream.z42` | `BufferedStream` | single-buffer Stream wrapper batching small Read/Write into larger inner ops（4 KB default） |
| `FileMode.z42` | `FileMode` | `FileStream` 构造模式常量（Read=0 / Write=1 / Append=2） |
| `SeekOrigin.z42` | `SeekOrigin` | `Seek(offset, origin)` origin 常量（Begin=0 / Current=1 / End=2） |
| `StringReader.z42` | `StringReader` | char-oriented reader over an in-memory string（`Peek` / `Read` / `ReadLine` / `ReadToEnd`） |
| `StringWriter.z42` | `StringWriter` | char-oriented writer accumulating into a string（`Write` / `WriteLine` / `ToString` / `Clear`） |
| `StreamReader.z42` | `StreamReader` | char-oriented reader over a byte `Stream` via an `Encoding`（drain-and-decode v0） |
| `StreamWriter.z42` | `StreamWriter` | char-oriented writer over a byte `Stream` via an `Encoding`（encode-on-write） |
| `Exceptions/` | 各类 IO 异常 | `FileNotFoundException` / `ProcessHandleInvalidException` 等 |
