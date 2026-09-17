# z42.encoding —— 字节与文本的编解码

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.encoding/`；命名空间 `Std.Encoding`

`byte[]` ↔ `string` 的两类转换：**文本编码**（UTF-8 / UTF-16 / UTF-32，把字符串变成字节）
与 **二进制转文本编码**（Hex / Base64 / Base32 的四种变体，把任意字节变成可打印字符）。
全部是静态类，不需要实例化。

所有格式错误统一抛 `Std.FormatException`；只有 `Encoding.GetString` 的越界切片抛
`Std.ArgumentException`。解码一律**严格 fail-fast**，不会静默替换成 U+FFFD。

## 二进制转文本

### Hex

```z42
public static class Hex {
    public static string Encode(byte[] bytes)        // 小写
    public static string EncodeUpper(byte[] bytes)   // 大写
    public static byte[] Decode(string hex)
}
```

`Decode` 接受混合大小写。奇数长度抛 `FormatException("odd-length hex string")`，
非 `[0-9a-fA-F]` 字符抛 `invalid hex character '<c>'`。空串 ↔ 空数组。

### Base64（RFC 4648 §4）

```z42
public static class Base64 {
    public static string Encode(byte[] bytes)
    public static byte[] Decode(string s)
}
```

标准字母表（`+` / `/`），输出**带 `=` padding**。`Decode` 严格：长度不是 4 的倍数抛
`invalid base64 length (must be multiple of 4)`，padding 超过 2 个抛
`too much padding in base64 input`，非字母表字符抛 `invalid base64 character '<c>'`。

### Base64Url（RFC 4648 §5）

```z42
public static class Base64Url {
    public static string Encode(byte[] bytes)
    public static byte[] Decode(string s)
}
```

URL-safe 字母表（`-` / `_`），**编码输出不带 padding**（JWT / RFC 7515 Appendix C 形态）。
`Decode` 宽松接受带或不带 padding 的输入，但**遇到标准变体的 `+` / `/` 立即报错**并提示改用
`Base64`；长度 mod 4 == 1 也直接报错。

### Base32（RFC 4648 §6）

```z42
public static class Base32 {
    public static string Encode(byte[] bytes)
    public static byte[] Decode(string s)
}
```

字母表 `A-Z2-7`。每 5 字节 → 8 字符，尾部用 `=` 补齐到 8 的倍数。`Decode` **只接受大写**，
长度必须是 8 的倍数，padding 个数必须属于 `{0, 1, 3, 4, 6}`。典型用途：TOTP / HOTP 密钥。

### Base32Hex（RFC 4648 §7）

```z42
public static class Base32Hex {
    public static string Encode(byte[] bytes)
    public static byte[] Decode(string s)
}
```

字母表 `0-9A-V`（A=10 … V=31），打包与 padding 规则同 `Base32`。字母表本身按序，因此
**编码串的字典序与原字节的自然序一致**——排序后仍能还原顺序的场景（如 DNS NSEC3）用它。
`Decode` 同样只接受大写。

### Base32Crockford

```z42
public static class Base32Crockford {
    public static string Encode(byte[] bytes)
    public static byte[] Decode(string s)
}
```

字母表 `0123456789ABCDEFGHJKMNPQRSTVWXYZ`（剔除 `I` `L` `O` `U` 以降低人工抄写歧义），
与 RFC 4648 §6 **完全不是同一张表**，两者不能互解。

- `Encode` **无 padding**，输出长度 = `ceil(输入位数 / 5)`。
- `Decode` 大小写不敏感；`I` / `L` / `i` / `l` 归一成 `1`，`O` / `o` 归一成 `0`；
  `-` 分隔符被直接丢弃。
- 有效字符数（去掉 `-` 后）对 8 取模不能是 `1` / `3` / `6`——这三个余数不可能由任何字节数
  产生，会抛 `FormatException`。
- `U` 不在字母表内，出现即报错。

典型用途：ULID、面向人类的短 ID。

## 文本编码

### Utf8

```z42
public static class Utf8 {
    public static byte[] GetBytes(string s)
    public static string GetString(byte[] bytes)
}
```

`GetString` 严格校验并拒绝：截断序列、非法 continuation 字节、overlong 编码（2/3/4 字节
各自判断）、surrogate 码点（U+D800–U+DFFF）、超出 U+10FFFF、非法首字节。

### Utf16（RFC 2781）/ Utf32

```z42
public static class Utf16 {
    public static byte[] GetBytesLE(string s)
    public static byte[] GetBytesBE(string s)
    public static string GetStringLE(byte[] bytes)
    public static string GetStringBE(byte[] bytes)
}

public static class Utf32 {
    public static byte[] GetBytesLE(string s)
    public static byte[] GetBytesBE(string s)
    public static string GetStringLE(byte[] bytes)
    public static string GetStringBE(byte[] bytes)
}
```

字节序必须在方法名上选定，**没有不带 LE / BE 的重载**，也不读写 BOM。

- UTF-16：BMP 码点 2 字节，增补平面 4 字节（代理对）。解码校验代理对配对，拒绝孤立高/低代理
  与截断输入；字节长度不是 2 的倍数即报错。
- UTF-32：定长 4 字节一个码点。编解码两侧都拒绝孤立 surrogate 与 > U+10FFFF；字节长度不是
  4 的倍数即报错。

### Encoding（实例门面）

```z42
public class Encoding {
    public Encoding(int kind)
    public static Encoding GetUtf8()

    public byte[] GetBytes(string s)
    public string GetString(byte[] bytes)
    public string GetString(byte[] bytes, int offset, int count)
}

public static class EncodingSingletons {
    public static Encoding Utf8()
}
```

给需要「接受任意编码」的调用方（例如 `Std.IO` 的 `StreamReader` / `StreamWriter`）用的实例
门面。`GetUtf8()` 在一次 VM 生命周期内返回同一个实例；`EncodingSingletons.Utf8()` 返回的是
同一个对象。

| 成员 | 说明 |
|---|---|
| `GetBytes(s)` | 等价 `Utf8.GetBytes(s)` |
| `GetString(bytes)` | 等价 `Utf8.GetString(bytes)`，同样严格校验 |
| `GetString(bytes, offset, count)` | 解码切片 `bytes[offset .. offset+count)`。`count == 0` 返回 `""`；`offset < 0`、`count < 0` 或 `offset + count > bytes.Length` 抛 `ArgumentException` |

> **当前只有 UTF-8 一种编码。** 公开构造器 `Encoding(int kind)` 的 `kind` 参数目前不影响任何
> 行为——无论传什么值，编解码都走 UTF-8。请用 `Encoding.GetUtf8()`，不要自己 `new`。

## 用法

```z42
using Std;
using Std.IO;
using Std.Encoding;

void Main() {
    byte[] data = Utf8.GetBytes("foobar");

    Console.WriteLine(Hex.Encode(data));               // 666f6f626172
    Console.WriteLine(Base64.Encode(data));            // Zm9vYmFy
    Console.WriteLine(Base32.Encode(data));            // MZXW6YTBOI======
    Console.WriteLine(Base32Hex.Encode(data));         // CPNMUOJ1E8======
    Console.WriteLine(Base32Crockford.Encode(data));   // CSQPYRK1E8（无 padding）

    // Crockford 解码：大小写不敏感 + `-` 丢弃
    Console.WriteLine(Utf8.GetString(Base32Crockford.Decode("csqpy-rk1e8")));  // foobar

    // UTF-16 / UTF-32 必须选字节序
    string s = "a中😀";
    Console.WriteLine(Utf16.GetBytesLE(s).Length);      // 8
    Console.WriteLine(Utf32.GetBytesLE(s).Length);      // 12
    Console.WriteLine(Utf16.GetStringBE(Utf16.GetBytesBE(s)));  // a中😀

    // 实例门面 + 切片解码
    Encoding e = Encoding.GetUtf8();
    Console.WriteLine(e.GetString(Utf8.GetBytes("hello"), 1, 3));   // ell

    try {
        Base64Url.Decode("a+b=");
    } catch (FormatException ex) {
        // invalid base64url character '+' — use Std.Encoding.Base64 for standard variant
        Console.WriteLine(ex.Message);
    }
}
```

## 不支持

- **只有一次性全量转换**：没有 streaming 的 `Encoder` / `Decoder` 状态机，输入必须整个
  `byte[]` / `string` 在手。
- **没有宽松解码**：所有解码器遇到非法输入都抛异常，没有「替换成 U+FFFD 继续」的 overload。
- **UTF-8 之外的单字节编码没有**：Latin-1 / ASCII / GBK 等都不提供；`Encoding` 类目前只包
  UTF-8 一种实现。
- **不处理 BOM**：`Utf16` / `Utf32` 既不写入也不识别 BOM，字节序只由方法名决定。
- **Base32 / Base32Hex 只接受大写**：小写输入报错，需要容错请先自行 `ToUpper`。
- **没有 Base85 / ASCII85**。
- **Crockford 的 check-digit 扩展（`*~$=U`）不支持**。
- **Base64 不支持换行折行**：既不输出 MIME 76 列折行，解码也不跳过 `\r` / `\n`。
- **Hex 无分隔符形态**：不产生也不接受 `de:ad:be:ef` 这类带分隔符的 hex。
