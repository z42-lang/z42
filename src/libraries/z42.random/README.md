# z42.random

## 职责
Deterministic 伪随机数生成器（PCG-XSH-RR 64→32 输出）。Seed 后输出可重现，适合
测试 fixture、UI 抖动、shuffling、生成 ID 等。

**不是 CSPRNG**：安全场景用 `z42.crypto` 的 `SecureRandom`。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Random.z42` | `Std.Random.Random` 类，PCG-XSH-RR 实现 + range / double / bool helpers + `ShuffleInt/Long/String` / `SampleInt/String` 集合操作 + `NextGaussian` / `NextExponential` 分布 |

## 入口点
- `Std.Random.Random()` — 用 wall-clock seed
- `Std.Random.Random(long seed)` — 显式 seed（同 seed → 同序列）
- `Std.Random.Random(long seed, long streamId)` — 同 seed 下选独立 PCG 流
- `NextInt()` — i32 全范围
- `NextLong()` — i64 全范围
- `NextIntRange(int min, int max)` — `[min, max)`，max exclusive
- `NextLongRange(long min, long max)` — `[min, max)`
- `NextDouble()` — `[0.0, 1.0)`，53-bit 精度
- `NextBool()` — 公平硬币
- `NextGaussian(mean, stddev)` / `NextExponential(lambda)` — 正态 / 指数分布

## 用法

```z42
using Std.Random;
using Std.IO;

var r = new Random(42);              // 显式 seed
Console.WriteLine(r.NextInt());      // -1024113624 (deterministic)
Console.WriteLine(r.NextDouble());   // [0, 1)
Console.WriteLine(r.NextIntRange(1, 7));  // 模拟骰子

var live = new Random();             // wall-clock seed → 每次启动不同
```

## 依赖关系
仅依赖 `z42.core`（基础类型 + `DateTime.UtcNow()` 提供 wall-clock seed）。不依赖 `z42.crypto`（`crypto → numerics → random` 会成环）。

## 算法

PCG (Permuted Congruential Generator) — Melissa O'Neill, 2014。64-bit state，
LCG 步进 + xorshift + rotate 输出。passes PractRand 32TB。详 https://www.pcg-random.org/。

参数（来自原论文）：
- multiplier: `6364136223846793005`
- increment:  `1442695040888963407`
- output:     `rotr32(((state >> 18) ^ state) >> 27, state >> 59)`

## 如何测试验证

```bash
xtask test stdlib z42.random    # 本库全部 [Test]
```
