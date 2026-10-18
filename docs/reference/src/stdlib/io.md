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

[`src/libraries/z42.io/src/Ansi.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L6)

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

成员：[`Enabled`](#Std.IO.Ansi.Enabled()) · [`SetEnabled`](#Std.IO.Ansi.SetEnabled(bool)) · [`Black`](#Std.IO.Ansi.Black(string)) · [`Red`](#Std.IO.Ansi.Red(string)) · [`Green`](#Std.IO.Ansi.Green(string)) · [`Yellow`](#Std.IO.Ansi.Yellow(string)) · [`Blue`](#Std.IO.Ansi.Blue(string)) · [`Magenta`](#Std.IO.Ansi.Magenta(string)) · [`Cyan`](#Std.IO.Ansi.Cyan(string)) · [`White`](#Std.IO.Ansi.White(string)) · [`BrightBlack`](#Std.IO.Ansi.BrightBlack(string)) · [`BrightRed`](#Std.IO.Ansi.BrightRed(string)) · [`BrightGreen`](#Std.IO.Ansi.BrightGreen(string)) · [`BrightYellow`](#Std.IO.Ansi.BrightYellow(string)) · [`BrightBlue`](#Std.IO.Ansi.BrightBlue(string)) · [`BrightMagenta`](#Std.IO.Ansi.BrightMagenta(string)) · [`BrightCyan`](#Std.IO.Ansi.BrightCyan(string)) · [`BrightWhite`](#Std.IO.Ansi.BrightWhite(string)) · [`Bold`](#Std.IO.Ansi.Bold(string)) · [`Dim`](#Std.IO.Ansi.Dim(string)) · [`Italic`](#Std.IO.Ansi.Italic(string)) · [`Underline`](#Std.IO.Ansi.Underline(string)) · [`Reverse`](#Std.IO.Ansi.Reverse(string)) · [`Strip`](#Std.IO.Ansi.Strip(string))

<a id="Std.IO.Ansi.Enabled()"></a>
### static bool Enabled()

[`src/libraries/z42.io/src/Ansi.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L19)

是否启用着色；首次调用自动检测（Console.IsTerminal() 且 NO_COLOR 未设置或为空），之后返回缓存值。

<a id="Std.IO.Ansi.SetEnabled(bool)"></a>
### static void SetEnabled(bool b)

[`src/libraries/z42.io/src/Ansi.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L31)

显式覆盖；调用过后自动检测对本进程永久失效。

<a id="Std.IO.Ansi.Black(string)"></a>
### static string Black(string s)

[`src/libraries/z42.io/src/Ansi.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L38)

黑色前景。

<a id="Std.IO.Ansi.Red(string)"></a>
### static string Red(string s)

[`src/libraries/z42.io/src/Ansi.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L40)

红色前景。

<a id="Std.IO.Ansi.Green(string)"></a>
### static string Green(string s)

[`src/libraries/z42.io/src/Ansi.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L42)

绿色前景。

<a id="Std.IO.Ansi.Yellow(string)"></a>
### static string Yellow(string s)

[`src/libraries/z42.io/src/Ansi.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L44)

黄色前景。

<a id="Std.IO.Ansi.Blue(string)"></a>
### static string Blue(string s)

[`src/libraries/z42.io/src/Ansi.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L46)

蓝色前景。

<a id="Std.IO.Ansi.Magenta(string)"></a>
### static string Magenta(string s)

[`src/libraries/z42.io/src/Ansi.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L48)

品红前景。

<a id="Std.IO.Ansi.Cyan(string)"></a>
### static string Cyan(string s)

[`src/libraries/z42.io/src/Ansi.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L50)

青色前景。

<a id="Std.IO.Ansi.White(string)"></a>
### static string White(string s)

[`src/libraries/z42.io/src/Ansi.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L52)

白色前景。

<a id="Std.IO.Ansi.BrightBlack(string)"></a>
### static string BrightBlack(string s)

[`src/libraries/z42.io/src/Ansi.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L56)

亮黑前景。

<a id="Std.IO.Ansi.BrightRed(string)"></a>
### static string BrightRed(string s)

[`src/libraries/z42.io/src/Ansi.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L58)

亮红前景。

<a id="Std.IO.Ansi.BrightGreen(string)"></a>
### static string BrightGreen(string s)

[`src/libraries/z42.io/src/Ansi.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L60)

亮绿前景。

<a id="Std.IO.Ansi.BrightYellow(string)"></a>
### static string BrightYellow(string s)

[`src/libraries/z42.io/src/Ansi.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L62)

亮黄前景。

<a id="Std.IO.Ansi.BrightBlue(string)"></a>
### static string BrightBlue(string s)

[`src/libraries/z42.io/src/Ansi.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L64)

亮蓝前景。

<a id="Std.IO.Ansi.BrightMagenta(string)"></a>
### static string BrightMagenta(string s)

[`src/libraries/z42.io/src/Ansi.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L66)

亮品红前景。

<a id="Std.IO.Ansi.BrightCyan(string)"></a>
### static string BrightCyan(string s)

[`src/libraries/z42.io/src/Ansi.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L68)

亮青前景。

<a id="Std.IO.Ansi.BrightWhite(string)"></a>
### static string BrightWhite(string s)

[`src/libraries/z42.io/src/Ansi.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L70)

亮白前景。

<a id="Std.IO.Ansi.Bold(string)"></a>
### static string Bold(string s)

[`src/libraries/z42.io/src/Ansi.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L74)

加粗。

<a id="Std.IO.Ansi.Dim(string)"></a>
### static string Dim(string s)

[`src/libraries/z42.io/src/Ansi.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L76)

暗淡。

<a id="Std.IO.Ansi.Italic(string)"></a>
### static string Italic(string s)

[`src/libraries/z42.io/src/Ansi.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L78)

斜体。

<a id="Std.IO.Ansi.Underline(string)"></a>
### static string Underline(string s)

[`src/libraries/z42.io/src/Ansi.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L80)

下划线。

<a id="Std.IO.Ansi.Reverse(string)"></a>
### static string Reverse(string s)

[`src/libraries/z42.io/src/Ansi.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L82)

反显。

<a id="Std.IO.Ansi.Strip(string)"></a>
### static string Strip(string s)

[`src/libraries/z42.io/src/Ansi.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Ansi.z42#L85)

移除所有 CSI 转义序列（ESC [ 到 0x40..0x7E 范围内的终结字节）。

## BufferedStream

[`src/libraries/z42.io/src/BufferedStream.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L5)

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

成员：[`BufferedStream`](#Std.IO.BufferedStream.ctor(Stream)) · [`BufferedStream`](#Std.IO.BufferedStream.ctor(Stream,int)) · [`CanRead`](#Std.IO.BufferedStream.CanRead()) · [`CanWrite`](#Std.IO.BufferedStream.CanWrite()) · [`CanSeek`](#Std.IO.BufferedStream.CanSeek()) · [`Read`](#Std.IO.BufferedStream.Read(byte[],int,int)) · [`Write`](#Std.IO.BufferedStream.Write(byte[],int,int)) · [`Length`](#Std.IO.BufferedStream.Length()) · [`Position`](#Std.IO.BufferedStream.Position()) · [`Seek`](#Std.IO.BufferedStream.Seek(long,int)) · [`Flush`](#Std.IO.BufferedStream.Flush()) · [`Close`](#Std.IO.BufferedStream.Close())

<a id="Std.IO.BufferedStream.ctor(Stream)"></a>
### BufferedStream(Stream inner)

[`src/libraries/z42.io/src/BufferedStream.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L22)

用默认 4 KB 缓冲包装内层流。

<a id="Std.IO.BufferedStream.ctor(Stream,int)"></a>
### BufferedStream(Stream inner, int bufferSize)

[`src/libraries/z42.io/src/BufferedStream.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L33)

用指定大小的缓冲包装内层流；bufferSize <= 0 抛 ArgumentException。

<a id="Std.IO.BufferedStream.CanRead()"></a>
### override bool CanRead()

[`src/libraries/z42.io/src/BufferedStream.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L49)

是否可读；关闭后为 false，否则转发给内层流。

<a id="Std.IO.BufferedStream.CanWrite()"></a>
### override bool CanWrite()

[`src/libraries/z42.io/src/BufferedStream.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L55)

是否可写；关闭后为 false，否则转发给内层流。

<a id="Std.IO.BufferedStream.CanSeek()"></a>
### override bool CanSeek()

[`src/libraries/z42.io/src/BufferedStream.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L61)

是否可定位；关闭后为 false，否则转发给内层流。

<a id="Std.IO.BufferedStream.Read(byte[],int,int)"></a>
### override int Read(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/BufferedStream.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L69)

缓冲读；从写切到读会先 flush 待写字节。

<a id="Std.IO.BufferedStream.Write(byte[],int,int)"></a>
### override void Write(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/BufferedStream.z42:110`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L110)

缓冲写；从读切到写会丢弃未消费的读缓冲。

<a id="Std.IO.BufferedStream.Length()"></a>
### override long Length()

[`src/libraries/z42.io/src/BufferedStream.z42:147`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L147)

长度；关闭后抛 InvalidOperationException，否则转发给内层流。

<a id="Std.IO.BufferedStream.Position()"></a>
### override long Position()

[`src/libraries/z42.io/src/BufferedStream.z42:153`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L153)

修正后的当前位置：读模式减去未消费缓冲，写模式加上待写字节。

<a id="Std.IO.BufferedStream.Seek(long,int)"></a>
### override long Seek(long offset, int origin)

[`src/libraries/z42.io/src/BufferedStream.z42:166`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L166)

先 flush 待写数据、丢弃读缓冲，再转发给内层流定位。

<a id="Std.IO.BufferedStream.Flush()"></a>
### override void Flush()

[`src/libraries/z42.io/src/BufferedStream.z42:180`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L180)

flush 待写数据并转发给内层流 Flush。

<a id="Std.IO.BufferedStream.Close()"></a>
### override void Close()

[`src/libraries/z42.io/src/BufferedStream.z42:189`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/BufferedStream.z42#L189)

先 flush 待写数据再关闭，但不关闭内层流（生命周期归调用方）。

## EndOfStreamException

[`src/libraries/z42.io/src/Exceptions/EndOfStreamException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/EndOfStreamException.z42#L4)

流在凑满所需字节前就到达 EOF（如 Stream.ReadExactly 不足）时抛出；继承 IOException。

```z42
public class EndOfStreamException : IOException {
    EndOfStreamException(string message)
}
```

成员：[`EndOfStreamException`](#Std.IO.EndOfStreamException.ctor(string))

<a id="Std.IO.EndOfStreamException.ctor(string)"></a>
### EndOfStreamException(string message)

[`src/libraries/z42.io/src/Exceptions/EndOfStreamException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/EndOfStreamException.z42#L6)

用指定消息构造。

## ProcessExitException

[`src/libraries/z42.io/src/Exceptions/ProcessExitException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessExitException.z42#L4)

仅在 ProcessResult.EnsureSuccess() 遇到非零退出码时抛出；Run() 本身不会因退出码抛错。

```z42
public class ProcessExitException : Exception {
    int ExitCode
    string Program
    ProcessExitException(string program, int exitCode, string stderrPreview)
}
```

成员：[`ExitCode`](#Std.IO.ProcessExitException.ExitCode) · [`Program`](#Std.IO.ProcessExitException.Program) · [`ProcessExitException`](#Std.IO.ProcessExitException.ctor(string,int,string))

<a id="Std.IO.ProcessExitException.ExitCode"></a>
### int ExitCode

[`src/libraries/z42.io/src/Exceptions/ProcessExitException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessExitException.z42#L6)

退出码。

<a id="Std.IO.ProcessExitException.Program"></a>
### string Program

[`src/libraries/z42.io/src/Exceptions/ProcessExitException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessExitException.z42#L8)

被执行的程序。

<a id="Std.IO.ProcessExitException.ctor(string,int,string)"></a>
### ProcessExitException(string program, int exitCode, string stderrPreview)

[`src/libraries/z42.io/src/Exceptions/ProcessExitException.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessExitException.z42#L11)

用程序名、退出码与 stderr 预览构造。

## ProcessHandleInvalidException

[`src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42#L4)

对已 Wait / TryWait（有结果）/ Dispose 的 ProcessHandle 再操作时抛出；继承 InvalidOperationException。

```z42
public class ProcessHandleInvalidException : InvalidOperationException {
    string Program
    ProcessHandleInvalidException(string program)
}
```

成员：[`Program`](#Std.IO.ProcessHandleInvalidException.Program) · [`ProcessHandleInvalidException`](#Std.IO.ProcessHandleInvalidException.ctor(string))

<a id="Std.IO.ProcessHandleInvalidException.Program"></a>
### string Program

[`src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42#L6)

被执行的程序。

<a id="Std.IO.ProcessHandleInvalidException.ctor(string)"></a>
### ProcessHandleInvalidException(string program)

[`src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessHandleInvalidException.z42#L9)

用程序名构造。

## ProcessStartException

[`src/libraries/z42.io/src/Exceptions/ProcessStartException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessStartException.z42#L4)

子进程根本没启动（可执行文件找不到、权限不足、fork/exec 失败等）时抛出；继承 IOException。

```z42
public class ProcessStartException : IOException {
    ProcessStartException(string message)
}
```

成员：[`ProcessStartException`](#Std.IO.ProcessStartException.ctor(string))

<a id="Std.IO.ProcessStartException.ctor(string)"></a>
### ProcessStartException(string message)

[`src/libraries/z42.io/src/Exceptions/ProcessStartException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessStartException.z42#L6)

用指定消息构造。

## ProcessTimeoutException

[`src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42#L4)

Run() 超过 Timeout(ms) 时抛出；子进程连同派生进程树已被杀并回收，无需调用方再清理。

```z42
public class ProcessTimeoutException : TimeoutException {
    string Program
    long TimeoutMs
    ProcessTimeoutException(string program, long timeoutMs)
}
```

成员：[`Program`](#Std.IO.ProcessTimeoutException.Program) · [`TimeoutMs`](#Std.IO.ProcessTimeoutException.TimeoutMs) · [`ProcessTimeoutException`](#Std.IO.ProcessTimeoutException.ctor(string,long))

<a id="Std.IO.ProcessTimeoutException.Program"></a>
### string Program

[`src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42#L6)

被执行的程序。

<a id="Std.IO.ProcessTimeoutException.TimeoutMs"></a>
### long TimeoutMs

[`src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42#L8)

超时上限（毫秒）。

<a id="Std.IO.ProcessTimeoutException.ctor(string,long)"></a>
### ProcessTimeoutException(string program, long timeoutMs)

[`src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Exceptions/ProcessTimeoutException.z42#L11)

用程序名与超时上限构造。

## FileMode

[`src/libraries/z42.io/src/FileMode.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileMode.z42#L4)

FileStream(path, mode) 的打开模式常量。

```z42
public static class FileMode {
    static int Read
    static int Write
    static int Append
}
```

成员：[`Read`](#Std.IO.FileMode.Read) · [`Write`](#Std.IO.FileMode.Write) · [`Append`](#Std.IO.FileMode.Append)

<a id="Std.IO.FileMode.Read"></a>
### static int Read

[`src/libraries/z42.io/src/FileMode.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileMode.z42#L6)

打开已有文件读。

<a id="Std.IO.FileMode.Write"></a>
### static int Write

[`src/libraries/z42.io/src/FileMode.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileMode.z42#L8)

创建或截断写入。

<a id="Std.IO.FileMode.Append"></a>
### static int Append

[`src/libraries/z42.io/src/FileMode.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileMode.z42#L10)

不存在则创建，写入永远落在文件末尾。

## FileStream

[`src/libraries/z42.io/src/FileStream.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L5)

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

成员：[`FileStream`](#Std.IO.FileStream.ctor(string)) · [`FileStream`](#Std.IO.FileStream.ctor(string,int)) · [`CanRead`](#Std.IO.FileStream.CanRead()) · [`CanWrite`](#Std.IO.FileStream.CanWrite()) · [`CanSeek`](#Std.IO.FileStream.CanSeek()) · [`Read`](#Std.IO.FileStream.Read(byte[],int,int)) · [`Write`](#Std.IO.FileStream.Write(byte[],int,int)) · [`Length`](#Std.IO.FileStream.Length()) · [`Position`](#Std.IO.FileStream.Position()) · [`Seek`](#Std.IO.FileStream.Seek(long,int)) · [`Flush`](#Std.IO.FileStream.Flush()) · [`Close`](#Std.IO.FileStream.Close())

<a id="Std.IO.FileStream.ctor(string)"></a>
### FileStream(string path)

[`src/libraries/z42.io/src/FileStream.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L11)

以 FileMode.Read 打开已有文件（默认模式）。

<a id="Std.IO.FileStream.ctor(string,int)"></a>
### FileStream(string path, int mode)

[`src/libraries/z42.io/src/FileStream.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L18)

以指定 FileMode（Read / Write / Append）打开文件。

<a id="Std.IO.FileStream.CanRead()"></a>
### override bool CanRead()

[`src/libraries/z42.io/src/FileStream.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L30)

是否可读；Read 模式下为 true。

<a id="Std.IO.FileStream.CanWrite()"></a>
### override bool CanWrite()

[`src/libraries/z42.io/src/FileStream.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L35)

是否可写；Write / Append 模式下为 true。

<a id="Std.IO.FileStream.CanSeek()"></a>
### override bool CanSeek()

[`src/libraries/z42.io/src/FileStream.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L40)

是否可定位；Append 模式为 false（POSIX O_APPEND 会把每次写强制拉到 EOF），其余为 true。

<a id="Std.IO.FileStream.Read(byte[],int,int)"></a>
### override int Read(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/FileStream.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L47)

从文件读取字节；关闭或非读模式抛相应异常。

<a id="Std.IO.FileStream.Write(byte[],int,int)"></a>
### override void Write(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/FileStream.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L64)

向文件写入字节；关闭或读模式抛相应异常。

<a id="Std.IO.FileStream.Length()"></a>
### override long Length()

[`src/libraries/z42.io/src/FileStream.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L81)

文件字节长度。

<a id="Std.IO.FileStream.Position()"></a>
### override long Position()

[`src/libraries/z42.io/src/FileStream.z42:89`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L89)

当前读写位置。

<a id="Std.IO.FileStream.Seek(long,int)"></a>
### override long Seek(long offset, int origin)

[`src/libraries/z42.io/src/FileStream.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L97)

相对 origin 定位；结果位置为负抛 ArgumentException，越过文件末尾允许（写入时稀疏扩展）。

<a id="Std.IO.FileStream.Flush()"></a>
### override void Flush()

[`src/libraries/z42.io/src/FileStream.z42:128`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L128)

刷新到 OS；已关闭时静默返回。

<a id="Std.IO.FileStream.Close()"></a>
### override void Close()

[`src/libraries/z42.io/src/FileStream.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/FileStream.z42#L134)

幂等关闭；关闭后 Read / Write / Length / Position / Seek 抛 InvalidOperationException。

## MemoryStream

[`src/libraries/z42.io/src/MemoryStream.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L5)

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

成员：[`MemoryStream`](#Std.IO.MemoryStream.ctor()) · [`MemoryStream`](#Std.IO.MemoryStream.ctor(byte[])) · [`CanRead`](#Std.IO.MemoryStream.CanRead()) · [`CanWrite`](#Std.IO.MemoryStream.CanWrite()) · [`CanSeek`](#Std.IO.MemoryStream.CanSeek()) · [`Read`](#Std.IO.MemoryStream.Read(byte[],int,int)) · [`Write`](#Std.IO.MemoryStream.Write(byte[],int,int)) · [`Length`](#Std.IO.MemoryStream.Length()) · [`Position`](#Std.IO.MemoryStream.Position()) · [`Seek`](#Std.IO.MemoryStream.Seek(long,int)) · [`Flush`](#Std.IO.MemoryStream.Flush()) · [`Close`](#Std.IO.MemoryStream.Close()) · [`ToArray`](#Std.IO.MemoryStream.ToArray())

<a id="Std.IO.MemoryStream.ctor()"></a>
### MemoryStream()

[`src/libraries/z42.io/src/MemoryStream.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L12)

构造空、可写、可增长的内存流（初始容量 16，超出时 2× 增长）。

<a id="Std.IO.MemoryStream.ctor(byte[])"></a>
### MemoryStream(byte[] data)

[`src/libraries/z42.io/src/MemoryStream.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L20)

构造 data 的只读视图，不复制原数组；修改原数组会反映到流中。

<a id="Std.IO.MemoryStream.CanRead()"></a>
### override bool CanRead()

[`src/libraries/z42.io/src/MemoryStream.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L30)

是否可读；恒为 true。

<a id="Std.IO.MemoryStream.CanWrite()"></a>
### override bool CanWrite()

[`src/libraries/z42.io/src/MemoryStream.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L32)

是否可写；只读视图为 false。

<a id="Std.IO.MemoryStream.CanSeek()"></a>
### override bool CanSeek()

[`src/libraries/z42.io/src/MemoryStream.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L34)

是否可定位；恒为 true。

<a id="Std.IO.MemoryStream.Read(byte[],int,int)"></a>
### override int Read(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/MemoryStream.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L39)

从当前位置填充 buffer[offset..offset+count]，返回实际读到的字节数，0 = EOF。

<a id="Std.IO.MemoryStream.Write(byte[],int,int)"></a>
### override void Write(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/MemoryStream.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L59)

从当前位置写入 buffer[offset..offset+count]；只读视图抛 NotSupportedException。

<a id="Std.IO.MemoryStream.Length()"></a>
### override long Length()

[`src/libraries/z42.io/src/MemoryStream.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L84)

当前内容的字节长度。

<a id="Std.IO.MemoryStream.Position()"></a>
### override long Position()

[`src/libraries/z42.io/src/MemoryStream.z42:89`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L89)

当前读写位置。

<a id="Std.IO.MemoryStream.Seek(long,int)"></a>
### override long Seek(long offset, int origin)

[`src/libraries/z42.io/src/MemoryStream.z42:94`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L94)

相对 origin 定位；结果位置为负或超过 int 上限时抛 ArgumentException。

<a id="Std.IO.MemoryStream.Flush()"></a>
### override void Flush()

[`src/libraries/z42.io/src/MemoryStream.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L121)

内存流无缓冲，空操作。

<a id="Std.IO.MemoryStream.Close()"></a>
### override void Close()

[`src/libraries/z42.io/src/MemoryStream.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L125)

关闭并释放内部缓冲引用；此后读写将抛错。

<a id="Std.IO.MemoryStream.ToArray()"></a>
### byte[] ToArray()

[`src/libraries/z42.io/src/MemoryStream.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/MemoryStream.z42#L132)

返回当前 [0, Length) 内容的独立副本，与内部存储互不影响。

## Process

[`src/libraries/z42.io/src/Process.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L4)

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

成员：[`Process`](#Std.IO.Process.ctor(string)) · [`Arg`](#Std.IO.Process.Arg(string)) · [`Args`](#Std.IO.Process.Args(string[])) · [`WorkingDirectory`](#Std.IO.Process.WorkingDirectory(string)) · [`Env`](#Std.IO.Process.Env(string,string)) · [`EnvRemove`](#Std.IO.Process.EnvRemove(string)) · [`ClearEnv`](#Std.IO.Process.ClearEnv()) · [`Stdin`](#Std.IO.Process.Stdin(Stdio)) · [`Stdout`](#Std.IO.Process.Stdout(Stdio)) · [`Stderr`](#Std.IO.Process.Stderr(Stdio)) · [`StdinBytes`](#Std.IO.Process.StdinBytes(byte[])) · [`Timeout`](#Std.IO.Process.Timeout(long)) · [`ShareProcessGroup`](#Std.IO.Process.ShareProcessGroup()) · [`Run`](#Std.IO.Process.Run()) · [`Which`](#Std.IO.Process.Which(string)) · [`Spawn`](#Std.IO.Process.Spawn())

<a id="Std.IO.Process.ctor(string)"></a>
### Process(string program)

[`src/libraries/z42.io/src/Process.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L22)

指定可执行文件名或路径；名字不含分隔符时按 $PATH 查找。

<a id="Std.IO.Process.Arg(string)"></a>
### Process Arg(string a)

[`src/libraries/z42.io/src/Process.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L43)

追加一个 argv；不经 shell，特殊字符不会被展开。

<a id="Std.IO.Process.Args(string[])"></a>
### Process Args(string[] xs)

[`src/libraries/z42.io/src/Process.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L50)

批量追加 argv。

<a id="Std.IO.Process.WorkingDirectory(string)"></a>
### Process WorkingDirectory(string d)

[`src/libraries/z42.io/src/Process.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L61)

设置子进程工作目录；不设则继承调用方。

<a id="Std.IO.Process.Env(string,string)"></a>
### Process Env(string k, string v)

[`src/libraries/z42.io/src/Process.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L68)

在继承的环境之上覆盖 / 新增一项。

<a id="Std.IO.Process.EnvRemove(string)"></a>
### Process EnvRemove(string k)

[`src/libraries/z42.io/src/Process.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L76)

从继承的环境里删掉一项。

<a id="Std.IO.Process.ClearEnv()"></a>
### Process ClearEnv()

[`src/libraries/z42.io/src/Process.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L83)

完全不继承调用方环境。

<a id="Std.IO.Process.Stdin(Stdio)"></a>
### Process Stdin(Stdio s)

[`src/libraries/z42.io/src/Process.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L86)

配置 stdin；默认 Null()。

<a id="Std.IO.Process.Stdout(Stdio)"></a>
### Process Stdout(Stdio s)

[`src/libraries/z42.io/src/Process.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L88)

配置 stdout；默认 Pipe()。

<a id="Std.IO.Process.Stderr(Stdio)"></a>
### Process Stderr(Stdio s)

[`src/libraries/z42.io/src/Process.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L90)

配置 stderr；默认 Pipe()。

<a id="Std.IO.Process.StdinBytes(byte[])"></a>
### Process StdinBytes(byte[] b)

[`src/libraries/z42.io/src/Process.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L93)

一次性喂给子进程 stdin 的负载，写完即关；必须配 Stdin(Stdio.Pipe()) 才生效。

<a id="Std.IO.Process.Timeout(long)"></a>
### Process Timeout(long milliseconds)

[`src/libraries/z42.io/src/Process.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L100)

设置超时（毫秒，-1 不限时）；超时抛 ProcessTimeoutException，只对 Run() 生效。

<a id="Std.IO.Process.ShareProcessGroup()"></a>
### Process ShareProcessGroup()

[`src/libraries/z42.io/src/Process.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L106)

让子进程留在调用方的进程组里；仅交互式 tty 透传时需要。

<a id="Std.IO.Process.Run()"></a>
### ProcessResult Run()

[`src/libraries/z42.io/src/Process.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L114)

同步跑到结束，返回 ProcessResult；退出码非 0 不抛（要 fail-fast 调 EnsureSuccess()）。

<a id="Std.IO.Process.Which(string)"></a>
### static string? Which(string name)

[`src/libraries/z42.io/src/Process.z42:137`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L137)

按 $PATH（Windows 上还按 PATHEXT）定位可执行文件，返回解析到的路径，找不到返回 null。

<a id="Std.IO.Process.Spawn()"></a>
### ProcessHandle Spawn()

[`src/libraries/z42.io/src/Process.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Process.z42#L142)

立刻返回 ProcessHandle，子进程在后台跑；启动失败抛 ProcessStartException。

## ProcessHandle

[`src/libraries/z42.io/src/ProcessHandle.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L6)

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

成员：[`ProcessHandle`](#Std.IO.ProcessHandle.ctor(string,long)) · [`Pid`](#Std.IO.ProcessHandle.Pid()) · [`Wait`](#Std.IO.ProcessHandle.Wait()) · [`TryWait`](#Std.IO.ProcessHandle.TryWait()) · [`Kill`](#Std.IO.ProcessHandle.Kill()) · [`KillForce`](#Std.IO.ProcessHandle.KillForce()) · [`WriteStdin`](#Std.IO.ProcessHandle.WriteStdin(byte[])) · [`WriteStdinString`](#Std.IO.ProcessHandle.WriteStdinString(string)) · [`CloseStdin`](#Std.IO.ProcessHandle.CloseStdin()) · [`ReadStdout`](#Std.IO.ProcessHandle.ReadStdout(byte[],int,int)) · [`ReadStderr`](#Std.IO.ProcessHandle.ReadStderr(byte[],int,int)) · [`GetStdinStream`](#Std.IO.ProcessHandle.GetStdinStream()) · [`GetStdoutStream`](#Std.IO.ProcessHandle.GetStdoutStream()) · [`GetStderrStream`](#Std.IO.ProcessHandle.GetStderrStream()) · [`Dispose`](#Std.IO.ProcessHandle.Dispose())

<a id="Std.IO.ProcessHandle.ctor(string,long)"></a>
### ProcessHandle(string program, long slotId)

[`src/libraries/z42.io/src/ProcessHandle.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L16)

由 Process.Spawn() 内部构造。

<a id="Std.IO.ProcessHandle.Pid()"></a>
### long Pid()

[`src/libraries/z42.io/src/ProcessHandle.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L23)

OS 进程号。

<a id="Std.IO.ProcessHandle.Wait()"></a>
### ProcessResult Wait()

[`src/libraries/z42.io/src/ProcessHandle.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L29)

阻塞到子进程结束，返回 ProcessResult；调用之后句柄即失效。

<a id="Std.IO.ProcessHandle.TryWait()"></a>
### ProcessResult? TryWait()

[`src/libraries/z42.io/src/ProcessHandle.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L36)

还在跑返回 null，已结束返回 ProcessResult 并使句柄失效。

<a id="Std.IO.ProcessHandle.Kill()"></a>
### void Kill()

[`src/libraries/z42.io/src/ProcessHandle.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L47)

强制终止子进程（当前与 KillForce 行为相同）。

<a id="Std.IO.ProcessHandle.KillForce()"></a>
### void KillForce()

[`src/libraries/z42.io/src/ProcessHandle.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L53)

强制终止子进程（当前与 Kill 行为相同）。

<a id="Std.IO.ProcessHandle.WriteStdin(byte[])"></a>
### void WriteStdin(byte[] bytes)

[`src/libraries/z42.io/src/ProcessHandle.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L59)

写子进程 stdin；管道写满阻塞，子进程已关 stdin 抛 IOException。

<a id="Std.IO.ProcessHandle.WriteStdinString(string)"></a>
### void WriteStdinString(string s)

[`src/libraries/z42.io/src/ProcessHandle.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L65)

等价 WriteStdin(Utf8.GetBytes(s))。

<a id="Std.IO.ProcessHandle.CloseStdin()"></a>
### void CloseStdin()

[`src/libraries/z42.io/src/ProcessHandle.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L70)

关闭 stdin，子进程读到 EOF；幂等。

<a id="Std.IO.ProcessHandle.ReadStdout(byte[],int,int)"></a>
### int ReadStdout(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/ProcessHandle.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L78)

阻塞读子进程 stdout，返回实际读到的字节数，0 = EOF。

<a id="Std.IO.ProcessHandle.ReadStderr(byte[],int,int)"></a>
### int ReadStderr(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/ProcessHandle.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L84)

阻塞读子进程 stderr，返回实际读到的字节数，0 = EOF。

<a id="Std.IO.ProcessHandle.GetStdinStream()"></a>
### ProcessStdinStream GetStdinStream()

[`src/libraries/z42.io/src/ProcessHandle.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L92)

返回 stdin 的 Stream 包装；重复调用返回同一对象。

<a id="Std.IO.ProcessHandle.GetStdoutStream()"></a>
### ProcessOutputStream GetStdoutStream()

[`src/libraries/z42.io/src/ProcessHandle.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L100)

返回 stdout 的 Stream 包装；重复调用返回同一对象。

<a id="Std.IO.ProcessHandle.GetStderrStream()"></a>
### ProcessOutputStream GetStderrStream()

[`src/libraries/z42.io/src/ProcessHandle.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L108)

返回 stderr 的 Stream 包装；重复调用返回同一对象。

<a id="Std.IO.ProcessHandle.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.io/src/ProcessHandle.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessHandle.z42#L116)

幂等；子进程仍活着时会被 kill + reap（不留僵尸）。

## ProcessOutputStream

[`src/libraries/z42.io/src/ProcessOutputStream.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L5)

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

成员：[`Stdout`](#Std.IO.ProcessOutputStream.Stdout) · [`Stderr`](#Std.IO.ProcessOutputStream.Stderr) · [`ProcessOutputStream`](#Std.IO.ProcessOutputStream.ctor(ProcessHandle,int)) · [`CanRead`](#Std.IO.ProcessOutputStream.CanRead()) · [`CanWrite`](#Std.IO.ProcessOutputStream.CanWrite()) · [`CanSeek`](#Std.IO.ProcessOutputStream.CanSeek()) · [`Read`](#Std.IO.ProcessOutputStream.Read(byte[],int,int)) · [`Flush`](#Std.IO.ProcessOutputStream.Flush()) · [`Close`](#Std.IO.ProcessOutputStream.Close())

<a id="Std.IO.ProcessOutputStream.Stdout"></a>
### static int Stdout

[`src/libraries/z42.io/src/ProcessOutputStream.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L7)

stdout 的文件描述符编号。

<a id="Std.IO.ProcessOutputStream.Stderr"></a>
### static int Stderr

[`src/libraries/z42.io/src/ProcessOutputStream.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L9)

stderr 的文件描述符编号。

<a id="Std.IO.ProcessOutputStream.ctor(ProcessHandle,int)"></a>
### ProcessOutputStream(ProcessHandle handle, int fd)

[`src/libraries/z42.io/src/ProcessOutputStream.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L16)

构造指定 fd 的只读包装；fd 只能是 1 或 2。

<a id="Std.IO.ProcessOutputStream.CanRead()"></a>
### override bool CanRead()

[`src/libraries/z42.io/src/ProcessOutputStream.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L28)

是否可读；关闭前为 true。

<a id="Std.IO.ProcessOutputStream.CanWrite()"></a>
### override bool CanWrite()

[`src/libraries/z42.io/src/ProcessOutputStream.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L30)

是否可写；恒为 false。

<a id="Std.IO.ProcessOutputStream.CanSeek()"></a>
### override bool CanSeek()

[`src/libraries/z42.io/src/ProcessOutputStream.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L32)

是否可定位；恒为 false。

<a id="Std.IO.ProcessOutputStream.Read(byte[],int,int)"></a>
### override int Read(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/ProcessOutputStream.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L37)

阻塞读，直到至少一个字节或管道关闭（EOF 返回 0）。

<a id="Std.IO.ProcessOutputStream.Flush()"></a>
### override void Flush()

[`src/libraries/z42.io/src/ProcessOutputStream.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L54)

只读流无缓冲，空操作。

<a id="Std.IO.ProcessOutputStream.Close()"></a>
### override void Close()

[`src/libraries/z42.io/src/ProcessOutputStream.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessOutputStream.z42#L58)

仅本侧逻辑关闭；底层管道由 ProcessHandle 负责。

## ProcessResult

[`src/libraries/z42.io/src/ProcessResult.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L4)

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

成员：[`Program`](#Std.IO.ProcessResult.Program) · [`ExitCode`](#Std.IO.ProcessResult.ExitCode) · [`Stdout`](#Std.IO.ProcessResult.Stdout) · [`Stderr`](#Std.IO.ProcessResult.Stderr) · [`StdoutBytes`](#Std.IO.ProcessResult.StdoutBytes) · [`StderrBytes`](#Std.IO.ProcessResult.StderrBytes) · [`ProcessResult`](#Std.IO.ProcessResult.ctor(string,int,string,string,byte[],byte[])) · [`EnsureSuccess`](#Std.IO.ProcessResult.EnsureSuccess())

<a id="Std.IO.ProcessResult.Program"></a>
### string Program

[`src/libraries/z42.io/src/ProcessResult.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L6)

被执行的程序。

<a id="Std.IO.ProcessResult.ExitCode"></a>
### int ExitCode

[`src/libraries/z42.io/src/ProcessResult.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L8)

退出码；被信号杀死时为 128 + 信号号。

<a id="Std.IO.ProcessResult.Stdout"></a>
### string Stdout

[`src/libraries/z42.io/src/ProcessResult.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L10)

stdout 的 UTF-8 lossy 解码。

<a id="Std.IO.ProcessResult.Stderr"></a>
### string Stderr

[`src/libraries/z42.io/src/ProcessResult.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L12)

stderr 的 UTF-8 lossy 解码。

<a id="Std.IO.ProcessResult.StdoutBytes"></a>
### byte[] StdoutBytes

[`src/libraries/z42.io/src/ProcessResult.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L14)

stdout 原始字节。

<a id="Std.IO.ProcessResult.StderrBytes"></a>
### byte[] StderrBytes

[`src/libraries/z42.io/src/ProcessResult.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L16)

stderr 原始字节。

<a id="Std.IO.ProcessResult.ctor(string,int,string,string,byte[],byte[])"></a>
### ProcessResult(string program, int exitCode, string stdout, string stderr, byte[] stdoutBytes, byte[] stderrBytes)

[`src/libraries/z42.io/src/ProcessResult.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L19)

用各字段构造。

<a id="Std.IO.ProcessResult.EnsureSuccess()"></a>
### ProcessResult EnsureSuccess()

[`src/libraries/z42.io/src/ProcessResult.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessResult.z42#L43)

ExitCode != 0 时抛 ProcessExitException，否则返回 this（可链式 .Run().EnsureSuccess()）。

## ProcessStdinStream

[`src/libraries/z42.io/src/ProcessStdinStream.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L5)

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

成员：[`ProcessStdinStream`](#Std.IO.ProcessStdinStream.ctor(ProcessHandle)) · [`CanWrite`](#Std.IO.ProcessStdinStream.CanWrite()) · [`CanRead`](#Std.IO.ProcessStdinStream.CanRead()) · [`CanSeek`](#Std.IO.ProcessStdinStream.CanSeek()) · [`Write`](#Std.IO.ProcessStdinStream.Write(byte[],int,int)) · [`Flush`](#Std.IO.ProcessStdinStream.Flush()) · [`Close`](#Std.IO.ProcessStdinStream.Close())

<a id="Std.IO.ProcessStdinStream.ctor(ProcessHandle)"></a>
### ProcessStdinStream(ProcessHandle handle)

[`src/libraries/z42.io/src/ProcessStdinStream.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L10)

由 ProcessHandle.GetStdinStream() 构造。

<a id="Std.IO.ProcessStdinStream.CanWrite()"></a>
### override bool CanWrite()

[`src/libraries/z42.io/src/ProcessStdinStream.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L18)

是否可写；关闭前为 true。

<a id="Std.IO.ProcessStdinStream.CanRead()"></a>
### override bool CanRead()

[`src/libraries/z42.io/src/ProcessStdinStream.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L20)

是否可读；恒为 false。

<a id="Std.IO.ProcessStdinStream.CanSeek()"></a>
### override bool CanSeek()

[`src/libraries/z42.io/src/ProcessStdinStream.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L22)

是否可定位；恒为 false。

<a id="Std.IO.ProcessStdinStream.Write(byte[],int,int)"></a>
### override void Write(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/ProcessStdinStream.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L27)

写子进程 stdin；未配 Stdin(Stdio.Pipe()) 或子进程已关 stdin 时抛相应异常。

<a id="Std.IO.ProcessStdinStream.Flush()"></a>
### override void Flush()

[`src/libraries/z42.io/src/ProcessStdinStream.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L53)

本侧管道写无缓冲，空操作。

<a id="Std.IO.ProcessStdinStream.Close()"></a>
### override void Close()

[`src/libraries/z42.io/src/ProcessStdinStream.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/ProcessStdinStream.z42#L57)

关闭子进程的 stdin（子进程读到 EOF）；不回收进程句柄，仍需 Wait()。

## SeekOrigin

[`src/libraries/z42.io/src/SeekOrigin.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/SeekOrigin.z42#L4)

Seek 的 origin 参数常量（int 常量而非 enum）。

```z42
public static class SeekOrigin {
    static int Begin
    static int Current
    static int End
}
```

成员：[`Begin`](#Std.IO.SeekOrigin.Begin) · [`Current`](#Std.IO.SeekOrigin.Current) · [`End`](#Std.IO.SeekOrigin.End)

<a id="Std.IO.SeekOrigin.Begin"></a>
### static int Begin

[`src/libraries/z42.io/src/SeekOrigin.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/SeekOrigin.z42#L6)

偏移相对流的开头。

<a id="Std.IO.SeekOrigin.Current"></a>
### static int Current

[`src/libraries/z42.io/src/SeekOrigin.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/SeekOrigin.z42#L8)

偏移相对当前位置。

<a id="Std.IO.SeekOrigin.End"></a>
### static int End

[`src/libraries/z42.io/src/SeekOrigin.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/SeekOrigin.z42#L10)

偏移相对流的末尾（负值往回定位）。

## Stdio

[`src/libraries/z42.io/src/Stdio.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L4)

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

成员：[`ModeNull`](#Std.IO.Stdio.ModeNull) · [`ModeInherit`](#Std.IO.Stdio.ModeInherit) · [`ModePipe`](#Std.IO.Stdio.ModePipe) · [`ModeFile`](#Std.IO.Stdio.ModeFile) · [`Stdio`](#Std.IO.Stdio.ctor(int,string)) · [`Inherit`](#Std.IO.Stdio.Inherit()) · [`Pipe`](#Std.IO.Stdio.Pipe()) · [`Null`](#Std.IO.Stdio.Null()) · [`ToFile`](#Std.IO.Stdio.ToFile(string)) · [`GetMode`](#Std.IO.Stdio.GetMode()) · [`GetPath`](#Std.IO.Stdio.GetPath())

<a id="Std.IO.Stdio.ModeNull"></a>
### static int ModeNull

[`src/libraries/z42.io/src/Stdio.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L7)

丢弃（/dev/null）。

<a id="Std.IO.Stdio.ModeInherit"></a>
### static int ModeInherit

[`src/libraries/z42.io/src/Stdio.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L9)

直接继承调用方的 fd。

<a id="Std.IO.Stdio.ModePipe"></a>
### static int ModePipe

[`src/libraries/z42.io/src/Stdio.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L11)

建管道（stdout / stderr 捕获，stdin 可喂数据）。

<a id="Std.IO.Stdio.ModeFile"></a>
### static int ModeFile

[`src/libraries/z42.io/src/Stdio.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L13)

重定向到文件。

<a id="Std.IO.Stdio.ctor(int,string)"></a>
### Stdio(int mode, string path)

[`src/libraries/z42.io/src/Stdio.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L19)

由静态工厂内部构造。

<a id="Std.IO.Stdio.Inherit()"></a>
### static Stdio Inherit()

[`src/libraries/z42.io/src/Stdio.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L26)

直接继承调用方的那个 fd。

<a id="Std.IO.Stdio.Pipe()"></a>
### static Stdio Pipe()

[`src/libraries/z42.io/src/Stdio.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L28)

建管道。

<a id="Std.IO.Stdio.Null()"></a>
### static Stdio Null()

[`src/libraries/z42.io/src/Stdio.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L30)

丢弃（stdin 立刻 EOF，输出丢弃）。

<a id="Std.IO.Stdio.ToFile(string)"></a>
### static Stdio ToFile(string path)

[`src/libraries/z42.io/src/Stdio.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L33)

重定向到文件（只能用于 stdout / stderr）。

<a id="Std.IO.Stdio.GetMode()"></a>
### int GetMode()

[`src/libraries/z42.io/src/Stdio.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L38)

返回模式常量。

<a id="Std.IO.Stdio.GetPath()"></a>
### string GetPath()

[`src/libraries/z42.io/src/Stdio.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stdio.z42#L40)

返回路径；非 File 模式返回 ""。

## Stream

[`src/libraries/z42.io/src/Stream.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L20)

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

成员：[`CanRead`](#Std.IO.Stream.CanRead()) · [`CanWrite`](#Std.IO.Stream.CanWrite()) · [`CanSeek`](#Std.IO.Stream.CanSeek()) · [`Read`](#Std.IO.Stream.Read(byte[],int,int)) · [`Write`](#Std.IO.Stream.Write(byte[],int,int)) · [`ReadByte`](#Std.IO.Stream.ReadByte()) · [`WriteByte`](#Std.IO.Stream.WriteByte(byte)) · [`Flush`](#Std.IO.Stream.Flush()) · [`Close`](#Std.IO.Stream.Close()) · [`Length`](#Std.IO.Stream.Length()) · [`Position`](#Std.IO.Stream.Position()) · [`Seek`](#Std.IO.Stream.Seek(long,int)) · [`ReadAllBytes`](#Std.IO.Stream.ReadAllBytes()) · [`WriteAllBytes`](#Std.IO.Stream.WriteAllBytes(byte[])) · [`ReadExactly`](#Std.IO.Stream.ReadExactly(int)) · [`CopyTo`](#Std.IO.Stream.CopyTo(Stream)) · [`CopyTo`](#Std.IO.Stream.CopyTo(Stream,int))

<a id="Std.IO.Stream.CanRead()"></a>
### virtual bool CanRead()

[`src/libraries/z42.io/src/Stream.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L25)

是否可读；基类返回 false。

<a id="Std.IO.Stream.CanWrite()"></a>
### virtual bool CanWrite()

[`src/libraries/z42.io/src/Stream.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L27)

是否可写；基类返回 false。

<a id="Std.IO.Stream.CanSeek()"></a>
### virtual bool CanSeek()

[`src/libraries/z42.io/src/Stream.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L29)

是否可定位；基类返回 false。

<a id="Std.IO.Stream.Read(byte[],int,int)"></a>
### virtual int Read(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/Stream.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L34)

填充 buffer[offset..offset+count]，返回实际写入的字节数（0..count），0 = EOF。

<a id="Std.IO.Stream.Write(byte[],int,int)"></a>
### virtual void Write(byte[] buffer, int offset, int count)

[`src/libraries/z42.io/src/Stream.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L39)

写 buffer[offset..offset+count]。

<a id="Std.IO.Stream.ReadByte()"></a>
### virtual int ReadByte()

[`src/libraries/z42.io/src/Stream.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L44)

读一个字节，返回 0..255；EOF 返回 -1。

<a id="Std.IO.Stream.WriteByte(byte)"></a>
### virtual void WriteByte(byte b)

[`src/libraries/z42.io/src/Stream.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L52)

写一个字节。

<a id="Std.IO.Stream.Flush()"></a>
### virtual void Flush()

[`src/libraries/z42.io/src/Stream.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L61)

刷新缓冲；基类无缓冲状态，默认空操作。

<a id="Std.IO.Stream.Close()"></a>
### virtual void Close()

[`src/libraries/z42.io/src/Stream.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L65)

释放资源；基类无资源持有，默认空操作。

<a id="Std.IO.Stream.Length()"></a>
### virtual long Length()

[`src/libraries/z42.io/src/Stream.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L71)

流的字节长度；基类抛 NotSupportedException。

<a id="Std.IO.Stream.Position()"></a>
### virtual long Position()

[`src/libraries/z42.io/src/Stream.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L76)

当前读写位置；基类抛 NotSupportedException。

<a id="Std.IO.Stream.Seek(long,int)"></a>
### virtual long Seek(long offset, int origin)

[`src/libraries/z42.io/src/Stream.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L81)

把游标移到相对 origin（SeekOrigin.Begin / Current / End）偏移 offset 处，返回新的绝对位置。

<a id="Std.IO.Stream.ReadAllBytes()"></a>
### byte[] ReadAllBytes()

[`src/libraries/z42.io/src/Stream.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L88)

读到 EOF 为止，返回精确长度的新 byte[]（内部按 4 KB 分块累积）。

<a id="Std.IO.Stream.WriteAllBytes(byte[])"></a>
### void WriteAllBytes(byte[] data)

[`src/libraries/z42.io/src/Stream.z42:126`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L126)

等价 Write(data, 0, data.Length)。

<a id="Std.IO.Stream.ReadExactly(int)"></a>
### byte[] ReadExactly(int count)

[`src/libraries/z42.io/src/Stream.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L134)

必须凑满 count 字节并作为新 byte[] 返回，否则抛 Std.IO.EndOfStreamException。

<a id="Std.IO.Stream.CopyTo(Stream)"></a>
### void CopyTo(Stream dest)

[`src/libraries/z42.io/src/Stream.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L155)

把剩余内容全部灌入 dest（默认 4 KB 中转缓冲），两端都不关闭。

<a id="Std.IO.Stream.CopyTo(Stream,int)"></a>
### void CopyTo(Stream dest, int bufferSize)

[`src/libraries/z42.io/src/Stream.z42:160`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/Stream.z42#L160)

用指定 bufferSize 的中转缓冲拷贝；bufferSize <= 0 抛 ArgumentException。

## StreamReader

[`src/libraries/z42.io/src/StreamReader.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L6)

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

成员：[`StreamReader`](#Std.IO.StreamReader.ctor(Stream)) · [`StreamReader`](#Std.IO.StreamReader.ctor(Stream,Encoding)) · [`Peek`](#Std.IO.StreamReader.Peek()) · [`Read`](#Std.IO.StreamReader.Read()) · [`Read`](#Std.IO.StreamReader.Read(char[],int,int)) · [`ReadLine`](#Std.IO.StreamReader.ReadLine()) · [`ReadToEnd`](#Std.IO.StreamReader.ReadToEnd()) · [`Close`](#Std.IO.StreamReader.Close())

<a id="Std.IO.StreamReader.ctor(Stream)"></a>
### StreamReader(Stream source)

[`src/libraries/z42.io/src/StreamReader.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L14)

默认 UTF-8 编码。

<a id="Std.IO.StreamReader.ctor(Stream,Encoding)"></a>
### StreamReader(Stream source, Encoding encoding)

[`src/libraries/z42.io/src/StreamReader.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L22)

指定编码。

<a id="Std.IO.StreamReader.Peek()"></a>
### int Peek()

[`src/libraries/z42.io/src/StreamReader.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L32)

窥视下一个字符码；EOF 返回 -1。

<a id="Std.IO.StreamReader.Read()"></a>
### int Read()

[`src/libraries/z42.io/src/StreamReader.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L39)

消费一个字符码；EOF 返回 -1。

<a id="Std.IO.StreamReader.Read(char[],int,int)"></a>
### int Read(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/StreamReader.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L48)

填充 buffer[offset..offset+count]，返回实际填入数，0 = EOF。

<a id="Std.IO.StreamReader.ReadLine()"></a>
### string? ReadLine()

[`src/libraries/z42.io/src/StreamReader.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L57)

读一行（不含终止符），EOF 返回 null。

<a id="Std.IO.StreamReader.ReadToEnd()"></a>
### string ReadToEnd()

[`src/libraries/z42.io/src/StreamReader.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L64)

返回剩余全部内容；EOF 返回 ""。

<a id="Std.IO.StreamReader.Close()"></a>
### void Close()

[`src/libraries/z42.io/src/StreamReader.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamReader.z42#L73)

标记关闭并释放缓存字符串；不关闭被包装的源流。

## StreamWriter

[`src/libraries/z42.io/src/StreamWriter.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L6)

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

成员：[`StreamWriter`](#Std.IO.StreamWriter.ctor(Stream)) · [`StreamWriter`](#Std.IO.StreamWriter.ctor(Stream,Encoding)) · [`Write`](#Std.IO.StreamWriter.Write(string)) · [`Write`](#Std.IO.StreamWriter.Write(char[],int,int)) · [`WriteLine`](#Std.IO.StreamWriter.WriteLine()) · [`WriteLine`](#Std.IO.StreamWriter.WriteLine(string)) · [`Flush`](#Std.IO.StreamWriter.Flush()) · [`Close`](#Std.IO.StreamWriter.Close())

<a id="Std.IO.StreamWriter.ctor(Stream)"></a>
### StreamWriter(Stream dest)

[`src/libraries/z42.io/src/StreamWriter.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L12)

默认 UTF-8 编码。

<a id="Std.IO.StreamWriter.ctor(Stream,Encoding)"></a>
### StreamWriter(Stream dest, Encoding encoding)

[`src/libraries/z42.io/src/StreamWriter.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L19)

指定编码。

<a id="Std.IO.StreamWriter.Write(string)"></a>
### void Write(string s)

[`src/libraries/z42.io/src/StreamWriter.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L28)

编码并写入字符串。

<a id="Std.IO.StreamWriter.Write(char[],int,int)"></a>
### void Write(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/StreamWriter.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L36)

编码并写入字符区间 buffer[offset..offset+count]。

<a id="Std.IO.StreamWriter.WriteLine()"></a>
### void WriteLine()

[`src/libraries/z42.io/src/StreamWriter.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L60)

写 "\n"。

<a id="Std.IO.StreamWriter.WriteLine(string)"></a>
### void WriteLine(string s)

[`src/libraries/z42.io/src/StreamWriter.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L65)

写 s + "\n"。

<a id="Std.IO.StreamWriter.Flush()"></a>
### void Flush()

[`src/libraries/z42.io/src/StreamWriter.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L75)

转发给 dest.Flush()。

<a id="Std.IO.StreamWriter.Close()"></a>
### void Close()

[`src/libraries/z42.io/src/StreamWriter.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StreamWriter.z42#L81)

先 Flush 再标记关闭；不关闭被包装的目标流。

## StringReader

[`src/libraries/z42.io/src/StringReader.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L5)

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

成员：[`StringReader`](#Std.IO.StringReader.ctor(string)) · [`Peek`](#Std.IO.StringReader.Peek()) · [`Read`](#Std.IO.StringReader.Read()) · [`Read`](#Std.IO.StringReader.Read(char[],int,int)) · [`ReadLine`](#Std.IO.StringReader.ReadLine()) · [`ReadToEnd`](#Std.IO.StringReader.ReadToEnd()) · [`Close`](#Std.IO.StringReader.Close())

<a id="Std.IO.StringReader.ctor(string)"></a>
### StringReader(string source)

[`src/libraries/z42.io/src/StringReader.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L14)

在 source 上构造游标式读取器。

<a id="Std.IO.StringReader.Peek()"></a>
### int Peek()

[`src/libraries/z42.io/src/StringReader.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L24)

窥视下一个字符码，不前进游标；EOF 返回 -1。

<a id="Std.IO.StringReader.Read()"></a>
### int Read()

[`src/libraries/z42.io/src/StringReader.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L31)

消费下一个字符码；EOF 返回 -1（游标不变）。

<a id="Std.IO.StringReader.Read(char[],int,int)"></a>
### int Read(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/StringReader.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L42)

填充 buffer[offset..offset+count]，返回实际填入数，0 = EOF。

<a id="Std.IO.StringReader.ReadLine()"></a>
### string? ReadLine()

[`src/libraries/z42.io/src/StringReader.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L64)

读一行（不含终止符）；\n / \r\n / 单独 \r 都终止一行，游标已在 EOF 时返回 null。

<a id="Std.IO.StringReader.ReadToEnd()"></a>
### string ReadToEnd()

[`src/libraries/z42.io/src/StringReader.z42:98`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L98)

返回从当前游标到末尾的剩余内容；EOF 返回 ""。

<a id="Std.IO.StringReader.Close()"></a>
### void Close()

[`src/libraries/z42.io/src/StringReader.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringReader.z42#L109)

标记关闭（幂等）；关闭后任何读操作抛 InvalidOperationException。

## StringWriter

[`src/libraries/z42.io/src/StringWriter.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L6)

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

成员：[`StringWriter`](#Std.IO.StringWriter.ctor()) · [`StringWriter`](#Std.IO.StringWriter.ctor(int)) · [`Write`](#Std.IO.StringWriter.Write(string)) · [`Write`](#Std.IO.StringWriter.Write(char[],int,int)) · [`WriteLine`](#Std.IO.StringWriter.WriteLine()) · [`WriteLine`](#Std.IO.StringWriter.WriteLine(string)) · [`Clear`](#Std.IO.StringWriter.Clear()) · [`Close`](#Std.IO.StringWriter.Close())

<a id="Std.IO.StringWriter.ctor()"></a>
### StringWriter()

[`src/libraries/z42.io/src/StringWriter.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L11)

构造空写入器。

<a id="Std.IO.StringWriter.ctor(int)"></a>
### StringWriter(int initialCapacity)

[`src/libraries/z42.io/src/StringWriter.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L17)

initialCapacity 仅为 API 形状兼容的提示，当前被忽略。

<a id="Std.IO.StringWriter.Write(string)"></a>
### void Write(string s)

[`src/libraries/z42.io/src/StringWriter.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L25)

追加字符串。

<a id="Std.IO.StringWriter.Write(char[],int,int)"></a>
### void Write(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/StringWriter.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L34)

追加字符区间 buffer[offset..offset+count]。

<a id="Std.IO.StringWriter.WriteLine()"></a>
### void WriteLine()

[`src/libraries/z42.io/src/StringWriter.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L57)

写 "\n"。

<a id="Std.IO.StringWriter.WriteLine(string)"></a>
### void WriteLine(string s)

[`src/libraries/z42.io/src/StringWriter.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L63)

写 s + "\n"。

<a id="Std.IO.StringWriter.Clear()"></a>
### void Clear()

[`src/libraries/z42.io/src/StringWriter.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L78)

清空，后续写入从头开始。

<a id="Std.IO.StringWriter.Close()"></a>
### void Close()

[`src/libraries/z42.io/src/StringWriter.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/StringWriter.z42#L86)

标记关闭（幂等）；关闭后 Write / WriteLine / Clear 抛 InvalidOperationException，ToString() 仍可用。

## TextReader

[`src/libraries/z42.io/src/TextReader.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L5)

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

成员：[`Peek`](#Std.IO.TextReader.Peek()) · [`Read`](#Std.IO.TextReader.Read()) · [`Close`](#Std.IO.TextReader.Close()) · [`Dispose`](#Std.IO.TextReader.Dispose()) · [`ReadBufferBase`](#Std.IO.TextReader.ReadBufferBase(char[],int,int)) · [`ReadLineBase`](#Std.IO.TextReader.ReadLineBase()) · [`ReadToEndBase`](#Std.IO.TextReader.ReadToEndBase())

<a id="Std.IO.TextReader.Peek()"></a>
### virtual int Peek()

[`src/libraries/z42.io/src/TextReader.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L9)

窥视下一个字符码；基类抛 NotSupportedException。

<a id="Std.IO.TextReader.Read()"></a>
### virtual int Read()

[`src/libraries/z42.io/src/TextReader.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L14)

读一个字符码；基类抛 NotSupportedException。

<a id="Std.IO.TextReader.Close()"></a>
### virtual void Close()

[`src/libraries/z42.io/src/TextReader.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L19)

基类无资源持有，默认空操作。

<a id="Std.IO.TextReader.Dispose()"></a>
### virtual void Dispose()

[`src/libraries/z42.io/src/TextReader.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L23)

等价 Close()。

<a id="Std.IO.TextReader.ReadBufferBase(char[],int,int)"></a>
### int ReadBufferBase(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/TextReader.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L32)

默认批量读：循环 Read()，返回实际填入数（0 = EOF）。

<a id="Std.IO.TextReader.ReadLineBase()"></a>
### string? ReadLineBase()

[`src/libraries/z42.io/src/TextReader.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L48)

默认行读取：\n、\r\n 与单独 \r 都终止一行，空行返回 ""，EOF 返回 null。

<a id="Std.IO.TextReader.ReadToEndBase()"></a>
### string ReadToEndBase()

[`src/libraries/z42.io/src/TextReader.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextReader.z42#L82)

默认读剩余全部：循环 Read() 直到 EOF。

## TextWriter

[`src/libraries/z42.io/src/TextWriter.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L5)

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

成员：[`Write`](#Std.IO.TextWriter.Write(string)) · [`Flush`](#Std.IO.TextWriter.Flush()) · [`Close`](#Std.IO.TextWriter.Close()) · [`Dispose`](#Std.IO.TextWriter.Dispose()) · [`WriteCharsBase`](#Std.IO.TextWriter.WriteCharsBase(char[],int,int)) · [`WriteCharBase`](#Std.IO.TextWriter.WriteCharBase(char)) · [`WriteLineBase`](#Std.IO.TextWriter.WriteLineBase()) · [`WriteLineBase`](#Std.IO.TextWriter.WriteLineBase(string))

<a id="Std.IO.TextWriter.Write(string)"></a>
### virtual void Write(string s)

[`src/libraries/z42.io/src/TextWriter.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L9)

写字符串；基类抛 NotSupportedException。

<a id="Std.IO.TextWriter.Flush()"></a>
### virtual void Flush()

[`src/libraries/z42.io/src/TextWriter.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L14)

基类无缓冲状态，默认空操作。

<a id="Std.IO.TextWriter.Close()"></a>
### virtual void Close()

[`src/libraries/z42.io/src/TextWriter.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L18)

基类无资源持有，默认空操作。

<a id="Std.IO.TextWriter.Dispose()"></a>
### virtual void Dispose()

[`src/libraries/z42.io/src/TextWriter.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L22)

等价 Close()。

<a id="Std.IO.TextWriter.WriteCharsBase(char[],int,int)"></a>
### void WriteCharsBase(char[] buffer, int offset, int count)

[`src/libraries/z42.io/src/TextWriter.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L29)

默认批量写字符：写 buffer[offset..offset+count]。

<a id="Std.IO.TextWriter.WriteCharBase(char)"></a>
### void WriteCharBase(char c)

[`src/libraries/z42.io/src/TextWriter.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L46)

默认写单个字符。

<a id="Std.IO.TextWriter.WriteLineBase()"></a>
### void WriteLineBase()

[`src/libraries/z42.io/src/TextWriter.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L53)

写 "\n"（不分平台）。

<a id="Std.IO.TextWriter.WriteLineBase(string)"></a>
### void WriteLineBase(string s)

[`src/libraries/z42.io/src/TextWriter.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.io/src/TextWriter.z42#L58)

写 s + "\n"（不分平台）。

