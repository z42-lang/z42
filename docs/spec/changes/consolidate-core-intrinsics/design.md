# Design: 去重 cross-cutting intrinsic 到 z42.core（A1）

## Architecture

```
                 z42.core（唯一 extern 声明点）
   ┌─────────────────────────┬───────────────────────────┐
   │ Std.BitConverter        │ Std.Runtime.Clock          │
   │  [Native __single_to_bits] SingleToBits(double)->int │
   │  [Native __single_from_bits] SingleFromBits(int)->double
   │  [Native __double_to_bits] DoubleToBits(double)->long │
   │  [Native __double_from_bits] DoubleFromBits(long)->double
   │                         │  [Native __time_now_ms] WallMillis()->long
   │                         │  [Native __time_now_mono_ns] MonoNanos()->long
   └───────────▲─────────────┴──────────────▲─────────────┘
   位转换调用方  │                    时钟调用方 │
   z42.io.binary (BinaryWriter/Reader — 直接调)
   z42.ir       (ZbcInstr.DoubleToBits/ZbcReaderInstr.BitsToDouble
                 —— 保公开签名，body 委托 core；z42c 源仍调它们)
   z42.time     (DateTime→WallMillis, Stopwatch→MonoNanos)
   z42.io       (Environment.GetCurrentTimeMs 保签名，委托 WallMillis)
   z42.net      (HttpClient 直接调 WallMillis)
   z42.test     (Bencher 直接调 MonoNanos)
```

不改任何依赖 toml——所有调用方本已 `dependencies { z42.core }`。

## Decisions

### Decision 1: 位转换放独立 `Std.BitConverter`，不折进 `Convert`
**选项：** A 折进 `Std.Convert`（已有 `__*_parse`/`__to_str`）；B 独立 `Std.BitConverter`。
**决定：** B。`Convert` 语义是 string↔numeric；位重解释（IEEE754 reinterpret）是不同概念，独立类更诚实、
可发现（对齐 BCL `BitConverter`）。新增一个 public 类的表面成本可接受——4 个方法、职责单一。

### Decision 2: 时钟放 `Std.Runtime.Clock`，暴露最小 2 方法
**决定：** 新增 `Std.Runtime.Clock`（与既有 `Std.Runtime.Runtime` 同命名空间，低层原语域），只暴露
`WallMillis()`（unix 毫秒，= `__time_now_ms`）与 `MonoNanos()`（单调纳秒，= `__time_now_mono_ns`）。
不加任何便利方法（接口最小化）——DateTime/TimeSpan/Stopwatch 等语义封装仍留 `z42.time`。

### Decision 3: bootstrap —— `_ensureBootstrapZ42Ir` 先建 core（位转换的唯一真实风险）
**问题：** `z42.ir` 是 z42c 的**运行期自依赖**（zbc/zpkg 后端）。冷/首暖构建时
`_ensureBootstrapZ42Ir`（`scripts/build/xtask_compiler.z42:69,92-97`）用 `_z42cBuildPackage` 把**当前源
z42.ir 单独**编进 flat libs——此刻 flat 里的 `z42.core` 还是 seed/上一次的旧 core，**缺 `BitConverter`** →
`z42.ir` 编译 `undefined function`。
**决定：** 在该函数里，重建 `z42.ir` **之前**先用同样的 `_z42cBuildPackage` 把**当前源 z42.core** 建进
flat（幂等；暖树重复建 core 开销小）。与函数已对 z42.ir 做的「先预建再自建」是同一模式
（`xtask_compiler.z42:84-91` 注释）。
**为何不是两-nightly（seed-API 轴）：** 该轴只绑 **z42c/xtask 源**新用 stdlib API；本 change **z42c 源零
改动**（保留 `ZbcInstr.DoubleToBits`/`BitsToDouble` 签名）。stdlib 源自身不受该轴约束
（bootstrap-seed.md）。故只需 (c) 预建 core，不需延迟一个 nightly。

### Decision 4: z42.ir 保 wrapper（不让 z42c 源改调 core）
**问题：** `IrGenFacts.z42:113,127,161-165` 与 `IrGen.z42:9` 用 `ZbcInstr.DoubleToBits` /
`ZbcReaderInstr.BitsToDouble` 做浮点字面量常量折叠。若删这两个方法让 z42c 改调 `Std.BitConverter`，则
**z42c 源新用 core API** → 踩 seed-API 两-nightly 轴 + `xtask test bootstrap` 红（nightly z42c 编不了当前
z42c 源）。
**决定：** 保留这两个方法的公开签名，body 从 `extern` 改为 `return Std.BitConverter.DoubleToBits(v);`。
z42c 源零改动；`test bootstrap` 用 nightly z42.ir（含旧方法）解析 → 绿。副作用：z42.ir 多两个薄 wrapper
（可接受——它本就是 zbc 编码门面）。

### Decision 5: 时钟 (a) 安全，无 bootstrap 特殊处理
`z42.ir` 不用时钟；z42c/xtask 源不用时钟 intrinsic（只用 z42.time.Stopwatch 公开 API，签名不变）。所有
时钟调用方（time/io/net/test）都在 core-first 的 workspace 全量构建里重建。故时钟部分单暖 change 即安全。

## Implementation Notes
- 跨 zpkg 静态调用比原 local extern 多一层帧（VCall）。位转换在 BinaryWriter 逐值 / zbc 逐指令；时钟调用
  低频。均非热路径瓶颈，符合升级阶梯「先脚本/最小、必要再优化」——记为可接受权衡，不在本 change 优化。
- zbc 输出必须 **byte-identical**：`DoubleToBits` 语义不变（同一 VM builtin），故浮点字面量编码不变 →
  z42c 自举 gen1==gen2 不动点应保持。
- 实施顺序：先 core 两门面 → 时钟调用方（安全）→ 位转换调用方 + `_ensureBootstrapZ42Ir` 修正 → 文档。

## Testing Strategy
- `xtask test`（完整 GREEN gate）：e2e / cross-zpkg / stdlib [Test] / compiler 自举 / vscode-syntax。
  重点看 **compiler 自举**（验证 `_ensureBootstrapZ42Ir` 修正 + z42.ir 委托无回归）与 **io.binary / time /
  test 的 [Test]**（位转换 / 时钟行为不变）。
- 自举字节不动点：z42c 自建产物应与改前 byte-identical（intrinsic 语义未变、z42c 源未变）。
- 位转换回归：z42.io.binary 既有 round-trip 测试覆盖 Single/Double LE/BE；z42.ir 的 zbc round-trip 覆盖
  float 字面量。
- 冷路径（`_ensureBootstrapZ42Ir`）本地暖树不可完全复现——依赖 CI 冷构建腿（bootstrap-seed.md：cold 判定以
  CI 为准）；本地至少验证暖树 `xtask test compiler` 绿。
