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

`src/libraries/z42.numerics/src/BigInt.z42:38`


```z42
public class _MagDivModResult {
    int[] q
    int[] r
    _MagDivModResult(int[] q, int[] r)
}
```

### int[] q

`src/libraries/z42.numerics/src/BigInt.z42:39`


### int[] r

`src/libraries/z42.numerics/src/BigInt.z42:40`


### _MagDivModResult(int[] q, int[] r)

`src/libraries/z42.numerics/src/BigInt.z42:41`


## BigInt

`src/libraries/z42.numerics/src/BigInt.z42:47`


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

### static BigInt Zero

`src/libraries/z42.numerics/src/BigInt.z42:52`

零。

### static BigInt One

`src/libraries/z42.numerics/src/BigInt.z42:54`

一。

### static BigInt MinusOne

`src/libraries/z42.numerics/src/BigInt.z42:56`

负一。

### static int LIMB_BITS

`src/libraries/z42.numerics/src/BigInt.z42:63`


### static long BASE

`src/libraries/z42.numerics/src/BigInt.z42:64`


### int[] _mag

`src/libraries/z42.numerics/src/BigInt.z42:68`


### int _sign

`src/libraries/z42.numerics/src/BigInt.z42:69`


### BigInt(long value)

`src/libraries/z42.numerics/src/BigInt.z42:74`

用 `long` 构造任意精度整数（`int` 实参走 int→long 隐式加宽）。

### static BigInt Parse(string s)

`src/libraries/z42.numerics/src/BigInt.z42:97`

解析十进制表示，允许前导 `+` / `-`。空串或非法字符抛 `FormatException`。

### static BigInt ParseHex(string s)

`src/libraries/z42.numerics/src/BigInt.z42:144`

解析十六进制（大小写不敏感），允许 `0x` 前缀与 `-`。非法输入抛 `FormatException`。

### bool IsZero()

`src/libraries/z42.numerics/src/BigInt.z42:185`

是否为零。

### bool IsOne()

`src/libraries/z42.numerics/src/BigInt.z42:190`

是否为一。

### bool IsNegative()

`src/libraries/z42.numerics/src/BigInt.z42:197`

是否为负。

### int Sign()

`src/libraries/z42.numerics/src/BigInt.z42:202`

符号：-1 / 0 / +1。

### int ToInt32()

`src/libraries/z42.numerics/src/BigInt.z42:209`

转为 `int`，超出 i32 范围抛 `OverflowException`。

### long ToInt64()

`src/libraries/z42.numerics/src/BigInt.z42:218`

转为 `long`，超出 i64 范围抛 `OverflowException`。

### string ToBase(int radix)

`src/libraries/z42.numerics/src/BigInt.z42:308`

转成 `radix` 进制字符串（`radix` ∈ [2, 36]，小写字母，负数带 `-`）。

### string ToHex()

`src/libraries/z42.numerics/src/BigInt.z42:359`

小写十六进制，无 `0x` 前缀；零为 `"0"`，负数带 `-`。

### BigInt Add(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:403`

加法。

### BigInt Subtract(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:423`

减法。

### BigInt Multiply(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:428`

乘法。

### BigInt Divide(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:436`

除法，向零截断。

### BigInt Mod(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:442`

取模，余数符号跟被除数。

### BigInt Negate()

`src/libraries/z42.numerics/src/BigInt.z42:486`

取相反数。

### BigInt Abs()

`src/libraries/z42.numerics/src/BigInt.z42:492`

取绝对值。

### BigInt Pow(int exponent)

`src/libraries/z42.numerics/src/BigInt.z42:498`

整数幂（`exponent >= 0`），用重复平方（O(log exp) 次乘法）。负指数抛 `ArgumentException`。

### BigInt And(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:531`

位与 (AND). 两个操作数都按无限位 two's-complement 表示后逐限位相与； 结果符号由 result_lead = a_lead AND b_lead 决定 (两个都负才负)。

### BigInt Or(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:553`

位或 (OR). 两个操作数 two's-complement OR；结果符号 = a_lead OR b_lead (任一负则负).

### BigInt Xor(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:578`

位异或 (XOR). 两个操作数 two's-complement XOR；结果符号 = a_lead XOR b_lead (恰好一个负才负).

### BigInt BitNot()

`src/libraries/z42.numerics/src/BigInt.z42:604`

位非 (NOT). 用代数恒等式 `~x == -(x + 1)`。 命名为 `BitNot` 而非 `Not` 以避开 z42 method resolution 的 name-only dispatch 隐患（与未来潜在的 boolean `Not()` 同名风险）。

### BigInt ShiftLeft(int n)

`src/libraries/z42.numerics/src/BigInt.z42:705`

左移 n 位 (n >= 0)。Magnitude 左移；符号保留 (-8 << 1 == -16). n 为负数抛 ArgumentException.

### BigInt ShiftRight(int n)

`src/libraries/z42.numerics/src/BigInt.z42:719`

右移 n 位 (n >= 0)。Magnitude 逻辑右移；符号保留 (-8 >> 1 == -4). 是 **逻辑** magnitude shift，**非** 算术 shift (.NET BigInteger 用算术 shift on 负数会向 -∞ 取整；本 v0 用 sign+magnitude repr 不便实现， follow-up 处理).

### bool TestBit(int n)

`src/libraries/z42.numerics/src/BigInt.z42:734`

测试第 `n` 位（`n >= 0`）是否为 1；`n` 为负抛 `ArgumentException`。

负数按无限精度 two's-complement 解释（同 Python `(n >> i) & 1`）：
`(-1).TestBit(i)` 对任意 `i` 都为 true（符号扩展的 1）。

### int BitLength()

`src/libraries/z42.numerics/src/BigInt.z42:754`

表示该数值（绝对值）所需的最少 bit 数。 `Zero.BitLength() == 0`, `One.BitLength() == 1`, `255.BitLength() == 8`, `(-256).BitLength() == 9`. 永远 `>= 0`.

### BigInt ModPow(BigInt exp, BigInt modulus)

`src/libraries/z42.numerics/src/BigInt.z42:771`

Modular exponentiation `this^exp mod modulus`（负指数经 ModInverse；奇模数走 Montgomery REDC）。见 BigIntModular。

### BigInt ModInverse(BigInt modulus)

`src/libraries/z42.numerics/src/BigInt.z42:774`

Modular multiplicative inverse in `[0, modulus)`（扩展欧几里得）。见 BigIntModular。

### bool IsProbablyPrime(int rounds)

`src/libraries/z42.numerics/src/BigInt.z42:777`

Miller-Rabin 概率素性检测，用挂钟播种的默认 RNG（结果不可复现）。要可复现就用 `(int, Random)` 重载。

### bool IsProbablyPrime(int rounds, Random rng)

`src/libraries/z42.numerics/src/BigInt.z42:783`

Miller-Rabin with caller-provided RNG（错误率 ≤ 4^-rounds）。见 BigIntPrimality。

### BigInt NextPrime()

`src/libraries/z42.numerics/src/BigInt.z42:786`

Smallest (probably) prime strictly greater than `this`（6k±1 轮转 + 小素数试除 + 20 轮 MR）。见 BigIntPrimality。

### bool IsPrime()

`src/libraries/z42.numerics/src/BigInt.z42:789`

Deterministic primality for n < 3,317,044,064,679,887,385,961,981（OEIS A014233 见证集）。见 BigIntPrimality。

### bool IsBpswPrime()

`src/libraries/z42.numerics/src/BigInt.z42:792`

BPSW primality（强 MR 基 2 + 强 Lucas）。见 BigIntBpsw。

### BigInt Gcd(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:810`

`|this|` 与 `|other|` 的最大公约数，结果非负（`gcd(0,0)==0`，`gcd(a,0)==|a|`）。

用 Stein 二进制 GCD：每步是移位 + 比较 + 减法，而非完整 divmod，对加密规模
操作数约快 3-5×（BigInt.Divide 是 O(n*m)，ShiftRight 是 O(n)，Subtract 是 O(max(n,m))）。

迭代形式（无递归）：
  1. 提出公共因子 2：两者同偶时 `a, b ← a/2, b/2`，记移位次数 k，最后乘回 2^k。
  2. 剥掉 a 剩余因子 2 直到 a 为奇。
  3. 主循环：b 剥 2 到奇，保证 `a ≤ b`，令 `b ← b - a`（奇-奇=偶，下一轮至少再剥一个 2），直到 `b == 0`。
  4. 返回 `a << k`。

### BigInt Lcm(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:848`

`|this|` 与 `|other|` 的最小公倍数，结果非负；`lcm(0, _) == 0`。

### int CompareTo(BigInt other)

`src/libraries/z42.numerics/src/BigInt.z42:860`

与另一个 BigInt 比较，返回 -1 / 0 / +1。

## Complex

`src/libraries/z42.numerics/src/Complex.z42:5`

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

### double Real

`src/libraries/z42.numerics/src/Complex.z42:7`

实部（公开字段）。

### double Imaginary

`src/libraries/z42.numerics/src/Complex.z42:9`

虚部（公开字段）。

### Complex(double real, double imaginary)

`src/libraries/z42.numerics/src/Complex.z42:12`

用实部与虚部构造复数。

### static Complex Zero()

`src/libraries/z42.numerics/src/Complex.z42:22`

零（0 + 0i）。

### static Complex One()

`src/libraries/z42.numerics/src/Complex.z42:27`

一（1 + 0i）。

### static Complex ImaginaryOne()

`src/libraries/z42.numerics/src/Complex.z42:32`

虚数单位（0 + 1i）。

### static Complex FromPolar(double magnitude, double phase)

`src/libraries/z42.numerics/src/Complex.z42:39`

由极坐标（模、相位弧度）构造；`magnitude < 0` 会被接受（相位旋转 π）。

### double Magnitude()

`src/libraries/z42.numerics/src/Complex.z42:45`

模 `|z| = sqrt(real² + imag²)`。

### double Phase()

`src/libraries/z42.numerics/src/Complex.z42:50`

辐角（弧度），范围 (-π, π]，即 atan2(imag, real)。

### bool IsZero()

`src/libraries/z42.numerics/src/Complex.z42:57`

是否为零。

### bool IsReal()

`src/libraries/z42.numerics/src/Complex.z42:62`

是否为实数（虚部为 0）。

### bool IsImaginary()

`src/libraries/z42.numerics/src/Complex.z42:67`

是否为纯虚数（实部为 0 且虚部非 0）。

### bool Equals(Complex other)

`src/libraries/z42.numerics/src/Complex.z42:72`

精确的 `double` 相等比较。

### Complex Add(Complex other)

`src/libraries/z42.numerics/src/Complex.z42:79`

复数加法。

### Complex Subtract(Complex other)

`src/libraries/z42.numerics/src/Complex.z42:85`

复数减法。

### Complex Multiply(Complex other)

`src/libraries/z42.numerics/src/Complex.z42:91`

复数乘法：`(a + bi)(c + di) = (ac - bd) + (ad + bc)i`。

### Complex MultiplyReal(double s)

`src/libraries/z42.numerics/src/Complex.z42:100`

乘以实数标量（避免构造 `Complex(s, 0)`）。

### Complex Divide(Complex other)

`src/libraries/z42.numerics/src/Complex.z42:106`

复数除法。除以零不抛异常，按 IEEE 浮点规则得 (NaN, NaN)。 用 Smith 算法取 |c|、|d| 较大者缩放，避免中间溢出。

### Complex Negate()

`src/libraries/z42.numerics/src/Complex.z42:130`

相反数：`-(a + bi) = -a - bi`。

### Complex Conjugate()

`src/libraries/z42.numerics/src/Complex.z42:135`

共轭：`(a + bi)* = a - bi`。

### Complex Reciprocal()

`src/libraries/z42.numerics/src/Complex.z42:140`

倒数：`1 / z = z* / |z|²`（`0` 的倒数返回 `Zero()`）。

### static Complex Exp(Complex z)

`src/libraries/z42.numerics/src/Complex.z42:152`

指数：`e^z = e^Real * (cos(Imag) + i*sin(Imag))`。

### static Complex Log(Complex z)

`src/libraries/z42.numerics/src/Complex.z42:159`

自然对数主支：`ln(z) = ln|z| + i*arg(z)`（`ln(0)` 按 IEEE 返回 NaN）。

### static Complex Sqrt(Complex z)

`src/libraries/z42.numerics/src/Complex.z42:164`

主平方根：`sqrt(r·e^(iθ)) = sqrt(r) · e^(iθ/2)`。

### static Complex Sin(Complex z)

`src/libraries/z42.numerics/src/Complex.z42:174`

正弦：`sin(a + bi) = sin(a)·cosh(b) + i·cos(a)·sinh(b)`。

### static Complex Cos(Complex z)

`src/libraries/z42.numerics/src/Complex.z42:182`

余弦：`cos(a + bi) = cos(a)·cosh(b) - i·sin(a)·sinh(b)`。

### static Complex Pow(Complex z, Complex w)

`src/libraries/z42.numerics/src/Complex.z42:190`

幂：`z^w = exp(w · ln(z))`（`0^0` → 1）。

### override string ToString()

`src/libraries/z42.numerics/src/Complex.z42:201`

字符串形式 `"(real, imag)"`。

## Decimal

`src/libraries/z42.numerics/src/Decimal.z42:6`

任意精度十进制定点：`BigInt` 尾数 + 非负十进制 `scale`，表示 `mantissa * 10^-scale`。 没有精度上限（乘法得 2N 位结果，除法显式指定结果位数）。按不可变使用，每个运算返回新对象。

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

### BigInt _mantissa

`src/libraries/z42.numerics/src/Decimal.z42:9`


### int _scale

`src/libraries/z42.numerics/src/Decimal.z42:10`


### Decimal(BigInt mantissa, int scale)

`src/libraries/z42.numerics/src/Decimal.z42:13`

用尾数与 scale 构造定点数（`scale >= 0`）。

### Decimal(int value)

`src/libraries/z42.numerics/src/Decimal.z42:22`

用整数构造定点数（scale 为 0）。

### static Decimal Zero()

`src/libraries/z42.numerics/src/Decimal.z42:30`

零值定点数（0，scale 0）。

### static Decimal One()

`src/libraries/z42.numerics/src/Decimal.z42:35`

一值定点数（1，scale 0）。

### static Decimal FromInt(int value)

`src/libraries/z42.numerics/src/Decimal.z42:40`

从 `int` 构造定点数。

### static Decimal FromLong(long value)

`src/libraries/z42.numerics/src/Decimal.z42:45`

从 `long` 构造定点数。

### static Decimal Parse(string s)

`src/libraries/z42.numerics/src/Decimal.z42:50`

解析十进制字面量 `[-]?digit+([.]digit+)?`（不支持指数/千分位）。非法输入抛 `FormatException`。

### bool IsZero()

`src/libraries/z42.numerics/src/Decimal.z42:101`

是否为零。

### bool IsNegative()

`src/libraries/z42.numerics/src/Decimal.z42:106`

是否为负。

### int Sign()

`src/libraries/z42.numerics/src/Decimal.z42:111`

符号：-1 / 0 / +1。

### BigInt Mantissa()

`src/libraries/z42.numerics/src/Decimal.z42:116`

返回尾数（`BigInt`）。

### int Scale()

`src/libraries/z42.numerics/src/Decimal.z42:121`

返回 scale（小数位数）。

### Decimal Add(Decimal other)

`src/libraries/z42.numerics/src/Decimal.z42:150`

加法（结果 scale 取两边较大者）。

### Decimal Subtract(Decimal other)

`src/libraries/z42.numerics/src/Decimal.z42:158`

减法（结果 scale 取两边较大者）。

### Decimal Multiply(Decimal other)

`src/libraries/z42.numerics/src/Decimal.z42:166`

乘法（结果 scale 为两边之和）。

### Decimal DivideBy(Decimal other, int resultScale)

`src/libraries/z42.numerics/src/Decimal.z42:177`

除法，显式指定结果位数（`resultScale` 位小数）。向零截断，无舍入模式。

### Decimal Negate()

`src/libraries/z42.numerics/src/Decimal.z42:207`

取相反数。

### Decimal Abs()

`src/libraries/z42.numerics/src/Decimal.z42:212`

取绝对值。

### int CompareTo(Decimal other)

`src/libraries/z42.numerics/src/Decimal.z42:219`

比较（跨 scale 对齐后比较，返回 -1 / 0 / +1）。

### bool Equals(Decimal other)

`src/libraries/z42.numerics/src/Decimal.z42:227`

按数值比较，跨 scale 相等（`Parse("19.99").Equals(Parse("19.990")) == true`）。

### override string ToString()

`src/libraries/z42.numerics/src/Decimal.z42:234`

十进制字符串，保留存储的 scale（`Parse("1.00").ToString() == "1.00"`），负数带 `-`。

