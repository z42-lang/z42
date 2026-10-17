<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.io/ -->

# z42.io —— 流式 I/O、二进制读写与子进程。

> 包路径 `src/libraries/z42.io/`；命名空间 `Std.IO`

`Std.IO` 提供统一的字节流抽象（Stream / MemoryStream / FileStream / BufferedStream）、字符读写器
（TextReader / TextWriter / StringReader / StringWriter / StreamReader / StreamWriter）、子进程执行
（Process / ProcessHandle / Stdio）与终端 ANSI 着色（Ansi）；`Std.IO.Binary` 提供带显式字节序的
二进制数值读写（BinaryReader / BinaryWriter）。

## 不支持

- 没有 async：不存在 ReadAsync / WriteAsync 等异步读写。
- Stream 本身不实现 Std.IDisposable（只有 Close()）；TextReader / TextWriter 名义实现了 Std.IDisposable。
- 没有超时旋钮：ReadTimeout / WriteTimeout 不存在。
- BinaryReader / BinaryWriter 没有无符号读写，也没有 Close() / Dispose()，生命周期归底层 Stream。
- 子进程不经 shell，Timeout 只管 Run()，Stdio.ToFile 不能用于 stdin。
- Ansi 没有 256 色 / truecolor / 背景色，也没有光标定位 / 清屏的生成接口。

## Ansi

终端 ANSI SGR 着色；启用时把文本包成 ESC[<code>m … ESC[0m，未启用时原样返回输入。

```z42
public static class Ansi {
    static bool Enabled()
    static void SetEnabled(bool b)
    static string Black(string s)
    static string Red(string s)
    static string Green(string s)
    static string Yellow(string s)
    static string Blue(string s)
    static string Magenta(string s)
    static string Cyan(string s)
    static string White(string s)
    static string BrightBlack(string s)
    static string BrightRed(string s)
    static string BrightGreen(string s)
    static string BrightYellow(string s)
    static string BrightBlue(string s)
    static string BrightMagenta(string s)
    static string BrightCyan(string s)
    static string BrightWhite(string s)
    static string Bold(string s)
    static string Dim(string s)
    static string Italic(string s)
    static string Underline(string s)
    static string Reverse(string s)
    static string Strip(string s)
}
```

### static bool Enabled()

是否启用着色；首次调用自动检测（Console.IsTerminal() 且 NO_COLOR 未设置或为空），之后返回缓存值。

### static void SetEnabled(bool b)

显式覆盖；调用过后自动检测对本进程永久失效。

### static string Black(string s)

黑色前景。

### static string Red(string s)

红色前景。

### static string Green(string s)

绿色前景。

### static string Yellow(string s)

黄色前景。

### static string Blue(string s)

蓝色前景。

### static string Magenta(string s)

品红前景。

### static string Cyan(string s)

青色前景。

### static string White(string s)

白色前景。

### static string BrightBlack(string s)

亮黑前景。

### static string BrightRed(string s)

亮红前景。

### static string BrightGreen(string s)

亮绿前景。

### static string BrightYellow(string s)

亮黄前景。

### static string BrightBlue(string s)

亮蓝前景。

### static string BrightMagenta(string s)

亮品红前景。

### static string BrightCyan(string s)

亮青前景。

### static string BrightWhite(string s)

亮白前景。

### static string Bold(string s)

加粗。

### static string Dim(string s)

暗淡。

### static string Italic(string s)

斜体。

### static string Underline(string s)

下划线。

### static string Reverse(string s)

反显。

### static string Strip(string s)

移除所有 CSI 转义序列（ESC [ 到 0x40..0x7E 范围内的终结字节）。

## BufferedStream

把小的读 / 写批量化成对内层流大操作的 Stream 包装；单缓冲，同一时刻只服务读或写。

```z42
public class BufferedStream : Stream {
    BufferedStream(Stream inner)
    BufferedStream(Stream inner, int bufferSize)
    override bool CanRead()
    override bool CanWrite()
    override bool CanSeek()
    override int Read(byte[] buffer, int offset, int count)
    override void Write(byte[] buffer, int offset, int count)
    override long Length()
    override long Position()
    override long Seek(long offset, int origin)
    override void Flush()
    override void Close()
}
```

### BufferedStream(Stream inner)

用默认 4 KB 缓冲包装内层流。

### BufferedStream(Stream inner, int bufferSize)

用指定大小的缓冲包装内层流；bufferSize <= 0 抛 ArgumentException。

### override bool CanRead()

是否可读；关闭后为 false，否则转发给内层流。

### override bool CanWrite()

是否可写；关闭后为 false，否则转发给内层流。

### override bool CanSeek()

是否可定位；关闭后为 false，否则转发给内层流。

### override int Read(byte[] buffer, int offset, int count)

缓冲读；从写切到读会先 flush 待写字节。

### override void Write(byte[] buffer, int offset, int count)

缓冲写；从读切到写会丢弃未消费的读缓冲。

### override long Length()

长度；关闭后抛 InvalidOperationException，否则转发给内层流。

### override long Position()

修正后的当前位置：读模式减去未消费缓冲，写模式加上待写字节。

### override long Seek(long offset, int origin)

先 flush 待写数据、丢弃读缓冲，再转发给内层流定位。

### override void Flush()

flush 待写数据并转发给内层流 Flush。

### override void Close()

先 flush 待写数据再关闭，但不关闭内层流（生命周期归调用方）。

## EndOfStreamException

流在凑满所需字节前就到达 EOF（如 Stream.ReadExactly 不足）时抛出；继承 IOException。

```z42
public class EndOfStreamException : IOException {
    EndOfStreamException(string message)
}
```

### EndOfStreamException(string message)

用指定消息构造。

## ProcessExitException

仅在 ProcessResult.EnsureSuccess() 遇到非零退出码时抛出；Run() 本身不会因退出码抛错。

```z42
public class ProcessExitException : Exception {
    int ExitCode
    string Program
    ProcessExitException(string program, int exitCode, string stderrPreview)
}
```

### int ExitCode

退出码。

### string Program

被执行的程序。

### ProcessExitException(string program, int exitCode, string stderrPreview)

用程序名、退出码与 stderr 预览构造。

## ProcessHandleInvalidException

对已 Wait / TryWait（有结果）/ Dispose 的 ProcessHandle 再操作时抛出；继承 InvalidOperationException。

```z42
public class ProcessHandleInvalidException : InvalidOperationException {
    string Program
    ProcessHandleInvalidException(string program)
}
```

### string Program

被执行的程序。

### ProcessHandleInvalidException(string program)

用程序名构造。

## ProcessStartException

子进程根本没启动（可执行文件找不到、权限不足、fork/exec 失败等）时抛出；继承 IOException。

```z42
public class ProcessStartException : IOException {
    ProcessStartException(string message)
}
```

### ProcessStartException(string message)

用指定消息构造。

## ProcessTimeoutException

Run() 超过 Timeout(ms) 时抛出；子进程连同派生进程树已被杀并回收，无需调用方再清理。

```z42
public class ProcessTimeoutException : TimeoutException {
    string Program
    long TimeoutMs
    ProcessTimeoutException(string program, long timeoutMs)
}
```

### string Program

被执行的程序。

### long TimeoutMs

超时上限（毫秒）。

### ProcessTimeoutException(string program, long timeoutMs)

用程序名与超时上限构造。

## FileMode

FileStream(path, mode) 的打开模式常量。

```z42
public static class FileMode {
    static int Read
    static int Write
    static int Append
}
```

### static int Read

打开已有文件读。

### static int Write

创建或截断写入。

### static int Append

不存在则创建，写入永远落在文件末尾。

## FileStream

OS 文件支撑的流，按 FileMode 打开，生命周期由本对象持有（Close() 释放 OS 句柄）。

```z42
public class FileStream : Stream {
    FileStream(string path)
    FileStream(string path, int mode)
    override bool CanRead()
    override bool CanWrite()
    override bool CanSeek()
    override int Read(byte[] buffer, int offset, int count)
    override void Write(byte[] buffer, int offset, int count)
    override long Length()
    override long Position()
    override long Seek(long offset, int origin)
    override void Flush()
    override void Close()
}
```

### FileStream(string path)

以 FileMode.Read 打开已有文件（默认模式）。

### FileStream(string path, int mode)

以指定 FileMode（Read / Write / Append）打开文件。

### override bool CanRead()

是否可读；Read 模式下为 true。

### override bool CanWrite()

是否可写；Write / Append 模式下为 true。

### override bool CanSeek()

是否可定位；Append 模式为 false（POSIX O_APPEND 会把每次写强制拉到 EOF），其余为 true。

### override int Read(byte[] buffer, int offset, int count)

从文件读取字节；关闭或非读模式抛相应异常。

### override void Write(byte[] buffer, int offset, int count)

向文件写入字节；关闭或读模式抛相应异常。

### override long Length()

文件字节长度。

### override long Position()

当前读写位置。

### override long Seek(long offset, int origin)

相对 origin 定位；结果位置为负抛 ArgumentException，越过文件末尾允许（写入时稀疏扩展）。

### override void Flush()

刷新到 OS；已关闭时静默返回。

### override void Close()

幂等关闭；关闭后 Read / Write / Length / Position / Seek 抛 InvalidOperationException。

## MemoryStream

byte[] 支撑的内存流，支持「空且可增长可写」与「对已有数组的只读视图」两种构造模式。

```z42
public class MemoryStream : Stream {
    MemoryStream()
    MemoryStream(byte[] data)
    override bool CanRead()
    override bool CanWrite()
    override bool CanSeek()
    override int Read(byte[] buffer, int offset, int count)
    override void Write(byte[] buffer, int offset, int count)
    override long Length()
    override long Position()
    override long Seek(long offset, int origin)
    override void Flush()
    override void Close()
    byte[] ToArray()
}
```

### MemoryStream()

构造空、可写、可增长的内存流（初始容量 16，超出时 2× 增长）。

### MemoryStream(byte[] data)

构造 data 的只读视图，不复制原数组；修改原数组会反映到流中。

### override bool CanRead()

是否可读；恒为 true。

### override bool CanWrite()

是否可写；只读视图为 false。

### override bool CanSeek()

是否可定位；恒为 true。

### override int Read(byte[] buffer, int offset, int count)

从当前位置填充 buffer[offset..offset+count]，返回实际读到的字节数，0 = EOF。

### override void Write(byte[] buffer, int offset, int count)

从当前位置写入 buffer[offset..offset+count]；只读视图抛 NotSupportedException。

### override long Length()

当前内容的字节长度。

### override long Position()

当前读写位置。

### override long Seek(long offset, int origin)

相对 origin 定位；结果位置为负或超过 int 上限时抛 ArgumentException。

### override void Flush()

内存流无缓冲，空操作。

### override void Close()

关闭并释放内部缓冲引用；此后读写将抛错。

### byte[] ToArray()

返回当前 [0, Length) 内容的独立副本，与内部存储互不影响。

## Process

builder 形态的子进程执行入口：argv 数组进出、不经 shell，stdio 三路各自可配。

```z42
public class Process {
    Process(string program)
    Process Arg(string a)
    Process Args(string[] xs)
    Process WorkingDirectory(string d)
    Process Env(string k, string v)
    Process EnvRemove(string k)
    Process ClearEnv()
    Process Stdin(Stdio s)
    Process Stdout(Stdio s)
    Process Stderr(Stdio s)
    Process StdinBytes(byte[] b)
    Process Timeout(long milliseconds)
    Process ShareProcessGroup()
    ProcessResult Run()
    static string? Which(string name)
    ProcessHandle Spawn()
}
```

### Process(string program)

指定可执行文件名或路径；名字不含分隔符时按 $PATH 查找。

### Process Arg(string a)

追加一个 argv；不经 shell，特殊字符不会被展开。

### Process Args(string[] xs)

批量追加 argv。

### Process WorkingDirectory(string d)

设置子进程工作目录；不设则继承调用方。

### Process Env(string k, string v)

在继承的环境之上覆盖 / 新增一项。

### Process EnvRemove(string k)

从继承的环境里删掉一项。

### Process ClearEnv()

完全不继承调用方环境。

### Process Stdin(Stdio s)

配置 stdin；默认 Null()。

### Process Stdout(Stdio s)

配置 stdout；默认 Pipe()。

### Process Stderr(Stdio s)

配置 stderr；默认 Pipe()。

### Process StdinBytes(byte[] b)

一次性喂给子进程 stdin 的负载，写完即关；必须配 Stdin(Stdio.Pipe()) 才生效。

### Process Timeout(long milliseconds)

设置超时（毫秒，-1 不限时）；超时抛 ProcessTimeoutException，只对 Run() 生效。

### Process ShareProcessGroup()

让子进程留在调用方的进程组里；仅交互式 tty 透传时需要。

### ProcessResult Run()

同步跑到结束，返回 ProcessResult；退出码非 0 不抛（要 fail-fast 调 EnsureSuccess()）。

### static string? Which(string name)

按 $PATH（Windows 上还按 PATHEXT）定位可执行文件，返回解析到的路径，找不到返回 null。

### ProcessHandle Spawn()

立刻返回 ProcessHandle，子进程在后台跑；启动失败抛 ProcessStartException。

## ProcessHandle

Spawn() 返回的活子进程句柄；实现 Std.IDisposable，一次性的（Wait / Dispose 后即失效）。

```z42
public class ProcessHandle : IDisposable {
    ProcessHandle(string program, long slotId)
    long Pid()
    ProcessResult Wait()
    ProcessResult? TryWait()
    void Kill()
    void KillForce()
    void WriteStdin(byte[] bytes)
    void WriteStdinString(string s)
    void CloseStdin()
    int ReadStdout(byte[] buffer, int offset, int count)
    int ReadStderr(byte[] buffer, int offset, int count)
    ProcessStdinStream GetStdinStream()
    ProcessOutputStream GetStdoutStream()
    ProcessOutputStream GetStderrStream()
    void Dispose()
}
```

### ProcessHandle(string program, long slotId)

由 Process.Spawn() 内部构造。

### long Pid()

OS 进程号。

### ProcessResult Wait()

阻塞到子进程结束，返回 ProcessResult；调用之后句柄即失效。

### ProcessResult? TryWait()

还在跑返回 null，已结束返回 ProcessResult 并使句柄失效。

### void Kill()

强制终止子进程（当前与 KillForce 行为相同）。

### void KillForce()

强制终止子进程（当前与 Kill 行为相同）。

### void WriteStdin(byte[] bytes)

写子进程 stdin；管道写满阻塞，子进程已关 stdin 抛 IOException。

### void WriteStdinString(string s)

等价 WriteStdin(Utf8.GetBytes(s))。

### void CloseStdin()

关闭 stdin，子进程读到 EOF；幂等。

### int ReadStdout(byte[] buffer, int offset, int count)

阻塞读子进程 stdout，返回实际读到的字节数，0 = EOF。

### int ReadStderr(byte[] buffer, int offset, int count)

阻塞读子进程 stderr，返回实际读到的字节数，0 = EOF。

### ProcessStdinStream GetStdinStream()

返回 stdin 的 Stream 包装；重复调用返回同一对象。

### ProcessOutputStream GetStdoutStream()

返回 stdout 的 Stream 包装；重复调用返回同一对象。

### ProcessOutputStream GetStderrStream()

返回 stderr 的 Stream 包装；重复调用返回同一对象。

### void Dispose()

幂等；子进程仍活着时会被 kill + reap（不留僵尸）。

## ProcessOutputStream

对活子进程 stdout / stderr 管道的只读 Stream 包装（fd 1 = stdout，2 = stderr）。

```z42
public class ProcessOutputStream : Stream {
    static int Stdout
    static int Stderr
    ProcessOutputStream(ProcessHandle handle, int fd)
    override bool CanRead()
    override bool CanWrite()
    override bool CanSeek()
    override int Read(byte[] buffer, int offset, int count)
    override void Flush()
    override void Close()
}
```

### static int Stdout

stdout 的文件描述符编号。

### static int Stderr

stderr 的文件描述符编号。

### ProcessOutputStream(ProcessHandle handle, int fd)

构造指定 fd 的只读包装；fd 只能是 1 或 2。

### override bool CanRead()

是否可读；关闭前为 true。

### override bool CanWrite()

是否可写；恒为 false。

### override bool CanSeek()

是否可定位；恒为 false。

### override int Read(byte[] buffer, int offset, int count)

阻塞读，直到至少一个字节或管道关闭（EOF 返回 0）。

### override void Flush()

只读流无缓冲，空操作。

### override void Close()

仅本侧逻辑关闭；底层管道由 ProcessHandle 负责。

## ProcessResult

Run() 与 Wait() / TryWait() 的返回值；字段是公开可写的普通字段。

```z42
public class ProcessResult {
    string Program
    int ExitCode
    string Stdout
    string Stderr
    byte[] StdoutBytes
    byte[] StderrBytes
    ProcessResult(string program, int exitCode, string stdout, string stderr, byte[] stdoutBytes, byte[] stderrBytes)
    ProcessResult EnsureSuccess()
}
```

### string Program

被执行的程序。

### int ExitCode

退出码；被信号杀死时为 128 + 信号号。

### string Stdout

stdout 的 UTF-8 lossy 解码。

### string Stderr

stderr 的 UTF-8 lossy 解码。

### byte[] StdoutBytes

stdout 原始字节。

### byte[] StderrBytes

stderr 原始字节。

### ProcessResult(string program, int exitCode, string stdout, string stderr, byte[] stdoutBytes, byte[] stderrBytes)

用各字段构造。

### ProcessResult EnsureSuccess()

ExitCode != 0 时抛 ProcessExitException，否则返回 this（可链式 .Run().EnsureSuccess()）。

## ProcessStdinStream

对活子进程 stdin 管道的只写 Stream 包装。

```z42
public class ProcessStdinStream : Stream {
    ProcessStdinStream(ProcessHandle handle)
    override bool CanWrite()
    override bool CanRead()
    override bool CanSeek()
    override void Write(byte[] buffer, int offset, int count)
    override void Flush()
    override void Close()
}
```

### ProcessStdinStream(ProcessHandle handle)

由 ProcessHandle.GetStdinStream() 构造。

### override bool CanWrite()

是否可写；关闭前为 true。

### override bool CanRead()

是否可读；恒为 false。

### override bool CanSeek()

是否可定位；恒为 false。

### override void Write(byte[] buffer, int offset, int count)

写子进程 stdin；未配 Stdin(Stdio.Pipe()) 或子进程已关 stdin 时抛相应异常。

### override void Flush()

本侧管道写无缓冲，空操作。

### override void Close()

关闭子进程的 stdin（子进程读到 EOF）；不回收进程句柄，仍需 Wait()。

## SeekOrigin

Seek 的 origin 参数常量（int 常量而非 enum）。

```z42
public static class SeekOrigin {
    static int Begin
    static int Current
    static int End
}
```

### static int Begin

偏移相对流的开头。

### static int Current

偏移相对当前位置。

### static int End

偏移相对流的末尾（负值往回定位）。

## Stdio

子进程三路标准流各自的配置：四个模式常量（Null / Inherit / Pipe / File）+ 可选路径，不是 enum。

```z42
public sealed class Stdio {
    static int ModeNull
    static int ModeInherit
    static int ModePipe
    static int ModeFile
    Stdio(int mode, string path)
    static Stdio Inherit()
    static Stdio Pipe()
    static Stdio Null()
    static Stdio ToFile(string path)
    int GetMode()
    string GetPath()
}
```

### static int ModeNull

丢弃（/dev/null）。

### static int ModeInherit

直接继承调用方的 fd。

### static int ModePipe

建管道（stdout / stderr 捕获，stdin 可喂数据）。

### static int ModeFile

重定向到文件。

### Stdio(int mode, string path)

由静态工厂内部构造。

### static Stdio Inherit()

直接继承调用方的那个 fd。

### static Stdio Pipe()

建管道。

### static Stdio Null()

丢弃（stdin 立刻 EOF，输出丢弃）。

### static Stdio ToFile(string path)

重定向到文件（只能用于 stdout / stderr）。

### int GetMode()

返回模式常量。

### string GetPath()

返回路径；非 File 模式返回 ""。

## Stream

所有字节流的统一基类；是具体类不是抽象类，能力谓词默认 false、核心方法抛 NotSupportedException。

```z42
public class Stream {
    virtual bool CanRead()
    virtual bool CanWrite()
    virtual bool CanSeek()
    virtual int Read(byte[] buffer, int offset, int count)
    virtual void Write(byte[] buffer, int offset, int count)
    virtual int ReadByte()
    virtual void WriteByte(byte b)
    virtual void Flush()
    virtual void Close()
    virtual long Length()
    virtual long Position()
    virtual long Seek(long offset, int origin)
    byte[] ReadAllBytes()
    void WriteAllBytes(byte[] data)
    byte[] ReadExactly(int count)
    void CopyTo(Stream dest)
    void CopyTo(Stream dest, int bufferSize)
}
```

### virtual bool CanRead()

是否可读；基类返回 false。

### virtual bool CanWrite()

是否可写；基类返回 false。

### virtual bool CanSeek()

是否可定位；基类返回 false。

### virtual int Read(byte[] buffer, int offset, int count)

填充 buffer[offset..offset+count]，返回实际写入的字节数（0..count），0 = EOF。

### virtual void Write(byte[] buffer, int offset, int count)

写 buffer[offset..offset+count]。

### virtual int ReadByte()

读一个字节，返回 0..255；EOF 返回 -1。

### virtual void WriteByte(byte b)

写一个字节。

### virtual void Flush()

刷新缓冲；基类无缓冲状态，默认空操作。

### virtual void Close()

释放资源；基类无资源持有，默认空操作。

### virtual long Length()

流的字节长度；基类抛 NotSupportedException。

### virtual long Position()

当前读写位置；基类抛 NotSupportedException。

### virtual long Seek(long offset, int origin)

把游标移到相对 origin（SeekOrigin.Begin / Current / End）偏移 offset 处，返回新的绝对位置。

### byte[] ReadAllBytes()

读到 EOF 为止，返回精确长度的新 byte[]（内部按 4 KB 分块累积）。

### void WriteAllBytes(byte[] data)

等价 Write(data, 0, data.Length)。

### byte[] ReadExactly(int count)

必须凑满 count 字节并作为新 byte[] 返回，否则抛 Std.IO.EndOfStreamException。

### void CopyTo(Stream dest)

把剩余内容全部灌入 dest（默认 4 KB 中转缓冲），两端都不关闭。

### void CopyTo(Stream dest, int bufferSize)

用指定 bufferSize 的中转缓冲拷贝；bufferSize <= 0 抛 ArgumentException。

## StreamReader

字节流 ↔ 字符的桥；首次读时一次性抽干源流并整体解码，默认 UTF-8，行为与 StringReader 一致。

```z42
public class StreamReader {
    StreamReader(Stream source)
    StreamReader(Stream source, Encoding encoding)
    int Peek()
    int Read()
    int Read(char[] buffer, int offset, int count)
    string? ReadLine()
    string ReadToEnd()
    void Close()
}
```

### StreamReader(Stream source)

默认 UTF-8 编码。

### StreamReader(Stream source, Encoding encoding)

指定编码。

### int Peek()

窥视下一个字符码；EOF 返回 -1。

### int Read()

消费一个字符码；EOF 返回 -1。

### int Read(char[] buffer, int offset, int count)

填充 buffer[offset..offset+count]，返回实际填入数，0 = EOF。

### string? ReadLine()

读一行（不含终止符），EOF 返回 null。

### string ReadToEnd()

返回剩余全部内容；EOF 返回 ""。

### void Close()

标记关闭并释放缓存字符串；不关闭被包装的源流。

## StreamWriter

字节流 ↔ 字符的桥；每次 Write 立即编码并推给目标流，默认 UTF-8。

```z42
public class StreamWriter {
    StreamWriter(Stream dest)
    StreamWriter(Stream dest, Encoding encoding)
    void Write(string s)
    void Write(char[] buffer, int offset, int count)
    void WriteLine()
    void WriteLine(string s)
    void Flush()
    void Close()
}
```

### StreamWriter(Stream dest)

默认 UTF-8 编码。

### StreamWriter(Stream dest, Encoding encoding)

指定编码。

### void Write(string s)

编码并写入字符串。

### void Write(char[] buffer, int offset, int count)

编码并写入字符区间 buffer[offset..offset+count]。

### void WriteLine()

写 "\n"。

### void WriteLine(string s)

写 s + "\n"。

### void Flush()

转发给 dest.Flush()。

### void Close()

先 Flush 再标记关闭；不关闭被包装的目标流。

## StringReader

在一个 string 上做游标式字符读取（char 粒度，不是 Stream 的子类）。

```z42
public class StringReader {
    StringReader(string source)
    int Peek()
    int Read()
    int Read(char[] buffer, int offset, int count)
    string? ReadLine()
    string ReadToEnd()
    void Close()
}
```

### StringReader(string source)

在 source 上构造游标式读取器。

### int Peek()

窥视下一个字符码，不前进游标；EOF 返回 -1。

### int Read()

消费下一个字符码；EOF 返回 -1（游标不变）。

### int Read(char[] buffer, int offset, int count)

填充 buffer[offset..offset+count]，返回实际填入数，0 = EOF。

### string? ReadLine()

读一行（不含终止符）；\n / \r\n / 单独 \r 都终止一行，游标已在 EOF 时返回 null。

### string ReadToEnd()

返回从当前游标到末尾的剩余内容；EOF 返回 ""。

### void Close()

标记关闭（幂等）；关闭后任何读操作抛 InvalidOperationException。

## StringWriter

累积到内部 Std.Text.StringBuilder 的字符写入器。

```z42
public class StringWriter {
    StringWriter()
    StringWriter(int initialCapacity)
    void Write(string s)
    void Write(char[] buffer, int offset, int count)
    void WriteLine()
    void WriteLine(string s)
    void Clear()
    void Close()
}
```

### StringWriter()

构造空写入器。

### StringWriter(int initialCapacity)

initialCapacity 仅为 API 形状兼容的提示，当前被忽略。

### void Write(string s)

追加字符串。

### void Write(char[] buffer, int offset, int count)

追加字符区间 buffer[offset..offset+count]。

### void WriteLine()

写 "\n"。

### void WriteLine(string s)

写 s + "\n"。

### void Clear()

清空，后续写入从头开始。

### void Close()

标记关闭（幂等）；关闭后 Write / WriteLine / Clear 抛 InvalidOperationException，ToString() 仍可用。

## TextReader

供自定义字符源继承的读取器基类；核心方法抛桩，带 Base 后缀的是组合在 Read() 上的默认实现。

```z42
public class TextReader : IDisposable {
    virtual int Peek()
    virtual int Read()
    virtual void Close()
    virtual void Dispose()
    int ReadBufferBase(char[] buffer, int offset, int count)
    string? ReadLineBase()
    string ReadToEndBase()
}
```

### virtual int Peek()

窥视下一个字符码；基类抛 NotSupportedException。

### virtual int Read()

读一个字符码；基类抛 NotSupportedException。

### virtual void Close()

基类无资源持有，默认空操作。

### virtual void Dispose()

等价 Close()。

### int ReadBufferBase(char[] buffer, int offset, int count)

默认批量读：循环 Read()，返回实际填入数（0 = EOF）。

### string? ReadLineBase()

默认行读取：\n、\r\n 与单独 \r 都终止一行，空行返回 ""，EOF 返回 null。

### string ReadToEndBase()

默认读剩余全部：循环 Read() 直到 EOF。

## TextWriter

供自定义字符目标继承的写入器基类；核心方法抛桩，带 Base 后缀的是组合在 Write(string) 上的默认实现。

```z42
public class TextWriter : IDisposable {
    virtual void Write(string s)
    virtual void Flush()
    virtual void Close()
    virtual void Dispose()
    void WriteCharsBase(char[] buffer, int offset, int count)
    void WriteCharBase(char c)
    void WriteLineBase()
    void WriteLineBase(string s)
}
```

### virtual void Write(string s)

写字符串；基类抛 NotSupportedException。

### virtual void Flush()

基类无缓冲状态，默认空操作。

### virtual void Close()

基类无资源持有，默认空操作。

### virtual void Dispose()

等价 Close()。

### void WriteCharsBase(char[] buffer, int offset, int count)

默认批量写字符：写 buffer[offset..offset+count]。

### void WriteCharBase(char c)

默认写单个字符。

### void WriteLineBase()

写 "\n"（不分平台）。

### void WriteLineBase(string s)

写 s + "\n"（不分平台）。

