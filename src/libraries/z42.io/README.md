# z42.io — IO 库

## 职责

流式 I/O（Stream / 文本读写 / 二进制读写）、子进程、ANSI 着色。
不含 `Console` / `File` / `Directory` / `Path` / `Environment`——这些基础 IO 门面位于
`z42.core/src/IO/`（prelude，无需声明依赖）；本包在其上提供流抽象与进程子系统。

## 功能索引

| 功能 | 入口 |
|------|------|
| 字节流抽象与实现 | `Stream` / `MemoryStream` / `FileStream` / `BufferedStream`（`Std.IO`） |
| 字符读写 | `TextReader` / `TextWriter` 基类；`StringReader` / `StringWriter`；`StreamReader` / `StreamWriter`（经 `Encoding` 编解码） |
| 二进制读写 | `Std.IO.Binary.BinaryReader` / `BinaryWriter`（LE / BE 显式后缀 + varint + float/double） |
| 子进程 | `Process`（启动 / 等待 / kill / stdin 写入）+ `ProcessHandle` / `ProcessResult` / `Stdio`；`Process.Which(name)`；`ShareProcessGroup()` |
| ANSI 着色 | `Ansi.Red(s)` / `Bold` / `BrightGreen` …，自动检测 TTY + `NO_COLOR`，`Strip(s)` 去 escape |

二进制读写的设计要点见 `docs/reference/src/stdlib/io-binary.md`。

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `Stream.z42` | `Stream` | 流 base class（capability + Read/Write/Seek + ReadAllBytes / WriteAllBytes / ReadExactly） |
| `MemoryStream.z42` | `MemoryStream` | `byte[]`-backed Stream（可写可增长 / 只读视图 + `ToArray()`） |
| `FileStream.z42` | `FileStream` | OS 文件 Stream（Read / Write / Append，走 `VmCore.file_handles` slot table；`Seek` 拒绝负位置） |
| `BufferedStream.z42` | `BufferedStream` | 单缓冲包装，合并小读写（默认 4 KB） |
| `FileMode.z42` / `SeekOrigin.z42` | 常量类 | `FileStream` 构造模式（Read / Write / Append）/ `Seek` origin（Begin / Current / End） |
| `TextReader.z42` / `TextWriter.z42` | `TextReader` / `TextWriter` | 字符读写基类（`IDisposable`） |
| `StringReader.z42` / `StringWriter.z42` | `StringReader` / `StringWriter` | 内存字符串读 / 累积写 |
| `StreamReader.z42` / `StreamWriter.z42` | `StreamReader` / `StreamWriter` | 字节 Stream 之上经 `Encoding` 的字符读写 |
| `BinaryReader.z42` / `BinaryWriter.z42` | `BinaryReader` / `BinaryWriter`（namespace `Std.IO.Binary`） | 低层二进制读 / 写；`new …` 或 `OverStream(stream)` |
| `BinaryException.z42` | `BinaryException`（namespace `Std`） | 二进制越界 / 非法参数 |
| `Process.z42` | `Process` | 子进程 builder + 启动；`Which`；`ShareProcessGroup()` 让子进程留在调用方进程组（交互式 tty 透传必需；默认独立进程组以便超时树杀） |
| `ProcessHandle.z42` / `ProcessResult.z42` | 进程句柄 / 结果 | 等待 / kill / `WriteStdin(byte[])` / `WriteStdinString(string)` / 退出结果 |
| `Stdio.z42` | `Stdio` | 子进程 stdio 配置（Null / Inherit / Pipe / File） |
| `ProcessStdinStream.z42` / `ProcessOutputStream.z42` | Stream 子类 | 子进程 stdin 管道只写流 / stdout·stderr 只读流 |
| `Ansi.z42` | `Ansi` | ANSI SGR 包裹 |
| `Exceptions/` | 异常 | `EndOfStreamException` / `Process{Exit,HandleInvalid,Start,Timeout}Exception` |

## 如何测试验证

```bash
xtask test stdlib z42.io    # 本库全部 [Test]（含 core/IO 门面的 console / file / directory / path 等用例）
```

## 依赖关系
`z42.core` + `z42.encoding`（`StreamReader/Writer` 的 `Encoding`）+ `z42.text`（`StringWriter` 用 `StringBuilder`）。
