# z42.io —— 二进制读写器

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.io/`；命名空间 `Std.IO.Binary`（异常类型 `BinaryException` 在 `Std`）

`BinaryReader` / `BinaryWriter` 在 [`Std.IO.Stream`](io-stream.md) 之上提供定长整数、
IEEE-754 浮点、varint 与 UTF-8 字符串的**显式字节序**读写。用于自定义二进制协议、
二进制文件格式、跨进程 marshal 这类需要精确控制线上字节的场景。

字节序**没有默认值**：每个多字节方法都带 `LE` 或 `BE` 后缀，强制调用方在写代码时就
表明立场。`LE` = 低字节在前（x86/ARM 原生、PNG / ZIP / protobuf / msgpack）；
`BE` = 高字节在前，即 network byte order（TCP/IP 头、DNS、Java class 文件）。

本页只写数值编解码；底层 `Stream` 的能力谓词、EOF 约定、`Close()` 归属见
[流与字符读写器](io-stream.md)。

## BinaryReader

```z42
public class BinaryReader {
    public BinaryReader(byte[] data);                  // 包一层只读 MemoryStream
    public static BinaryReader OverStream(Stream src); // 在已有 Stream 上读

    // 游标（要求底层 Stream 可定位）
    public int  GetPosition();
    public int  GetLength();
    public bool EndOfStream();
    public void Seek(int pos);
    public void Skip(int count);

    // 字节
    public int    ReadByte();              // 0..255
    public byte[] ReadBytes(int count);

    // 整数
    public int  ReadInt16LE();  public int  ReadInt16BE();   // 符号扩展到 int
    public int  ReadInt32LE();  public int  ReadInt32BE();
    public long ReadInt64LE();  public long ReadInt64BE();

    // 浮点（返回 double）
    public double ReadSingleLE();  public double ReadSingleBE();   // 线上 4 字节 f32
    public double ReadDoubleLE();  public double ReadDoubleBE();   // 线上 8 字节 f64

    // 变长整数
    public long ReadVarInt64();
    public long ReadVarSInt64();

    // 字符串
    public string ReadString(int byteCount);   // 按 UTF-8 解码 byteCount 个字节
}
```

| 成员 | 说明 |
|---|---|
| `BinaryReader(byte[] data)` | 最常用形态。内部 `new MemoryStream(data)`，**不复制** `data` |
| `OverStream(Stream src)` | 静态工厂而非构造器（z42 的重载决议区分不了 `(byte[])` 与 `(Stream)`）。`src.CanRead()` 为 false 时抛 `BinaryException` |
| `GetPosition()` / `GetLength()` | 返回 `int`（不是 `long`）；底层流不可定位时抛 `BinaryException` |
| `EndOfStream()` | `Position >= Length`；同样要求可定位 |
| `Seek(pos)` | 绝对定位。`pos` 落在 `[0, Length]` 之外抛 `BinaryException` |
| `Skip(count)` | 等价 `Seek(GetPosition() + count)`，`count` 可为负 |
| `ReadByte()` | EOF 时抛 `BinaryException` |
| `ReadBytes(count)` | `count < 0` 抛 `BinaryException`，`count == 0` 返回空数组；字节不够抛 `Std.EndOfStreamException` |
| `ReadInt16*` | 从第 16 位符号扩展，所以 `0xFFFE` 读回 `-2` 而不是 65534 |
| `ReadSingle*` | 线上是 4 字节 f32，返回时加宽成 `double` |
| `ReadVarInt64()` | 7 位一组的 protobuf 式 varint；超过 10 字节仍未结束抛 `BinaryException` |
| `ReadVarSInt64()` | ZigZag 解码版（protobuf 的 `sint64`） |

在**不可定位**的流（如 `Gzip.WrapRead(...)` 的解码流）上，`GetPosition` / `GetLength` /
`EndOfStream` / `Seek` / `Skip` 全部抛 `BinaryException`，但所有 `Read*` 方法照常工作。

## BinaryWriter

```z42
public class BinaryWriter {
    public BinaryWriter();                              // 内部可增长 MemoryStream
    public static BinaryWriter OverStream(Stream dest); // 写进已有 Stream

    public int    GetLength();
    public byte[] ToArray();      // 仅默认构造时可用
    public void   Clear();        // 仅默认构造时可用

    public void WriteByte(int b);          // 只写低 8 位
    public void WriteBytes(byte[] data);

    public void WriteInt16LE(int v);   public void WriteInt16BE(int v);
    public void WriteInt32LE(int v);   public void WriteInt32BE(int v);
    public void WriteInt64LE(long v);  public void WriteInt64BE(long v);

    public void WriteSingleLE(double v);  public void WriteSingleBE(double v);
    public void WriteDoubleLE(double v);  public void WriteDoubleBE(double v);

    public int WriteVarInt64(long v);     // 返回写入字节数
    public int WriteVarSInt64(long v);    // 返回写入字节数

    public int WriteString(string s);     // UTF-8 编码后写入，返回字节数
}
```

| 成员 | 说明 |
|---|---|
| `BinaryWriter()` | 内部持有一个可增长 `MemoryStream`，`ToArray()` / `Clear()` 可用 |
| `OverStream(dest)` | `dest.CanWrite()` 为 false 时抛 `BinaryException`。**不拥有** `dest`：既不关闭它，`ToArray()` / `Clear()` 也一律抛 `BinaryException` |
| `GetLength()` | 底层流不可定位时抛 `BinaryException` |
| `ToArray()` | 当前内容的独立副本 |
| `Clear()` | 复位到空：`GetLength()` 归 0，后续写入从头开始 |
| `WriteByte(int b)` | 形参是 `int`，只有低 8 位落盘 |
| `WriteInt16*` / `WriteInt32*` | 形参都是 `int`，高位按宽度截断（「宽入窄线」） |
| `WriteSingle*` | 形参是 `double`，写成 4 字节 f32——超出 f32 精度/范围的值在此处舍入 |
| `WriteVarInt64(v)` | 正数按 ⌈bits/7⌉ 字节；**负数固定 10 字节**（符号位在最高段） |
| `WriteVarSInt64(v)` | 先 ZigZag 再 varint，小负数保持紧凑（`-1` / `-64` 都是 1 字节） |

`BinaryWriter` 没有 `Close()` / `Flush()`：它不拥有目标流的生命周期，需要收尾时直接对
那个 `Stream` 调用。

## BinaryException

```z42
namespace Std;

public class BinaryException : Exception {
    public BinaryException(string message);
}
```

由 `BinaryReader` / `BinaryWriter` 自己发现的错误（不可定位、越界定位、
`OverStream` 能力不符、`ToArray` / `Clear` 无所有权、varint 过长、`ReadByte` 撞上 EOF）
使用这个类型。

**但是多字节读取撞上 EOF 时抛的是 `Std.EndOfStreamException` 而不是
`BinaryException`**——`ReadBytes` / `ReadInt16*` / `ReadInt32*` / `ReadInt64*` /
`ReadSingle*` / `ReadDouble*` / `ReadString` 都走 `Stream.ReadExactly`，异常直接透出来。
想一网打尽就 `catch (Exception)`，或者两种都单独 catch。

## 用法

```z42
using Std;
using Std.IO;
using Std.IO.Binary;

void Main() {
    // 写一个小协议帧：magic(BE) + 长度(varint) + 载荷
    BinaryWriter w = new BinaryWriter();
    w.WriteInt32BE(0x5A343200);
    byte[] payload = new byte[] { (byte)1, (byte)2, (byte)3 };
    w.WriteVarInt64((long)payload.Length);
    w.WriteBytes(payload);
    byte[] frame = w.ToArray();

    // 读回来
    BinaryReader r = new BinaryReader(frame);
    int  magic = r.ReadInt32BE();
    long len   = r.ReadVarInt64();
    byte[] got = r.ReadBytes((int)len);
    Console.WriteLine($"magic={magic} len={len} rest={r.EndOfStream()}");

    // 写进任意 Stream（这里是内存流；FileStream / 压缩流同样可用）
    MemoryStream dest = new MemoryStream();
    BinaryWriter sw = BinaryWriter.OverStream(dest);
    sw.WriteInt64LE(-1L);
    sw.WriteString("héllo");          // 返回 6（UTF-8 字节数，不是字符数）
    Console.WriteLine($"dest={dest.Length()}");

    // 从任意 Stream 读
    BinaryReader sr = BinaryReader.OverStream(new MemoryStream(dest.ToArray()));
    Console.WriteLine($"{sr.ReadInt64LE()} {sr.ReadString(6)}");
}
```

## 不支持

- **没有 `BinaryReader(Stream)` / `BinaryWriter(Stream)` 构造器**：用静态工厂
  `OverStream(...)`。
- **没有 `BinaryWriter(int initialCapacity)`**：需要预留容量就自己建 `MemoryStream`
  再 `OverStream`。
- **没有 `byte` 形参 / 返回值**：字节一律以 `int` 进出，调用方自己 `(byte)x` 转回。
- **没有 `short` / `float` 返回类型**：Int16 以符号扩展后的 `int` 返回，
  Single 以加宽后的 `double` 返回。
- **`ReadString` 不带长度前缀**：帧格式由调用方决定，`ReadString(byteCount)` 只负责按
  给定字节数解码。
- **只有 UTF-8**：没有编码参数，也不接受 `Encoding`。
- **没有无符号读写**：不存在 `ReadUInt32` 之类；需要无符号语义时自己按 `long` 掩码处理。
- **没有 `Close()` / `Dispose()`**：生命周期归底层 `Stream`。
