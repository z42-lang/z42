# z42.numerics —— 任意精度整数 / 十进制定点 / 复数

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.numerics/`；命名空间 `Std.Numerics`

三个互不依赖的数值类型，都是引用类型，按**不可变**使用——每个运算返回新对象，从不改动接收者：

| 类型 | 用途 |
|---|---|
| [`BigInt`](#bigint) | 任意精度整数。溢出 `long` 的算术、模幂 / 模逆 / GCD / 素性检测 |
| [`Decimal`](#decimal) | 任意精度十进制定点（`BigInt` 尾数 + 十进制 scale）。金额等不能用二进制浮点的场合 |
| [`Complex`](#complex) | `double` 实部 + 虚部的复数 |

没有运算符重载——一律走方法调用（`a.Add(b)` 而不是 `a + b`）。
也没有 `Vector<T>` / SIMD 类型。

`BigInt` 的素性检测依赖 `z42.random`，包清单里会一并拉入。

## BigInt

```z42
// 常量
public static BigInt Zero;
public static BigInt One;
public static BigInt MinusOne;

// 构造
public BigInt(long value)                       // int 实参走 int→long 隐式加宽
public static BigInt Parse(string s)            // 十进制，允许前导 "+" / "-"
public static BigInt ParseHex(string s)         // 十六进制，允许 "0x" 前缀与 "-"

// 检视
public bool IsZero()
public bool IsOne()
public bool IsNegative()
public int  Sign()                              // -1 / 0 / +1

// 转换
public int    ToInt32()
public long   ToInt64()
override string ToString()                      // 十进制
public string ToHex()                           // 小写十六进制，无 "0x"，负数带 "-"
public string ToBase(int radix)                 // radix ∈ [2, 36]，小写字母

// 算术
public BigInt Add(BigInt other)
public BigInt Subtract(BigInt other)
public BigInt Multiply(BigInt other)
public BigInt Divide(BigInt other)              // 向零截断
public BigInt Mod(BigInt other)                 // 余数符号跟被除数
public BigInt Negate()
public BigInt Abs()
public BigInt Pow(int exponent)                 // exponent >= 0

// 位运算
public BigInt And(BigInt other)
public BigInt Or(BigInt other)
public BigInt Xor(BigInt other)
public BigInt BitNot()
public BigInt ShiftLeft(int n)
public BigInt ShiftRight(int n)
public bool   TestBit(int n)
public int    BitLength()

// 模运算与数论
public BigInt ModPow(BigInt exp, BigInt modulus)
public BigInt ModInverse(BigInt modulus)
public BigInt Gcd(BigInt other)
public BigInt Lcm(BigInt other)

// 素性
public bool   IsPrime()
public bool   IsBpswPrime()
public bool   IsProbablyPrime(int rounds)
public bool   IsProbablyPrime(int rounds, Random rng)   // Std.Random.Random
public BigInt NextPrime()

// 比较
public int CompareTo(BigInt other)
override bool Equals(object other)
override int  GetHashCode()
```

### 语义要点

| 成员 | 行为 |
|---|---|
| `Divide` / `Mod` | 向零截断，余数与被除数同号：`(-7)/3 == -2`，`(-7)%3 == -1` |
| `ShiftLeft` / `ShiftRight` | **幅值移位**并保留符号，不是算术移位：`(-8).ShiftRight(1) == -4`（不是 `-4` 向 -∞ 取整那种语义）。要 floor 语义请用 `Divide(2^k)` |
| `And` / `Or` / `Xor` / `TestBit` | 负数按**无限精度补码**解释（同 Python）：`(-1).And(7) == 7`，`(-2).Or(3) == -1`，`(-1).TestBit(100) == true` |
| `BitNot` | `~x == -(x+1)` |
| `BitLength` | 幅值位数，与符号无关：`(-256).BitLength() == 9` |
| `Gcd` | 取绝对值，结果非负；`gcd(0, 0) == 0`，`gcd(a, 0) == abs(a)` |
| `Lcm` | 非负；`lcm(0, _) == 0` |
| `ModPow` | `exp == 0` → `1`；`modulus == 1` → `0`；`exp < 0` 自动走 `ModInverse`（要求 `gcd(this, modulus) == 1`） |
| `ModInverse` | 返回 `[0, modulus)` 内的解；负的 `this` 先归一化 |
| `Equals` | 类型不符（如 `Equals("x")`）返回 `false`，不抛 |

### 三个素性判定的取舍

| 方法 | 适用范围 | 误判率 |
|---|---|---|
| `IsPrime()` | `n < 3 317 044 064 679 887 385 961 981`，超界抛 `ArgumentException` | 0（确定性见证集） |
| `IsBpswPrime()` | 任意 `n` | 无已知反例 |
| `IsProbablyPrime(rounds)` | 任意 `n` | ≤ 4^-rounds |

`IsProbablyPrime(int)` 内部用挂钟播种的 `Std.Random.Random`——**结果不可复现**；
要可复现就用 `IsProbablyPrime(int, Random)` 传自己种好的 RNG。
`NextPrime()` 返回严格大于当前值的最小**概率**素数（内部 20 轮 Miller-Rabin）；
`this < 2` 时返回 `2`。

### 异常

| 场景 | 异常 |
|---|---|
| `Parse` / `ParseHex` 空串、只有符号、非法字符 | `FormatException` |
| `ToInt32` / `ToInt64` 超出目标范围 | `OverflowException` |
| `Divide` / `Mod` 除数为 0 | `DivideByZeroException` |
| `Pow` 负指数 | `ArgumentException` |
| `ToBase` radix 不在 `[2, 36]` | `ArgumentException` |
| `ShiftLeft` / `ShiftRight` / `TestBit` 负参数 | `ArgumentException` |
| `ModPow` modulus ≤ 0 | `ArgumentException` |
| `ModInverse` modulus ≤ 1 或 `gcd != 1` | `ArgumentException` |
| `IsProbablyPrime` rounds ≤ 0 | `ArgumentException` |
| `IsPrime` 超出确定性上界 | `ArgumentException` |

> `ToInt32` 的越界消息里写的是 `BigInt.ToInt64`（它转发给 `ToInt64`），异常类型仍是
> `OverflowException`。

## Decimal

任意精度的十进制定点数：一个 `BigInt` 尾数配一个非负的十进制 `scale`（小数位数）。

```z42
public Decimal(BigInt mantissa, int scale)     // scale >= 0
public Decimal(int value)
public static Decimal Zero()
public static Decimal One()
public static Decimal FromInt(int value)
public static Decimal FromLong(long value)
public static Decimal Parse(string s)

public bool   IsZero()
public bool   IsNegative()
public int    Sign()
public BigInt Mantissa()
public int    Scale()

public Decimal Add(Decimal other)
public Decimal Subtract(Decimal other)
public Decimal Multiply(Decimal other)
public Decimal DivideBy(Decimal other, int resultScale)   // 向零截断到 resultScale 位
public Decimal Negate()
public Decimal Abs()

public int  CompareTo(Decimal other)
public bool Equals(Decimal other)
override string ToString()
```

- **`ToString` 保留存储的 scale**：`Parse("1.00").ToString() == "1.00"`，不会规范化成 `"1"`。
- **`Equals` 按数值比较，跨 scale 相等**：`Parse("19.99").Equals(Parse("19.990")) == true`。
- `Add` / `Subtract` 取两边 scale 的较大者，`Multiply` 的 scale 是两边之和
  （`19.99 × 0.08 == 1.5992`，4 位）。
- **除法必须显式给结果位数**——没有无参 `Divide`，也没有舍入模式可选，一律向零截断：
  `Decimal.One().DivideBy(Decimal.FromInt(3), 4).ToString() == "0.3333"`。
- 异常：`Parse` 空串 / 多个 `.` / 非数字字符 → `FormatException`；
  构造 scale 为负、`DivideBy` 除数为 0 或 `resultScale` 为负 → `ArgumentException`。

## Complex

`double` 实部 + 虚部。所有运算都返回新对象。

```z42
public double Real;         // 公开字段
public double Imaginary;

public Complex(double real, double imaginary)
public static Complex Zero()
public static Complex One()
public static Complex ImaginaryOne()
public static Complex FromPolar(double magnitude, double phase)

public double Magnitude()
public double Phase()
public bool   IsZero()
public bool   IsReal()
public bool   IsImaginary()
public bool   Equals(Complex other)

public Complex Add(Complex other)
public Complex Subtract(Complex other)
public Complex Multiply(Complex other)
public Complex MultiplyReal(double s)
public Complex Divide(Complex other)
public Complex Negate()
public Complex Conjugate()
public Complex Reciprocal()

public static Complex Exp(Complex z)
public static Complex Log(Complex z)
public static Complex Sqrt(Complex z)
public static Complex Sin(Complex z)
public static Complex Cos(Complex z)
public static Complex Pow(Complex z, Complex w)

public override string ToString()      // "(real, imag)"
```

- `Zero` / `One` / `ImaginaryOne` 是**方法**不是字段，要写 `Complex.Zero()`。
- **除以零不抛异常**，按 IEEE 浮点规则得到 `(NaN, NaN)`。
- 结果带浮点误差：`Sqrt(-1)` 得到 `(6.12e-17, 1)` 而非精确的 `(0, 1)`；
  比较请自行设容差，`Equals` 是精确的 `double` 相等。
- 没有 `Tan` / `Asin` / `Atan` 等其余三角与反三角函数。

## 用法

```z42
using Std.IO;
using Std.Numerics;

void Main() {
    // 大整数：30!
    var f = BigInt.One;
    int i = 1;
    while (i <= 30) { f = f.Multiply(new BigInt(i)); i = i + 1; }
    Console.WriteLine($"30! = {f.ToString()}");        // 265252859812191058636308480000000
    Console.WriteLine($"位数 = {f.BitLength()}");       // 108

    // 玩具 RSA：e=7, n=143, d = 7^-1 mod 120
    var m = new BigInt(42);
    var e = new BigInt(7);
    var n = new BigInt(143);
    var c = m.ModPow(e, n);
    var d = e.ModInverse(new BigInt(120));
    Console.WriteLine($"enc={c.ToString()} dec={c.ModPow(d, n).ToString()}");   // 81 / 42

    // 素数
    var p = BigInt.Parse("1000000007");
    Console.WriteLine($"isPrime={p.IsPrime()} next={p.NextPrime().ToString()}");

    // 金额：不丢精度
    var price = Decimal.Parse("19.99");
    var tax   = price.Multiply(Decimal.Parse("0.08"));
    Console.WriteLine($"total={price.Add(tax).ToString()}");   // 21.5892

    // 复数
    var z = new Complex(3.0, 4.0);
    Console.WriteLine($"|z|={z.Magnitude()} conj={z.Conjugate().ToString()}");
}
```

## 不支持

- **运算符重载**：`a + b` / `a * b` / `a == b` 对这三个类型都不成立，一律用方法
- **`Vector<T>` / SIMD**：本包没有向量类型
- **`BigInt` 的算术右移**：`ShiftRight` 是幅值移位，没有向 -∞ 取整的变体
- **`Decimal` 的舍入模式**：`DivideBy` 只有向零截断，没有 banker's rounding 之类的选项；
  也没有不指定结果位数的除法
- **`Decimal` 的科学计数法解析**：`Parse("1e5")` 不接受
- **`Complex` 的其余超越函数**：`Tan` / 反三角 / 双曲函数
- **格式化**：没有 `ToString(format)` / 千分位 / 定宽补零；`BigInt` 换进制只有
  `ToBase(radix)`，`Decimal` / `Complex` 只有固定形态的 `ToString()`

`BigInt._mag` / `BigInt._sign` / `Decimal._mantissa` / `Decimal._scale` /
`BigInt.LIMB_BITS` / `BigInt.BASE` / `_MagDivModResult` 虽然是 `public`，但属内部表示，
不构成稳定 API，不要直接读写。
