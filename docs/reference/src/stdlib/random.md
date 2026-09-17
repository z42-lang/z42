# z42.random —— 确定性伪随机数

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.random/`；命名空间 `Std.Random`

可复现的伪随机数发生器：给同一个 seed 就得到同一串数。适合测试 fixture、模拟、洗牌、
临时 ID、UI 抖动。

> ⚠️ **不是 CSPRNG。** 输出可预测——观察到少量输出即可反推内部状态。session token、
> CSRF nonce、密钥、盐、密码等任何「不可预测性」有安全含义的场景，一律用
> `Std.Crypto.SecureRandom`（见 [crypto](crypto.md)）。
>
> ⚠️ **非线程安全。** 一个 `Random` 实例不能被多个线程共用。多线程场景请每线程一个实例，
> 并用不同的 `streamId` 保证序列互不重叠。

## Random

```z42
namespace Std.Random;

public class Random {
    public Random()                              // 墙钟时间作 seed
    public Random(long seed)
    public Random(long seed, long streamId)

    // 原始输出
    public long   NextLong()
    public int    NextInt()
    public double NextDouble()
    public bool   NextBool()

    // 区间
    public long NextLongRange(long min, long max)
    public int  NextIntRange(int min, int max)

    // 集合
    public void ShuffleInt(int[] arr)
    public void ShuffleLong(long[] arr)
    public void ShuffleString(string[] arr)
    public int[]    SampleInt(int[] arr, int k)
    public string[] SampleString(string[] arr, int k)

    // 分布
    public double NextGaussian(double mean, double stddev)
    public double NextExponential(double lambda)
}
```

### 构造

| 构造器 | 说明 |
|---|---|
| `Random()` | seed 取 `DateTime.UtcNow().UnixMs()`。每次运行结果不同，**不可复现** |
| `Random(long seed)` | 固定 seed。相同 seed ⇒ 逐位相同的序列 |
| `Random(long seed, long streamId)` | 额外选一条独立的流。`streamId` 内部按 `| 1` 取奇数作为 PCG 增量；相同 seed、不同 `streamId` 的两个实例产生互不重叠的序列 |

`Random(seed)` 等价于某个固定 `streamId` 下的 `Random(seed, streamId)`——已有的 seeded
代码序列保持不变。

### 输出

| 成员 | 值域 / 说明 |
|---|---|
| `NextLong()` | 整个 i64 范围（含负数） |
| `NextInt()` | 整个 i32 范围（含负数） |
| `NextDouble()` | `[0.0, 1.0)`，取 53 位尾数精度 |
| `NextBool()` | 各 1/2 概率 |
| `NextLongRange(min, max)` | `[min, max)`。`max <= min` 抛 `ArgumentException` |
| `NextIntRange(min, max)` | `[min, max)`。`max <= min` 抛 `ArgumentException` |

`NextLongRange` / `NextIntRange` 用取模映射，**不做 rejection sampling**：当 span 不整除
生成器周期时存在极小的模偏差。对 span 远小于 2³² 的常规用法可以忽略；需要严格均匀请自行
做 rejection。

### 集合操作

| 成员 | 说明 |
|---|---|
| `ShuffleInt` / `ShuffleLong` / `ShuffleString` | **原地** Fisher–Yates 洗牌，返回 `void`，直接改传入数组。长度 0 / 1 是 no-op |
| `SampleInt(arr, k)` / `SampleString(arr, k)` | 不放回抽 `k` 个，**不改动 `arr`**，返回新数组。结果本身已是随机顺序 |

`SampleInt` / `SampleString` 在 `k < 0` 或 `k > arr.Length` 时抛 `ArgumentException`；
`k == 0` 返回空数组，`k == arr.Length` 返回整个数组的一个随机排列。

### 分布

| 成员 | 说明 |
|---|---|
| `NextGaussian(mean, stddev)` | 正态分布采样（Box–Muller）。一对均匀数只产出一个正态数 |
| `NextExponential(lambda)` | 指数分布采样，`lambda` 是速率参数（均值 = `1 / lambda`）。`lambda <= 0` 抛 `ArgumentException` |

## 用法

```z42
using Std;
using Std.IO;
using Std.Random;

void Main() {
    // 可复现：同 seed 同序列
    var a = new Random(42);
    var b = new Random(42);
    Console.WriteLine(a.NextLong() == b.NextLong());   // true

    Console.WriteLine(a.NextIntRange(0, 10));          // [0, 10)
    Console.WriteLine(a.NextDouble());                 // [0.0, 1.0)

    // 独立流：同 seed 不同 streamId ⇒ 互不重叠
    var s1 = new Random(7, 1L);
    var s2 = new Random(7, 2L);
    Console.WriteLine(s1.NextLong() != s2.NextLong()); // true

    // 洗牌是原地的
    int[] deck = [1, 2, 3, 4, 5, 6, 7, 8];
    a.ShuffleInt(deck);

    // 抽样不改原数组
    int[] hand = a.SampleInt(deck, 3);
    Console.WriteLine(hand.Length);                    // 3

    Console.WriteLine(a.NextGaussian(0.0, 1.0));
    Console.WriteLine(a.NextExponential(2.0));

    try {
        a.NextIntRange(5, 5);
    } catch (ArgumentException e) {
        Console.WriteLine(e.Message);   // NextIntRange: max must be > min
    }
}
```

## 不支持

- **没有从 OS 熵源取 seed 的入口**：`Random()` 只能拿墙钟毫秒，可被猜测。需要强 seed 请直接
  用 `Std.Crypto.SecureRandom`，不要给 `Random` 喂熵。
- **没有 `SampleLong`**：`Shuffle` 有 `Int` / `Long` / `String` 三个版本，`Sample` 只有
  `Int` / `String`。`long[]` 的抽样需要自己写。
- **没有泛型 / 对象数组版本**：`Shuffle` 与 `Sample` 只接受这三种元素类型的数组，
  `T[]` / `object[]` 不支持。
- **没有状态的保存与恢复**：内部状态不可读写，无法在两次运行之间续跑同一条序列
  （只能记住 seed 加抽取次数重放）。
- **没有 `NextBytes(byte[])`**：要随机字节请自己从 `NextInt()` 拆，或用
  `Std.Crypto.SecureRandom.GetBytes`。
- **分布只有正态与指数**：泊松 / 二项 / 伽马等未提供。
- **不做线程同步**：并发调用同一实例会撕裂内部状态，结果无定义。
