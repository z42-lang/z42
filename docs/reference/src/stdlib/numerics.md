<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.numerics/ -->

# z42.numerics —— 任意精度整数 / 十进制定点 / 复数。

> 包路径 `src/libraries/z42.numerics/`；命名空间 `Std.Numerics`

三个互不依赖的数值类型，都是引用类型，按不可变使用——每个运算返回新对象，
从不改动接收者：

- `BigInt`：任意精度整数。溢出 `long` 的算术、模幂 / 模逆 / GCD / 素性检测。
- `Decimal`：任意精度十进制定点（`BigInt` 尾数 + 十进制 scale）。金额等不能用二进制浮点的场合。
- `Complex`：`double` 实部 + 虚部的复数。

没有运算符重载——一律走方法调用（`a.Add(b)` 而不是 `a + b`）。也没有
`Vector<T>` / SIMD 类型。`BigInt` 的素性检测依赖 `z42.random`，包清单里会一并拉入。

## 不支持

- **运算符重载**：`a + b` / `a * b` / `a == b` 对这三个类型都不成立，一律用方法
- **`Vector<T>` / SIMD**：本包没有向量类型
- **`BigInt` 的算术右移**：`ShiftRight` 是幅值移位，没有向 -∞ 取整的变体
- **`Decimal` 的舍入模式**：`DivideBy` 只有向零截断，没有 banker's rounding 之类的选项；也没有不指定结果位数的除法
- **`Decimal` 的科学计数法解析**：`Parse("1e5")` 不接受
- **`Complex` 的其余超越函数**：`Tan` / 反三角 / 双曲函数
- **格式化**：没有 `ToString(format)` / 千分位 / 定宽补零；`BigInt` 换进制只有 `ToBase(radix)`，`Decimal` / `Complex` 只有固定形态的 `ToString()`

## _MagDivModResult

[`src/libraries/z42.numerics/src/BigInt.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L39)

幅值除法的商 + 余数载体（内部）。

z42 当前不支持 `int[][]`（jagged 2D 数组），把 q + r 包成 class 透传比 sentinel-pack 更显式。

```z42
public class _MagDivModResult {
    int[] q
    int[] r
    _MagDivModResult(int[] q, int[] r)
}
```

成员：[`q`](#Std.Numerics._MagDivModResult.q) · [`r`](#Std.Numerics._MagDivModResult.r) · [`_MagDivModResult`](#Std.Numerics._MagDivModResult.ctor(int[],int[]))

<a id="Std.Numerics._MagDivModResult.q"></a>
### int[] q

[`src/libraries/z42.numerics/src/BigInt.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L41)

商（规范化 limb 数组）。

<a id="Std.Numerics._MagDivModResult.r"></a>
### int[] r

[`src/libraries/z42.numerics/src/BigInt.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L43)

余数（规范化 limb 数组）。

<a id="Std.Numerics._MagDivModResult.ctor(int[],int[])"></a>
### _MagDivModResult(int[] q, int[] r)

[`src/libraries/z42.numerics/src/BigInt.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L45)

以商与余数构造载体。

## BigInt

[`src/libraries/z42.numerics/src/BigInt.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L55)

任意精度整数：sign + 31-bit limb 幅值，不可变引用类型（运算返回新对象）。

内部表示：`_mag` 为 31-bit little-endian limbs（每个 ∈ [0, BASE)），`_sign` ∈ {-1, 0, +1}；
规范化要求无尾随零 limb（`_mag.Length == 0` iff `_sign == 0`）。包级概览见本文件头部 `//!`。

```z42
public class BigInt {
    static BigInt Zero
    static BigInt One
    static BigInt MinusOne
    static int LIMB_BITS
    static long BASE
    int[] _mag
    int _sign
    BigInt(long value)
    static BigInt Parse(string s)
    static BigInt ParseHex(string s)
    bool IsZero()
    bool IsOne()
    bool IsNegative()
    int Sign()
    int ToInt32()
    long ToInt64()
    string ToBase(int radix)
    string ToHex()
    BigInt Add(BigInt other)
    BigInt Subtract(BigInt other)
    BigInt Multiply(BigInt other)
    BigInt Divide(BigInt other)
    BigInt Mod(BigInt other)
    BigInt Negate()
    BigInt Abs()
    BigInt Pow(int exponent)
    BigInt And(BigInt other)
    BigInt Or(BigInt other)
    BigInt Xor(BigInt other)
    BigInt BitNot()
    BigInt ShiftLeft(int n)
    BigInt ShiftRight(int n)
    bool TestBit(int n)
    int BitLength()
    BigInt ModPow(BigInt exp, BigInt modulus)
    BigInt ModInverse(BigInt modulus)
    bool IsProbablyPrime(int rounds)
    bool IsProbablyPrime(int rounds, Random rng)
    BigInt NextPrime()
    bool IsPrime()
    bool IsBpswPrime()
    BigInt Gcd(BigInt other)
    BigInt Lcm(BigInt other)
    int CompareTo(BigInt other)
}
```

成员：[`Zero`](#Std.Numerics.BigInt.Zero) · [`One`](#Std.Numerics.BigInt.One) · [`MinusOne`](#Std.Numerics.BigInt.MinusOne) · [`LIMB_BITS`](#Std.Numerics.BigInt.LIMB_BITS) · [`BASE`](#Std.Numerics.BigInt.BASE) · [`_mag`](#Std.Numerics.BigInt._mag) · [`_sign`](#Std.Numerics.BigInt._sign) · [`BigInt`](#Std.Numerics.BigInt.ctor(long)) · [`Parse`](#Std.Numerics.BigInt.Parse(string)) · [`ParseHex`](#Std.Numerics.BigInt.ParseHex(string)) · [`IsZero`](#Std.Numerics.BigInt.IsZero()) · [`IsOne`](#Std.Numerics.BigInt.IsOne()) · [`IsNegative`](#Std.Numerics.BigInt.IsNegative()) · [`Sign`](#Std.Numerics.BigInt.Sign()) · [`ToInt32`](#Std.Numerics.BigInt.ToInt32()) · [`ToInt64`](#Std.Numerics.BigInt.ToInt64()) · [`ToBase`](#Std.Numerics.BigInt.ToBase(int)) · [`ToHex`](#Std.Numerics.BigInt.ToHex()) · [`Add`](#Std.Numerics.BigInt.Add(BigInt)) · [`Subtract`](#Std.Numerics.BigInt.Subtract(BigInt)) · [`Multiply`](#Std.Numerics.BigInt.Multiply(BigInt)) · [`Divide`](#Std.Numerics.BigInt.Divide(BigInt)) · [`Mod`](#Std.Numerics.BigInt.Mod(BigInt)) · [`Negate`](#Std.Numerics.BigInt.Negate()) · [`Abs`](#Std.Numerics.BigInt.Abs()) · [`Pow`](#Std.Numerics.BigInt.Pow(int)) · [`And`](#Std.Numerics.BigInt.And(BigInt)) · [`Or`](#Std.Numerics.BigInt.Or(BigInt)) · [`Xor`](#Std.Numerics.BigInt.Xor(BigInt)) · [`BitNot`](#Std.Numerics.BigInt.BitNot()) · [`ShiftLeft`](#Std.Numerics.BigInt.ShiftLeft(int)) · [`ShiftRight`](#Std.Numerics.BigInt.ShiftRight(int)) · [`TestBit`](#Std.Numerics.BigInt.TestBit(int)) · [`BitLength`](#Std.Numerics.BigInt.BitLength()) · [`ModPow`](#Std.Numerics.BigInt.ModPow(BigInt,BigInt)) · [`ModInverse`](#Std.Numerics.BigInt.ModInverse(BigInt)) · [`IsProbablyPrime`](#Std.Numerics.BigInt.IsProbablyPrime(int)) · [`IsProbablyPrime`](#Std.Numerics.BigInt.IsProbablyPrime(int,Random)) · [`NextPrime`](#Std.Numerics.BigInt.NextPrime()) · [`IsPrime`](#Std.Numerics.BigInt.IsPrime()) · [`IsBpswPrime`](#Std.Numerics.BigInt.IsBpswPrime()) · [`Gcd`](#Std.Numerics.BigInt.Gcd(BigInt)) · [`Lcm`](#Std.Numerics.BigInt.Lcm(BigInt)) · [`CompareTo`](#Std.Numerics.BigInt.CompareTo(BigInt))

<a id="Std.Numerics.BigInt.Zero"></a>
### static BigInt Zero

[`src/libraries/z42.numerics/src/BigInt.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L60)

零。

<a id="Std.Numerics.BigInt.One"></a>
### static BigInt One

[`src/libraries/z42.numerics/src/BigInt.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L62)

一。

<a id="Std.Numerics.BigInt.MinusOne"></a>
### static BigInt MinusOne

[`src/libraries/z42.numerics/src/BigInt.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L64)

负一。

<a id="Std.Numerics.BigInt.LIMB_BITS"></a>
### static int LIMB_BITS

[`src/libraries/z42.numerics/src/BigInt.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L71)

每个 limb 的位数（31）。

limb 取值 `[0, BASE)`，存进 z42 `int`（signed i32）时正数范围不与符号位重叠。

<a id="Std.Numerics.BigInt.BASE"></a>
### static long BASE

[`src/libraries/z42.numerics/src/BigInt.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L73)

limb 基数，`2^31 = 2147483648`。

<a id="Std.Numerics.BigInt._mag"></a>
### int[] _mag

[`src/libraries/z42.numerics/src/BigInt.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L78)

幅值：31-bit little-endian limbs（每个 ∈ [0, BASE)，无尾随零）。

<a id="Std.Numerics.BigInt._sign"></a>
### int _sign

[`src/libraries/z42.numerics/src/BigInt.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L80)

符号：-1 负 / 0 零 / +1 正（零时 `_mag.Length == 0`）。

<a id="Std.Numerics.BigInt.ctor(long)"></a>
### BigInt(long value)

[`src/libraries/z42.numerics/src/BigInt.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L85)

用 `long` 构造任意精度整数（`int` 实参走 int→long 隐式加宽）。

<a id="Std.Numerics.BigInt.Parse(string)"></a>
### static BigInt Parse(string s)

[`src/libraries/z42.numerics/src/BigInt.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L108)

解析十进制表示，允许前导 `+` / `-`。空串或非法字符抛 `FormatException`。

<a id="Std.Numerics.BigInt.ParseHex(string)"></a>
### static BigInt ParseHex(string s)

[`src/libraries/z42.numerics/src/BigInt.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L155)

解析十六进制（大小写不敏感），允许 `0x` 前缀与 `-`。非法输入抛 `FormatException`。

<a id="Std.Numerics.BigInt.IsZero()"></a>
### bool IsZero()

[`src/libraries/z42.numerics/src/BigInt.z42:196`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L196)

是否为零。

<a id="Std.Numerics.BigInt.IsOne()"></a>
### bool IsOne()

[`src/libraries/z42.numerics/src/BigInt.z42:201`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L201)

是否为一。

<a id="Std.Numerics.BigInt.IsNegative()"></a>
### bool IsNegative()

[`src/libraries/z42.numerics/src/BigInt.z42:208`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L208)

是否为负。

<a id="Std.Numerics.BigInt.Sign()"></a>
### int Sign()

[`src/libraries/z42.numerics/src/BigInt.z42:213`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L213)

符号：-1 / 0 / +1。

<a id="Std.Numerics.BigInt.ToInt32()"></a>
### int ToInt32()

[`src/libraries/z42.numerics/src/BigInt.z42:220`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L220)

转为 `int`，超出 i32 范围抛 `OverflowException`。

<a id="Std.Numerics.BigInt.ToInt64()"></a>
### long ToInt64()

[`src/libraries/z42.numerics/src/BigInt.z42:229`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L229)

转为 `long`，超出 i64 范围抛 `OverflowException`。

<a id="Std.Numerics.BigInt.ToBase(int)"></a>
### string ToBase(int radix)

[`src/libraries/z42.numerics/src/BigInt.z42:319`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L319)

转成 `radix` 进制字符串（`radix` ∈ [2, 36]，小写字母，负数带 `-`）。

<a id="Std.Numerics.BigInt.ToHex()"></a>
### string ToHex()

[`src/libraries/z42.numerics/src/BigInt.z42:370`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L370)

小写十六进制，无 `0x` 前缀；零为 `"0"`，负数带 `-`。

<a id="Std.Numerics.BigInt.Add(BigInt)"></a>
### BigInt Add(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:414`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L414)

加法。

<a id="Std.Numerics.BigInt.Subtract(BigInt)"></a>
### BigInt Subtract(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:434`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L434)

减法。

<a id="Std.Numerics.BigInt.Multiply(BigInt)"></a>
### BigInt Multiply(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:439`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L439)

乘法。

<a id="Std.Numerics.BigInt.Divide(BigInt)"></a>
### BigInt Divide(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:447`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L447)

除法，向零截断。

<a id="Std.Numerics.BigInt.Mod(BigInt)"></a>
### BigInt Mod(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:453`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L453)

取模，余数符号跟被除数。

<a id="Std.Numerics.BigInt.Negate()"></a>
### BigInt Negate()

[`src/libraries/z42.numerics/src/BigInt.z42:497`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L497)

取相反数。

<a id="Std.Numerics.BigInt.Abs()"></a>
### BigInt Abs()

[`src/libraries/z42.numerics/src/BigInt.z42:503`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L503)

取绝对值。

<a id="Std.Numerics.BigInt.Pow(int)"></a>
### BigInt Pow(int exponent)

[`src/libraries/z42.numerics/src/BigInt.z42:509`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L509)

整数幂（`exponent >= 0`），用重复平方（O(log exp) 次乘法）。负指数抛 `ArgumentException`。

<a id="Std.Numerics.BigInt.And(BigInt)"></a>
### BigInt And(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:543`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L543)

位与（AND）：两操作数按无限位 two's-complement 相与，均为负时结果才负。

结果符号由 result_lead = a_lead AND b_lead 决定。

<a id="Std.Numerics.BigInt.Or(BigInt)"></a>
### BigInt Or(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:565`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L565)

位或 (OR). 两个操作数 two's-complement OR；结果符号 = a_lead OR b_lead (任一负则负).

<a id="Std.Numerics.BigInt.Xor(BigInt)"></a>
### BigInt Xor(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:590`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L590)

位异或 (XOR). 两个操作数 two's-complement XOR；结果符号 = a_lead XOR b_lead (恰好一个负才负).

<a id="Std.Numerics.BigInt.BitNot()"></a>
### BigInt BitNot()

[`src/libraries/z42.numerics/src/BigInt.z42:617`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L617)

位非（NOT）：按代数恒等式 `~x == -(x + 1)` 计算。

命名为 `BitNot` 而非 `Not`，避开 z42 method resolution 的 name-only
dispatch 隐患（与未来潜在的 boolean `Not()` 同名风险）。

<a id="Std.Numerics.BigInt.ShiftLeft(int)"></a>
### BigInt ShiftLeft(int n)

[`src/libraries/z42.numerics/src/BigInt.z42:718`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L718)

左移 n 位 (n >= 0)。Magnitude 左移；符号保留 (-8 << 1 == -16). n 为负数抛 ArgumentException.

<a id="Std.Numerics.BigInt.ShiftRight(int)"></a>
### BigInt ShiftRight(int n)

[`src/libraries/z42.numerics/src/BigInt.z42:732`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L732)

右移 n 位（n >= 0）：幅值逻辑右移，符号保留（`-8 >> 1 == -4`）。负 n 抛 `ArgumentException`。

是逻辑 magnitude shift 而非算术右移（.NET BigInteger 对负数算术移位会向 -∞ 取整）；
要算术语义请用 `Divide(2^k)`。

<a id="Std.Numerics.BigInt.TestBit(int)"></a>
### bool TestBit(int n)

[`src/libraries/z42.numerics/src/BigInt.z42:747`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L747)

测试第 `n` 位（`n >= 0`）是否为 1；`n` 为负抛 `ArgumentException`。

负数按无限精度 two's-complement 解释（同 Python `(n >> i) & 1`）：
`(-1).TestBit(i)` 对任意 `i` 都为 true（符号扩展的 1）。

<a id="Std.Numerics.BigInt.BitLength()"></a>
### int BitLength()

[`src/libraries/z42.numerics/src/BigInt.z42:768`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L768)

表示该数值（绝对值）所需的最少 bit 数，永远 `>= 0`。

`Zero.BitLength() == 0`、`One.BitLength() == 1`、`255.BitLength() == 8`、
`(-256).BitLength() == 9`。

<a id="Std.Numerics.BigInt.ModPow(BigInt,BigInt)"></a>
### BigInt ModPow(BigInt exp, BigInt modulus)

[`src/libraries/z42.numerics/src/BigInt.z42:787`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L787)

模幂 `this^exp mod modulus`（负指数经 ModInverse；奇模数走 Montgomery REDC）。

实现见 BigIntModular。

<a id="Std.Numerics.BigInt.ModInverse(BigInt)"></a>
### BigInt ModInverse(BigInt modulus)

[`src/libraries/z42.numerics/src/BigInt.z42:790`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L790)

Modular multiplicative inverse in `[0, modulus)`（扩展欧几里得）。见 BigIntModular。

<a id="Std.Numerics.BigInt.IsProbablyPrime(int)"></a>
### bool IsProbablyPrime(int rounds)

[`src/libraries/z42.numerics/src/BigInt.z42:793`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L793)

Miller-Rabin 概率素性检测，用挂钟播种的默认 RNG（结果不可复现）。要可复现就用 `(int, Random)` 重载。

<a id="Std.Numerics.BigInt.IsProbablyPrime(int,Random)"></a>
### bool IsProbablyPrime(int rounds, Random rng)

[`src/libraries/z42.numerics/src/BigInt.z42:799`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L799)

Miller-Rabin with caller-provided RNG（错误率 ≤ 4^-rounds）。见 BigIntPrimality。

<a id="Std.Numerics.BigInt.NextPrime()"></a>
### BigInt NextPrime()

[`src/libraries/z42.numerics/src/BigInt.z42:804`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L804)

严格大于 `this` 的最小（概率）素数（6k±1 轮转 + 小素数试除 + 20 轮 MR）。

实现见 BigIntPrimality。

<a id="Std.Numerics.BigInt.IsPrime()"></a>
### bool IsPrime()

[`src/libraries/z42.numerics/src/BigInt.z42:810`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L810)

确定性素性检测（OEIS A014233 见证集），范围内零错误；超限抛 `ArgumentException`。

范围上限 3,317,044,064,679,887,385,961,981，更大的 n 用 `IsProbablyPrime(rounds)`。
实现见 BigIntPrimality。

<a id="Std.Numerics.BigInt.IsBpswPrime()"></a>
### bool IsBpswPrime()

[`src/libraries/z42.numerics/src/BigInt.z42:813`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L813)

BPSW primality（强 MR 基 2 + 强 Lucas）。见 BigIntBpsw。

<a id="Std.Numerics.BigInt.Gcd(BigInt)"></a>
### BigInt Gcd(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:831`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L831)

`|this|` 与 `|other|` 的最大公约数，结果非负（`gcd(0,0)==0`，`gcd(a,0)==|a|`）。

用 Stein 二进制 GCD：每步是移位 + 比较 + 减法，而非完整 divmod，对加密规模
操作数约快 3-5×（BigInt.Divide 是 O(n*m)，ShiftRight 是 O(n)，Subtract 是 O(max(n,m))）。

迭代形式（无递归）：
  1. 提出公共因子 2：两者同偶时 `a, b ← a/2, b/2`，记移位次数 k，最后乘回 2^k。
  2. 剥掉 a 剩余因子 2 直到 a 为奇。
  3. 主循环：b 剥 2 到奇，保证 `a ≤ b`，令 `b ← b - a`（奇-奇=偶，下一轮至少再剥一个 2），直到 `b == 0`。
  4. 返回 `a << k`。

<a id="Std.Numerics.BigInt.Lcm(BigInt)"></a>
### BigInt Lcm(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:869`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L869)

`|this|` 与 `|other|` 的最小公倍数，结果非负；`lcm(0, _) == 0`。

<a id="Std.Numerics.BigInt.CompareTo(BigInt)"></a>
### int CompareTo(BigInt other)

[`src/libraries/z42.numerics/src/BigInt.z42:881`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/BigInt.z42#L881)

与另一个 BigInt 比较，返回 -1 / 0 / +1。

## Complex

[`src/libraries/z42.numerics/src/Complex.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L5)

复数：`double` 实部 + 虚部。所有运算返回新对象。

```z42
public sealed class Complex {
    double Real
    double Imaginary
    Complex(double real, double imaginary)
    static Complex Zero()
    static Complex One()
    static Complex ImaginaryOne()
    static Complex FromPolar(double magnitude, double phase)
    double Magnitude()
    double Phase()
    bool IsZero()
    bool IsReal()
    bool IsImaginary()
    bool Equals(Complex other)
    Complex Add(Complex other)
    Complex Subtract(Complex other)
    Complex Multiply(Complex other)
    Complex MultiplyReal(double s)
    Complex Divide(Complex other)
    Complex Negate()
    Complex Conjugate()
    Complex Reciprocal()
    static Complex Exp(Complex z)
    static Complex Log(Complex z)
    static Complex Sqrt(Complex z)
    static Complex Sin(Complex z)
    static Complex Cos(Complex z)
    static Complex Pow(Complex z, Complex w)
    override string ToString()
}
```

成员：[`Real`](#Std.Numerics.Complex.Real) · [`Imaginary`](#Std.Numerics.Complex.Imaginary) · [`Complex`](#Std.Numerics.Complex.ctor(double,double)) · [`Zero`](#Std.Numerics.Complex.Zero()) · [`One`](#Std.Numerics.Complex.One()) · [`ImaginaryOne`](#Std.Numerics.Complex.ImaginaryOne()) · [`FromPolar`](#Std.Numerics.Complex.FromPolar(double,double)) · [`Magnitude`](#Std.Numerics.Complex.Magnitude()) · [`Phase`](#Std.Numerics.Complex.Phase()) · [`IsZero`](#Std.Numerics.Complex.IsZero()) · [`IsReal`](#Std.Numerics.Complex.IsReal()) · [`IsImaginary`](#Std.Numerics.Complex.IsImaginary()) · [`Equals`](#Std.Numerics.Complex.Equals(Complex)) · [`Add`](#Std.Numerics.Complex.Add(Complex)) · [`Subtract`](#Std.Numerics.Complex.Subtract(Complex)) · [`Multiply`](#Std.Numerics.Complex.Multiply(Complex)) · [`MultiplyReal`](#Std.Numerics.Complex.MultiplyReal(double)) · [`Divide`](#Std.Numerics.Complex.Divide(Complex)) · [`Negate`](#Std.Numerics.Complex.Negate()) · [`Conjugate`](#Std.Numerics.Complex.Conjugate()) · [`Reciprocal`](#Std.Numerics.Complex.Reciprocal()) · [`Exp`](#Std.Numerics.Complex.Exp(Complex)) · [`Log`](#Std.Numerics.Complex.Log(Complex)) · [`Sqrt`](#Std.Numerics.Complex.Sqrt(Complex)) · [`Sin`](#Std.Numerics.Complex.Sin(Complex)) · [`Cos`](#Std.Numerics.Complex.Cos(Complex)) · [`Pow`](#Std.Numerics.Complex.Pow(Complex,Complex)) · [`ToString`](#Std.Numerics.Complex.ToString())

<a id="Std.Numerics.Complex.Real"></a>
### double Real

[`src/libraries/z42.numerics/src/Complex.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L7)

实部（公开字段）。

<a id="Std.Numerics.Complex.Imaginary"></a>
### double Imaginary

[`src/libraries/z42.numerics/src/Complex.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L9)

虚部（公开字段）。

<a id="Std.Numerics.Complex.ctor(double,double)"></a>
### Complex(double real, double imaginary)

[`src/libraries/z42.numerics/src/Complex.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L12)

用实部与虚部构造复数。

<a id="Std.Numerics.Complex.Zero()"></a>
### static Complex Zero()

[`src/libraries/z42.numerics/src/Complex.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L22)

零（0 + 0i）。

<a id="Std.Numerics.Complex.One()"></a>
### static Complex One()

[`src/libraries/z42.numerics/src/Complex.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L27)

一（1 + 0i）。

<a id="Std.Numerics.Complex.ImaginaryOne()"></a>
### static Complex ImaginaryOne()

[`src/libraries/z42.numerics/src/Complex.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L32)

虚数单位（0 + 1i）。

<a id="Std.Numerics.Complex.FromPolar(double,double)"></a>
### static Complex FromPolar(double magnitude, double phase)

[`src/libraries/z42.numerics/src/Complex.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L39)

由极坐标（模、相位弧度）构造；`magnitude < 0` 会被接受（相位旋转 π）。

<a id="Std.Numerics.Complex.Magnitude()"></a>
### double Magnitude()

[`src/libraries/z42.numerics/src/Complex.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L45)

模 `|z| = sqrt(real² + imag²)`。

<a id="Std.Numerics.Complex.Phase()"></a>
### double Phase()

[`src/libraries/z42.numerics/src/Complex.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L50)

辐角（弧度），范围 (-π, π]，即 atan2(imag, real)。

<a id="Std.Numerics.Complex.IsZero()"></a>
### bool IsZero()

[`src/libraries/z42.numerics/src/Complex.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L57)

是否为零。

<a id="Std.Numerics.Complex.IsReal()"></a>
### bool IsReal()

[`src/libraries/z42.numerics/src/Complex.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L62)

是否为实数（虚部为 0）。

<a id="Std.Numerics.Complex.IsImaginary()"></a>
### bool IsImaginary()

[`src/libraries/z42.numerics/src/Complex.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L67)

是否为纯虚数（实部为 0 且虚部非 0）。

<a id="Std.Numerics.Complex.Equals(Complex)"></a>
### bool Equals(Complex other)

[`src/libraries/z42.numerics/src/Complex.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L72)

精确的 `double` 相等比较。

<a id="Std.Numerics.Complex.Add(Complex)"></a>
### Complex Add(Complex other)

[`src/libraries/z42.numerics/src/Complex.z42:79`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L79)

复数加法。

<a id="Std.Numerics.Complex.Subtract(Complex)"></a>
### Complex Subtract(Complex other)

[`src/libraries/z42.numerics/src/Complex.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L85)

复数减法。

<a id="Std.Numerics.Complex.Multiply(Complex)"></a>
### Complex Multiply(Complex other)

[`src/libraries/z42.numerics/src/Complex.z42:91`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L91)

复数乘法：`(a + bi)(c + di) = (ac - bd) + (ad + bc)i`。

<a id="Std.Numerics.Complex.MultiplyReal(double)"></a>
### Complex MultiplyReal(double s)

[`src/libraries/z42.numerics/src/Complex.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L100)

乘以实数标量（避免构造 `Complex(s, 0)`）。

<a id="Std.Numerics.Complex.Divide(Complex)"></a>
### Complex Divide(Complex other)

[`src/libraries/z42.numerics/src/Complex.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L106)

复数除法。除以零不抛异常，按 IEEE 浮点规则得 (NaN, NaN)。 用 Smith 算法取 |c|、|d| 较大者缩放，避免中间溢出。

<a id="Std.Numerics.Complex.Negate()"></a>
### Complex Negate()

[`src/libraries/z42.numerics/src/Complex.z42:130`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L130)

相反数：`-(a + bi) = -a - bi`。

<a id="Std.Numerics.Complex.Conjugate()"></a>
### Complex Conjugate()

[`src/libraries/z42.numerics/src/Complex.z42:135`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L135)

共轭：`(a + bi)* = a - bi`。

<a id="Std.Numerics.Complex.Reciprocal()"></a>
### Complex Reciprocal()

[`src/libraries/z42.numerics/src/Complex.z42:140`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L140)

倒数：`1 / z = z* / |z|²`（`0` 的倒数返回 `Zero()`）。

<a id="Std.Numerics.Complex.Exp(Complex)"></a>
### static Complex Exp(Complex z)

[`src/libraries/z42.numerics/src/Complex.z42:152`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L152)

指数：`e^z = e^Real * (cos(Imag) + i*sin(Imag))`。

<a id="Std.Numerics.Complex.Log(Complex)"></a>
### static Complex Log(Complex z)

[`src/libraries/z42.numerics/src/Complex.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L159)

自然对数主支：`ln(z) = ln|z| + i*arg(z)`（`ln(0)` 按 IEEE 返回 NaN）。

<a id="Std.Numerics.Complex.Sqrt(Complex)"></a>
### static Complex Sqrt(Complex z)

[`src/libraries/z42.numerics/src/Complex.z42:164`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L164)

主平方根：`sqrt(r·e^(iθ)) = sqrt(r) · e^(iθ/2)`。

<a id="Std.Numerics.Complex.Sin(Complex)"></a>
### static Complex Sin(Complex z)

[`src/libraries/z42.numerics/src/Complex.z42:174`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L174)

正弦：`sin(a + bi) = sin(a)·cosh(b) + i·cos(a)·sinh(b)`。

<a id="Std.Numerics.Complex.Cos(Complex)"></a>
### static Complex Cos(Complex z)

[`src/libraries/z42.numerics/src/Complex.z42:182`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L182)

余弦：`cos(a + bi) = cos(a)·cosh(b) - i·sin(a)·sinh(b)`。

<a id="Std.Numerics.Complex.Pow(Complex,Complex)"></a>
### static Complex Pow(Complex z, Complex w)

[`src/libraries/z42.numerics/src/Complex.z42:190`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L190)

幂：`z^w = exp(w · ln(z))`（`0^0` → 1）。

<a id="Std.Numerics.Complex.ToString()"></a>
### override string ToString()

[`src/libraries/z42.numerics/src/Complex.z42:201`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Complex.z42#L201)

字符串形式 `"(real, imag)"`。

## Decimal

[`src/libraries/z42.numerics/src/Decimal.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L8)

任意精度十进制定点：`BigInt` 尾数 + 非负十进制 `scale`，表示 `mantissa * 10^-scale`。

没有精度上限（乘法得 2N 位结果，除法显式指定结果位数）。按不可变使用，每个
运算返回新对象。

```z42
public sealed class Decimal {
    BigInt _mantissa
    int _scale
    Decimal(BigInt mantissa, int scale)
    Decimal(int value)
    static Decimal Zero()
    static Decimal One()
    static Decimal FromInt(int value)
    static Decimal FromLong(long value)
    static Decimal Parse(string s)
    bool IsZero()
    bool IsNegative()
    int Sign()
    BigInt Mantissa()
    int Scale()
    Decimal Add(Decimal other)
    Decimal Subtract(Decimal other)
    Decimal Multiply(Decimal other)
    Decimal DivideBy(Decimal other, int resultScale)
    Decimal Negate()
    Decimal Abs()
    int CompareTo(Decimal other)
    bool Equals(Decimal other)
    override string ToString()
}
```

成员：[`_mantissa`](#Std.Numerics.Decimal._mantissa) · [`_scale`](#Std.Numerics.Decimal._scale) · [`Decimal`](#Std.Numerics.Decimal.ctor(BigInt,int)) · [`Decimal`](#Std.Numerics.Decimal.ctor(int)) · [`Zero`](#Std.Numerics.Decimal.Zero()) · [`One`](#Std.Numerics.Decimal.One()) · [`FromInt`](#Std.Numerics.Decimal.FromInt(int)) · [`FromLong`](#Std.Numerics.Decimal.FromLong(long)) · [`Parse`](#Std.Numerics.Decimal.Parse(string)) · [`IsZero`](#Std.Numerics.Decimal.IsZero()) · [`IsNegative`](#Std.Numerics.Decimal.IsNegative()) · [`Sign`](#Std.Numerics.Decimal.Sign()) · [`Mantissa`](#Std.Numerics.Decimal.Mantissa()) · [`Scale`](#Std.Numerics.Decimal.Scale()) · [`Add`](#Std.Numerics.Decimal.Add(Decimal)) · [`Subtract`](#Std.Numerics.Decimal.Subtract(Decimal)) · [`Multiply`](#Std.Numerics.Decimal.Multiply(Decimal)) · [`DivideBy`](#Std.Numerics.Decimal.DivideBy(Decimal,int)) · [`Negate`](#Std.Numerics.Decimal.Negate()) · [`Abs`](#Std.Numerics.Decimal.Abs()) · [`CompareTo`](#Std.Numerics.Decimal.CompareTo(Decimal)) · [`Equals`](#Std.Numerics.Decimal.Equals(Decimal)) · [`ToString`](#Std.Numerics.Decimal.ToString())

<a id="Std.Numerics.Decimal._mantissa"></a>
### BigInt _mantissa

[`src/libraries/z42.numerics/src/Decimal.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L10)

尾数（有符号 BigInt）；数值 = _mantissa * 10^-_scale。

<a id="Std.Numerics.Decimal._scale"></a>
### int _scale

[`src/libraries/z42.numerics/src/Decimal.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L12)

十进制 scale（小数位数），恒非负——负 scale 表达的尾零段归一化进尾数。

<a id="Std.Numerics.Decimal.ctor(BigInt,int)"></a>
### Decimal(BigInt mantissa, int scale)

[`src/libraries/z42.numerics/src/Decimal.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L15)

用尾数与 scale 构造定点数（`scale >= 0`）。

<a id="Std.Numerics.Decimal.ctor(int)"></a>
### Decimal(int value)

[`src/libraries/z42.numerics/src/Decimal.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L24)

用整数构造定点数（scale 为 0）。

<a id="Std.Numerics.Decimal.Zero()"></a>
### static Decimal Zero()

[`src/libraries/z42.numerics/src/Decimal.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L32)

零值定点数（0，scale 0）。

<a id="Std.Numerics.Decimal.One()"></a>
### static Decimal One()

[`src/libraries/z42.numerics/src/Decimal.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L37)

一值定点数（1，scale 0）。

<a id="Std.Numerics.Decimal.FromInt(int)"></a>
### static Decimal FromInt(int value)

[`src/libraries/z42.numerics/src/Decimal.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L42)

从 `int` 构造定点数。

<a id="Std.Numerics.Decimal.FromLong(long)"></a>
### static Decimal FromLong(long value)

[`src/libraries/z42.numerics/src/Decimal.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L47)

从 `long` 构造定点数。

<a id="Std.Numerics.Decimal.Parse(string)"></a>
### static Decimal Parse(string s)

[`src/libraries/z42.numerics/src/Decimal.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L52)

解析十进制字面量 `[-]?digit+([.]digit+)?`（不支持指数/千分位）。非法输入抛 `FormatException`。

<a id="Std.Numerics.Decimal.IsZero()"></a>
### bool IsZero()

[`src/libraries/z42.numerics/src/Decimal.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L103)

是否为零。

<a id="Std.Numerics.Decimal.IsNegative()"></a>
### bool IsNegative()

[`src/libraries/z42.numerics/src/Decimal.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L108)

是否为负。

<a id="Std.Numerics.Decimal.Sign()"></a>
### int Sign()

[`src/libraries/z42.numerics/src/Decimal.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L113)

符号：-1 / 0 / +1。

<a id="Std.Numerics.Decimal.Mantissa()"></a>
### BigInt Mantissa()

[`src/libraries/z42.numerics/src/Decimal.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L118)

返回尾数（`BigInt`）。

<a id="Std.Numerics.Decimal.Scale()"></a>
### int Scale()

[`src/libraries/z42.numerics/src/Decimal.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L123)

返回 scale（小数位数）。

<a id="Std.Numerics.Decimal.Add(Decimal)"></a>
### Decimal Add(Decimal other)

[`src/libraries/z42.numerics/src/Decimal.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L154)

加法（结果 scale 取两边较大者）。

<a id="Std.Numerics.Decimal.Subtract(Decimal)"></a>
### Decimal Subtract(Decimal other)

[`src/libraries/z42.numerics/src/Decimal.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L162)

减法（结果 scale 取两边较大者）。

<a id="Std.Numerics.Decimal.Multiply(Decimal)"></a>
### Decimal Multiply(Decimal other)

[`src/libraries/z42.numerics/src/Decimal.z42:170`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L170)

乘法（结果 scale 为两边之和）。

<a id="Std.Numerics.Decimal.DivideBy(Decimal,int)"></a>
### Decimal DivideBy(Decimal other, int resultScale)

[`src/libraries/z42.numerics/src/Decimal.z42:181`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L181)

除法，显式指定结果位数（`resultScale` 位小数）。向零截断，无舍入模式。

<a id="Std.Numerics.Decimal.Negate()"></a>
### Decimal Negate()

[`src/libraries/z42.numerics/src/Decimal.z42:211`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L211)

取相反数。

<a id="Std.Numerics.Decimal.Abs()"></a>
### Decimal Abs()

[`src/libraries/z42.numerics/src/Decimal.z42:216`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L216)

取绝对值。

<a id="Std.Numerics.Decimal.CompareTo(Decimal)"></a>
### int CompareTo(Decimal other)

[`src/libraries/z42.numerics/src/Decimal.z42:223`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L223)

比较（跨 scale 对齐后比较，返回 -1 / 0 / +1）。

<a id="Std.Numerics.Decimal.Equals(Decimal)"></a>
### bool Equals(Decimal other)

[`src/libraries/z42.numerics/src/Decimal.z42:231`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L231)

按数值比较，跨 scale 相等（`Parse("19.99").Equals(Parse("19.990")) == true`）。

<a id="Std.Numerics.Decimal.ToString()"></a>
### override string ToString()

[`src/libraries/z42.numerics/src/Decimal.z42:238`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.numerics/src/Decimal.z42#L238)

十进制字符串，保留存储的 scale（`Parse("1.00").ToString() == "1.00"`），负数带 `-`。

