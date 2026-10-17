<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.random/ -->

# z42.random —— 确定性伪随机数：给同一个 seed 得到同一串数。

> 包路径 `src/libraries/z42.random/`；命名空间 `Std.Random`

适合测试 fixture、模拟、洗牌、临时 ID、UI 抖动。

## 不支持

- 没有从 OS 熵源取 seed 的入口：`Random()` 只能拿墙钟毫秒，可被猜测。需要强 seed 请直接用
  [`Std.Crypto.SecureRandom`]，不要给 `Random` 喂熵。
- 没有 `SampleLong`：`Shuffle` 有 `Int` / `Long` / `String` 三个版本，`Sample` 只有 `Int` / `String`。
- 没有泛型 / 对象数组版本：`Shuffle` 与 `Sample` 只接受这三种元素类型的数组，`T[]` / `object[]` 不支持。
- 没有状态的保存与恢复：内部状态不可读写，无法在两次运行之间续跑同一条序列（只能记住 seed 加抽取次数重放）。
- 没有 `NextBytes(byte[])`：要随机字节请自己从 `NextInt()` 拆，或用 [`Std.Crypto.SecureRandom.GetBytes`]。
- 分布只有正态与指数：泊松 / 二项 / 伽马等未提供。
- 不做线程同步：并发调用同一实例会撕裂内部状态，结果无定义。

> ⚠️ **不是 CSPRNG。** 输出可预测——观察到少量输出即可反推内部状态。session token、CSRF nonce、 密钥、盐、密码等任何「不可预测性」有安全含义的场景，一律用 [`Std.Crypto.SecureRandom`]。

> ⚠️ **非线程安全。** 一个 `Random` 实例不能被多个线程共用。多线程请每线程一个实例，并用不同的 `streamId` 保证序列互不重叠。

## Random

`src/libraries/z42.random/src/Random.z42:24`

PCG-XSH-RR 伪随机数发生器。

```z42
public class Random {
    Random()
    Random(long seed)
    Random(long seed, long streamId)
    long NextLong()
    int NextInt()
    long NextLongRange(long min, long max)
    int NextIntRange(int min, int max)
    double NextDouble()
    bool NextBool()
    void ShuffleInt(int[] arr)
    void ShuffleLong(long[] arr)
    void ShuffleString(string[] arr)
    int[] SampleInt(int[] arr, int k)
    string[] SampleString(string[] arr, int k)
    double NextGaussian(double mean, double stddev)
    double NextExponential(double lambda)
}
```

### Random()

`src/libraries/z42.random/src/Random.z42:32`

用墙钟时间作 seed。每次运行结果不同，**不可复现**。

### Random(long seed)

`src/libraries/z42.random/src/Random.z42:39`

固定 seed：相同 seed ⇒ 逐位相同的序列。

### Random(long seed, long streamId)

`src/libraries/z42.random/src/Random.z42:47`

固定 seed 并选一条独立的流。

seed 相同、`streamId` 不同的两个实例，产生的序列互不重叠。

| 参数 | 说明 |
|---|---|
| `streamId` | 内部取 `streamId | 1` 作为 PCG 增量 |

### long NextLong()

`src/libraries/z42.random/src/Random.z42:85`

返回整个 i64 范围（含负数）的随机数。

### int NextInt()

`src/libraries/z42.random/src/Random.z42:92`

返回整个 i32 范围（含负数）的随机数。

### long NextLongRange(long min, long max)

`src/libraries/z42.random/src/Random.z42:107`

返回 `[min, max)` 内的整数。

用取模映射，不做 rejection sampling：span 不整除 2⁶³ 时存在微小偏差。

抛 `ArgumentException`：`max <= min`

### int NextIntRange(int min, int max)

`src/libraries/z42.random/src/Random.z42:125`

返回 `[min, max)` 内的整数。

用取模映射，不做 rejection sampling：span 不整除 2³² 时存在微小偏差。

抛 `ArgumentException`：`max <= min`

```z42
new Random(42).NextIntRange(5, 6)    // => 5
```

### double NextDouble()

`src/libraries/z42.random/src/Random.z42:136`

返回 `[0.0, 1.0)` 内的随机数，取 53 位尾数精度。

### bool NextBool()

`src/libraries/z42.random/src/Random.z42:147`

各 1/2 概率返回 true。

### void ShuffleInt(int[] arr)

`src/libraries/z42.random/src/Random.z42:155`

原地 Fisher–Yates 洗牌。长度 0 / 1 是 no-op。

### void ShuffleLong(long[] arr)

`src/libraries/z42.random/src/Random.z42:168`

原地 Fisher–Yates 洗牌。长度 0 / 1 是 no-op。

### void ShuffleString(string[] arr)

`src/libraries/z42.random/src/Random.z42:181`

原地 Fisher–Yates 洗牌。长度 0 / 1 是 no-op。

### int[] SampleInt(int[] arr, int k)

`src/libraries/z42.random/src/Random.z42:195`

不放回抽 `k` 个，不改动 `arr`，返回新数组。结果本身已是随机顺序。

抛 `ArgumentException`：`k < 0` 或 `k > arr.Length`

### string[] SampleString(string[] arr, int k)

`src/libraries/z42.random/src/Random.z42:221`

不放回抽 `k` 个，不改动 `arr`，返回新数组。结果本身已是随机顺序。

抛 `ArgumentException`：`k < 0` 或 `k > arr.Length`

### double NextGaussian(double mean, double stddev)

`src/libraries/z42.random/src/Random.z42:243`

正态分布采样（Box–Muller）。一对均匀数只产出一个正态数。

### double NextExponential(double lambda)

`src/libraries/z42.random/src/Random.z42:255`

指数分布采样，`lambda` 是速率参数（均值 = `1 / lambda`）。

抛 `ArgumentException`：`lambda <= 0`

