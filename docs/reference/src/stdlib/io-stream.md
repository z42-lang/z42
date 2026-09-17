# z42.io —— 字节流与字符读写器

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.io/`；命名空间 `Std.IO`（异常类型在 `Std`）

`Std.IO.Stream` 是所有字节流的统一基类：内存、文件、缓冲、子进程管道、压缩管道
都是它的子类，可以互相嵌套组合。字符侧另有一套 `StringReader` / `StringWriter` /
`StreamReader` / `StreamWriter`，负责 `char` ↔ `string` 的行读写，并通过
[`Std.Encoding.Encoding`](encoding.md) 与字节流互转。

注意 `Std.IO` 这个命名空间**跨两个包**：本页的流体系在 `z42.io`；同命名空间下的
`Console` / `File` / `Directory` / `Path` / `Environment` 在 `z42.core`
（`src/libraries/z42.core/src/IO/`），见[控制台与文件](io-file.md)；同在 `z42.io` 的
子进程执行与 ANSI 着色见[子进程与终端](process.md)。二进制数值读写器
`BinaryReader` / `BinaryWriter` 也在 `z42.io`，但命名空间是 `Std.IO.Binary`，
见 [io-binary](io-binary.md)。

## Stream

字节流基类。**是具体类不是抽象类**（z42 暂无 `abstract`）——`new Stream()` 能构造，
但所有能力谓词返回 `false`、核心方法抛 `NotSupportedException`。

```z42
public class Stream {
    // 能力谓词（基类全部 false，子类按实现覆写）
    public virtual bool CanRead();
    public virtual bool CanWrite();
    public virtual bool CanSeek();

    // 核心读写（基类抛 NotSupportedException）
    public virtual int  Read(byte[] buffer, int offset, int count);
    public virtual void Write(byte[] buffer, int offset, int count);
    public virtual int  ReadByte();          // 0..255；-1 表示 EOF
    public virtual void WriteByte(byte b);

    // 生命周期（基类 no-op）
    public virtual void Flush();
    public virtual void Close();

    // 位置（基类抛 NotSupportedException）
    public virtual long Length();
    public virtual long Position();
    public virtual long Seek(long offset, int origin);

    // 便利方法（基于 Read / Write 组合，非 virtual，子类不覆写）
    public byte[] ReadAllBytes();
    public void   WriteAllBytes(byte[] data);
    public byte[] ReadExactly(int count);
    public void   CopyTo(Stream dest);
    public void   CopyTo(Stream dest, int bufferSize);
}
```

| 成员 | 说明 |
|---|---|
| `Read(buffer, offset, count)` | 填充调用方的 buffer，返回实际写入的字节数（`0..count`）；**返回 0 = EOF**。一次调用可能少于 `count`，循环读是调用方的责任 |
| `Write(buffer, offset, count)` | 写 `buffer[offset..offset+count]` |
| `ReadByte()` | 读一个字节，返回 `0..255`；**EOF 返回 -1**（不是 0） |
| `Seek(offset, origin)` | `origin` 取 `SeekOrigin.Begin / Current / End`，返回新的绝对位置 |
| `ReadAllBytes()` | 读到 EOF 为止，返回精确长度的新 `byte[]`；内部按 4 KB 分块累积。不可读时抛 `NotSupportedException` |
| `WriteAllBytes(data)` | 等价 `Write(data, 0, data.Length)`。不可写时抛 `NotSupportedException` |
| `ReadExactly(count)` | 必须凑满 `count` 字节，否则抛 `Std.EndOfStreamException`；`count < 0` 抛 `ArgumentException` |
| `CopyTo(dest)` / `CopyTo(dest, bufferSize)` | 把本流剩余内容全部灌入 `dest`，默认 4 KB 中转缓冲。**两端都不关闭**；`bufferSize <= 0` 抛 `ArgumentException` |

越界参数（`offset < 0` / `count < 0` / `offset + count > buffer.Length`）在各具体子类的
`Read` / `Write` 里统一抛 `ArgumentException`。

### SeekOrigin

```z42
public static class SeekOrigin {
    public static int Begin   = 0;
    public static int Current = 1;
    public static int End     = 2;
}
```

是三个 `int` 常量而不是 `enum`，所以 `Seek` 的 `origin` 形参类型就是 `int`；
传入其它值时具体子类抛 `ArgumentException`。

## MemoryStream

`byte[]` 支撑的内存流，两种构造模式。

```z42
public class MemoryStream : Stream {
    public MemoryStream();               // 空、可写、可增长（初始容量 16，2× 增长）
    public MemoryStream(byte[] data);    // 只读视图，不复制 data
    public byte[] ToArray();             // 快照当前 [0, Length) 的独立副本
}
```

| 能力 | `MemoryStream()` | `MemoryStream(byte[])` |
|---|---|---|
| `CanRead` / `CanSeek` | `true` | `true` |
| `CanWrite` | `true` | `false`（写抛 `NotSupportedException`） |

- `MemoryStream(byte[] data)` **不复制** `data`：构造之后修改原数组，流读出来的内容随之变化。
  需要隔离就先自己拷一份，或读完用 `ToArray()` 取快照。
- 可以 `Seek` 到 `Length` 之后：位置立刻生效，`Length` 不变；下一次 `Write` 会把中间的空洞
  按 0 填上，`Length` 随之跳到写入末尾。
- `Seek` 结果为负、或超过 `int` 上限（2147483647）时抛 `ArgumentException`——内部位置是
  32 位，越界会静默回绕，所以直接拒绝。
- `ToArray()` 返回的数组与流内部存储彼此独立，之后的写入互不影响。

## FileStream / FileMode

OS 文件支撑的流。

```z42
public static class FileMode {
    public static int Read   = 0;
    public static int Write  = 1;
    public static int Append = 2;
}

public class FileStream : Stream {
    public FileStream(string path);              // 等价 FileMode.Read
    public FileStream(string path, int mode);
}
```

| `mode` | 打开语义 | `CanRead` | `CanWrite` | `CanSeek` |
|---|---|---|---|---|
| `FileMode.Read` | 打开已有文件读；路径不存在时构造即失败 | `true` | `false` | `true` |
| `FileMode.Write` | 创建或截断；原有内容丢失 | `false` | `true` | `true` |
| `FileMode.Append` | 不存在则创建，写入永远落在文件末尾 | `false` | `true` | `false` |

- `mode` 不是这三个值时构造抛 `ArgumentException`；打开失败（路径不存在、权限不足、
  父目录缺失）抛基类 `Std.Exception`，message 是 OS 原文（`No such file or directory
  (os error 2)`），不做二次包装。
- `Append` 模式下 `CanSeek()` 是 `false` 且 `Seek` 抛 `NotSupportedException`——
  POSIX `O_APPEND` 会把每次写强制拉到 EOF，报告可定位会误导调用方。
- `Close()` 幂等；关闭后 `Read` / `Write` / `Length` / `Position` / `Seek` 抛
  `InvalidOperationException`，`Flush()` 和再次 `Close()` 静默返回。
- `Seek` 的结果位置为负时抛 `ArgumentException`；越过文件末尾是允许的（后续写入稀疏扩展）。

## BufferedStream

把小的 `Read` / `Write` 批量化成对内层流的大操作。

```z42
public class BufferedStream : Stream {
    public BufferedStream(Stream inner);                  // 默认 4 KB 缓冲
    public BufferedStream(Stream inner, int bufferSize);  // bufferSize <= 0 抛 ArgumentException
}
```

- **单缓冲**：同一时刻只服务读或只服务写。从写切到读会先 flush 待写字节；从读切到写会
  **丢弃**尚未消费的读缓冲（需要位置一致就自己先 `Seek`）。
- `count >= bufferSize` 的大读 / 大写**绕过缓冲**直通内层流（大写之前会先 flush 已积压的小写）。
- `CanRead` / `CanWrite` / `CanSeek` / `Length` 转发给内层流；关闭后三个能力谓词一律 `false`。
- `Position()` 会做修正：读模式下减去缓冲里未消费的字节，写模式下加上待写的字节。
- `Seek` 先 flush 待写数据、丢弃读缓冲，再转发给内层流。
- `Close()` 先 flush 待写数据，**但不关闭内层流**——内层流的生命周期归调用方。关闭后再调
  `Read` / `Write` / `Seek` / `Length` / `Position` 抛 `InvalidOperationException`。

## 子进程管道流

由 `ProcessHandle` 的 `GetStdinStream()` / `GetStdoutStream()` / `GetStderrStream()`
返回，也可以直接构造：

```z42
public class ProcessStdinStream : Stream {
    public ProcessStdinStream(ProcessHandle handle);
}

public class ProcessOutputStream : Stream {
    public static int Stdout = 1;
    public static int Stderr = 2;
    public ProcessOutputStream(ProcessHandle handle, int fd);   // fd 只能是 1 或 2
}
```

- `ProcessStdinStream`：只写（`CanWrite` 在 `Close()` 前为 `true`，另两个能力恒 `false`）。
  `Close()` 关闭子进程的 stdin，让子进程读到 EOF；**不回收进程句柄**，仍需 `Wait()`。
  如果进程不是用 `Stdio.Pipe()` 启动的 stdin，首次 `Write` 抛
  `Std.ProcessHandleInvalidException`。
- `ProcessOutputStream`：只读。`Read` 阻塞到至少有一个字节或管道关闭（EOF 返回 0）。
  `fd` 不是 1 / 2 时构造抛 `ArgumentException`。`Close()` 只是本侧逻辑关闭，
  底层管道由 `ProcessHandle` 负责。
- 用流式读取抽干输出之后，`Wait()` 得到的 `ProcessResult` 里的 stdout / stderr
  只剩管道中残留的部分（通常为空）。

## 字符读写器

字符侧是 `char` / `string` 粒度，**不是 `Stream` 的子类**。

### TextReader / TextWriter

供自定义字符源使用的可继承基类，同样是「具体类 + 抛桩」：

```z42
public class TextReader {
    public virtual int  Peek();     // 基类抛 NotSupportedException
    public virtual int  Read();     // 基类抛 NotSupportedException
    public virtual void Close();    // 基类 no-op
    public virtual void Dispose();  // 等价 Close()

    // 非 virtual 默认实现，组合在 Read() / Peek() 之上
    public int    ReadBufferBase(char[] buffer, int offset, int count);
    public string ReadLineBase();
    public string ReadToEndBase();
}

public class TextWriter {
    public virtual void Write(string s);  // 基类抛 NotSupportedException
    public virtual void Flush();          // 基类 no-op
    public virtual void Close();          // 基类 no-op
    public virtual void Dispose();        // 等价 Close()

    public void WriteCharsBase(char[] buffer, int offset, int count);
    public void WriteCharBase(char c);
    public void WriteLineBase();
    public void WriteLineBase(string s);
}
```

只有 `Peek` / `Read` / `Write(string)` / `Flush` / `Close` / `Dispose` 是 `virtual`。
`ReadLineBase` / `ReadToEndBase` / `WriteLineBase` 等带 `Base` 后缀的是**非虚**的默认实现，
名字带后缀是为了让子类能另外声明自己的同名重载而不撞车。
`WriteLineBase` 无论平台一律写 `\n`。

**标准库里的 `StringReader` / `StreamReader` / `StringWriter` / `StreamWriter`
并不继承这两个基类**，它们是形状相同的独立类。要写一个能被当作 `TextReader` 传递的
自定义读取器，就自己 `: TextReader` 继承并覆写 `Peek()` / `Read()`。

### StringReader

在一个 `string` 上做游标式字符读取。

```z42
public class StringReader {
    public StringReader(string source);
    public int    Peek();                                   // 下一个 char 码；EOF 返回 -1
    public int    Read();                                   // 消费一个 char；EOF 返回 -1
    public int    Read(char[] buffer, int offset, int count);  // 返回实际填入数；0 = EOF
    public string ReadLine();                               // 行内容；EOF 返回 null
    public string ReadToEnd();                              // 剩余全部；EOF 返回 ""
    public void   Close();
}
```

行终止符规则：`\n`、`\r\n`、单独的 `\r` 三者都终止一行，**返回值里不含终止符**。
空行返回 `""`，末尾没有终止符的最后一行照样完整返回。`ReadLine()` 只有在调用时
游标已在 EOF 才返回 `null`——所以「`null` 才停」是正确的循环条件。

`Close()` 幂等；关闭后任何读操作抛 `InvalidOperationException`。

### StringWriter

累积到内部 `Std.Text.StringBuilder`。

```z42
public class StringWriter {
    public StringWriter();
    public StringWriter(int initialCapacity);   // 形参当前被忽略
    public void   Write(string s);
    public void   Write(char[] buffer, int offset, int count);
    public void   WriteLine();                  // 写 "\n"
    public void   WriteLine(string s);          // s + "\n"
    public void   Clear();                      // 清空，后续写入从头开始
    public void   Close();
    override string ToString();                 // 当前内容快照
}
```

`ToString()` 返回的字符串与后续写入无关。`Close()` 幂等；关闭后 `Write` / `WriteLine` /
`Clear` 抛 `InvalidOperationException`（`ToString()` 仍可用）。

### StreamReader / StreamWriter

字节流 ↔ 字符的桥。编码默认 UTF-8。

```z42
public class StreamReader {
    public StreamReader(Stream source);
    public StreamReader(Stream source, Encoding encoding);
    public int    Peek();
    public int    Read();
    public int    Read(char[] buffer, int offset, int count);
    public string ReadLine();     // EOF 返回 null
    public string ReadToEnd();
    public void   Close();
}

public class StreamWriter {
    public StreamWriter(Stream dest);
    public StreamWriter(Stream dest, Encoding encoding);
    public void Write(string s);
    public void Write(char[] buffer, int offset, int count);
    public void WriteLine();          // 写 "\n"
    public void WriteLine(string s);  // s + "\n"
    public void Flush();              // 转发给 dest.Flush()
    public void Close();
}
```

- `StreamReader` 在**第一次读操作**时把源流一次性读空（`ReadAllBytes()`）再整体解码，
  之后的读都在内存字符串上走。行为上与 `StringReader` 完全一致（含上面的行终止符规则），
  代价是源流的全部内容会在第一次读返回前驻留内存——适合配置文件、日志切片，
  不适合无界流或超大文件。
- `StreamWriter` 每次 `Write` 立即编码并推给目标流，不额外缓冲字符。
- 两者的 `Close()` **都不关闭**被包装的 `Stream`——生命周期归调用方。关闭后再操作抛
  `InvalidOperationException`；`StreamWriter.Close()` 会先 `Flush()`。

## 异常

| 类型（均在 `Std`） | 抛出场景 |
|---|---|
| `NotSupportedException` | 对不具备该能力的流调用 `Read` / `Write` / `Seek` / `Length` / `Position` |
| `ArgumentException` | `offset` / `count` 越界、`bufferSize <= 0`、未知 `SeekOrigin`、`Seek` 结果为负 |
| `InvalidOperationException` | 对已 `Close()` 的 `FileStream` / `BufferedStream` / 字符读写器继续操作 |
| `EndOfStreamException` | `Stream.ReadExactly(count)` 在凑满前遇到 EOF |
| `Exception`（基类本身） | 文件打开 / 读写的底层失败，message 是 OS 原文，例如 `No such file or directory (os error 2)` |

**没有 `IOException` 这个类型**：文件层面的失败（路径不存在、权限不足）抛的就是基类
`Std.Exception`，只能按 message 区分。

```z42
public class EndOfStreamException : Exception {
    public EndOfStreamException(string message);
}
```

## 用法

```z42
using Std;
using Std.IO;
using Std.Encoding;

void Main() {
    // 内存流：写入 → 快照
    MemoryStream ms = new MemoryStream();
    StreamWriter w = new StreamWriter(ms);
    w.WriteLine("第一行");
    w.WriteLine("第二行");
    w.Close();                       // 不会关闭 ms

    // 只读视图 + 逐行读
    StreamReader r = new StreamReader(new MemoryStream(ms.ToArray()));
    while (true) {
        string line = r.ReadLine();
        if (line == null) { break; }
        Console.WriteLine($"[{line}]");
    }
    r.Close();

    // 文件：先写一份，再缓冲读回
    FileStream out1 = new FileStream("data.bin", FileMode.Write);
    try { out1.WriteAllBytes(ms.ToArray()); } finally { out1.Close(); }

    FileStream fs = new FileStream("data.bin");
    try {
        BufferedStream bs = new BufferedStream(fs);
        byte[] head = bs.ReadExactly(4);       // 不足 4 字节抛 EndOfStreamException
        Console.WriteLine($"head={head.Length} total={fs.Length()}");
    } finally {
        fs.Close();                            // BufferedStream 不负责关内层
    }

    // 流对拷
    FileStream src = new FileStream("data.bin");
    FileStream dst = new FileStream("copy.bin", FileMode.Write);
    try { src.CopyTo(dst); } finally { src.Close(); dst.Close(); }
}
```

## 不支持

- **没有 `abstract`**：`Stream` / `TextReader` / `TextWriter` 都可以被 `new` 出来，
  误用只能在运行期以 `NotSupportedException` 暴露。
- **不实现 `Std.IDisposable`，也没有 `using (...)` 语句**：一律手写 `Close()`，配
  `try` / `finally`。`TextReader.Dispose()` / `TextWriter.Dispose()` 只是 `Close()`
  的普通别名方法，不代表实现了那个接口。
- **没有 async**：不存在 `ReadAsync` / `WriteAsync`。
- **没有超时旋钮**：`ReadTimeout` / `WriteTimeout` 不存在。
- **没有 `ObjectDisposedException`**：关闭后的误用要么抛 `InvalidOperationException`，
  要么（`MemoryStream`）根本不报错——`MemoryStream.Close()` 是空操作，关闭后流依旧可读可写。
- **`StreamReader` 不做分块解码**：见上面的一次性 drain 说明；`Encoding` 也没有
  流式 `Decoder` 接口。
- **`StringWriter(int initialCapacity)` 的容量被忽略**（`StringBuilder` 还没有带容量的构造）。
- **字符写入器没有 `Write(char)` / `WriteLine(char)` 重载**：写单个字符用
  `Write(c.ToString())` 或 `Write(chars, 0, 1)`。
- **`FileMode` 只有 `Read` / `Write` / `Append`**：没有 `Create` / `OpenOrCreate` /
  `Truncate`，也没有读写双向模式。
- **`BufferedStream` 不能同时缓冲读和写**（单缓冲，切方向即 flush 或丢弃）。
