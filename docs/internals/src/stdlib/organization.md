# 包划分与依赖层级

> 对齐：2026-09-17 ｜ 代码：`src/libraries/*/z42.*.z42.toml`（依赖声明）、`src/libraries/README.md`（目录说明）
>
> 一个方法落哪一层实现、全仓 native 表面有多大 → [实现分层与 native 预算](architecture.md)。
> 接口面怎么设计（漏斗 / 正交轴 / 便利糖）→ [API 设计准则](api-guidelines.md)。

本页写 **stdlib 分几个包、谁能依赖谁、新开一个包要满足什么**。

## 1. 两类库，命名空间是唯一区分

`src/libraries/` 下同住两类用途完全不同的库：

| 类别 | 命名空间 | 面向 | 成员 |
|---|---|---|---|
| **用户 stdlib** | `Std.*` | 应用开发者 | core / collections / io / text / encoding / toml / json / yaml / uri / regex / cli / diagnostics / random / numerics / net / threading / compression / crypto / test / scripting |
| **工具链库** | `Z42.*` | 编译器 / 工具自身 | `z42.ir`（`Z42.IR` + `Z42.Project`）、`z42.project`（`Z42.Build.Project`）、`z42.build`（`Z42.Build`）、`z42c.core`（`Z42.Core`）、`z42c.syntax`（`Z42.Syntax`） |

**本页的全部规则（层级、interop 归属、R1–R4）只约束 `Std.*`。** 工具链库住在 `src/libraries/` 是因为
它们要被 z42c 运行期加载、又要被 REPL / z42b 共享，故编译成 zpkg 与 stdlib 同址分发；但它们不是用户
API，不进用户文档。新增编译器支撑库 → `Z42.*`；新增用户库 → `Std.*`。

`z42.scripting`（`Std.Scripting`）是划在 `Std.*` 这边的边界情形：它虽然「编译代码」，但编译期只依赖
stdlib（走 `z42.build` 的 `IReplCompiler` 门面，实现 `Z42cReplCompiler` 运行期反射注入），且被
playground / wasm 当作用户 API 消费。真 tty 交互层 `z42.repl` 平台绑定重，留在 `src/toolchain/`、
不入 stdlib。

> **这条边界正在被 add-package-roles 重画。** 实测（2026-09-24）：`ReplCompilerHost` 的四条组件探测
> 路径全部指向 SDK 布局，**纯 runtime 包里的 scripting 是恒失败的空壳**（其头注自陈「组件缺失 →
> `NoReplCompiler` 兜底，编译恒失败、补全恒空」）。批 0 已断掉其对 `z42.ir` 的假依赖（那条只为
> `.version` 拼一句版本串而存在，已迁 z42i）；批 1 将按「能不能在只有 runtime 的环境下工作」拆成
> eval 内核（零编译器域依赖）+ editing（依赖 `z42c.syntax`）两包。
> 见 [add-package-roles design §scripting 判定](../../../spec/changes/add-package-roles/design.md)。

## 2. 层级：只要求 DAG，不钉固定层

硬约束只有一条：**依赖图必须无环，且 `z42.core` 在所有库之下**。这是通用规则「包依赖必须无环」
（见 [`z42.toml` 参考](../../../reference/src/toolchain/z42-toml.md#依赖必须无环no-circular-dependencies)）在
stdlib 上的特化。

**「层级」是从 manifest 算出来的量，不是钉在包上的标签**——一个包的深度 = 它依赖闭包里最深那条链
再加一。源码注释与 `src/libraries/README.md` 里散落的 `L0` / `L1` / `L2` / `L3` 写法是这个量的口语
速记，不构成额外契约；判定归属永远看依赖闭包，不看别人给它贴的层号。

按各包 `z42.*.z42.toml` 实测：

| 深度 | 包 | 依赖 |
|---|---|---|
| **0** | `z42.core` | —（隐式 prelude） |
| **1** | `z42.collections` / `z42.encoding` / `z42.random` / `z42.regex` / `z42.text` | core |
| **2** | `z42.io` | core, encoding, text |
| | `z42.numerics` | core, random |
| | `z42.uri` | core, text |
| **3** | `z42.cli` | core, text, io |
| | `z42.compression` | core, io |
| | `z42.crypto` | core, encoding, numerics |
| | `z42.diagnostics` | core, io |
| | `z42.json` | core, text, io |
| | `z42.test` | core, io |
| | `z42.toml` / `z42.yaml` | core, io |
| **4** | `z42.threading` | core, diagnostics |
| **5** | `z42.net` | core, io, encoding, random, crypto, threading, compression |
| **6** | `z42.scripting` | core, io, build, test, z42c.core, z42c.syntax, threading |

几处值得记住的形状：

- **`z42.io` 不在最底层**：`StreamReader`/`StreamWriter` 收 `Std.Encoding.Encoding`、`StringWriter`
  用 `Std.Text.StringBuilder`，所以 io 压在 encoding / text 之上。
- **`z42.threading` 比 `z42.io` 高**：`Timer` 的回调异常要 `Log.Error` 吞掉，于是 threading → diagnostics → io。
- **`z42.numerics` 依赖 `z42.random`**：`BigInt.IsProbablyPrime` 的 Miller-Rabin 见证数。这条又逼得
  `z42.random` **不能**依赖 `z42.crypto`（会成环 crypto → numerics → random → crypto），
  `Random.FromEntropy()` 因此一直缺席。
- **序列化三兄弟 toml / json / yaml 都依赖 io**：因为各自都提供 `ParseStream` / `WriteTo(Stream, …)`。

新开包时**不要去找「我属于哪一层」**，而是问「我的依赖闭包是什么、有没有环」。

## 3. interop 归属：三类包

每个库属于且只属于以下之一：

| 类别 | 声明 interop？ | 成员 |
|---|---|---|
| **① 语义汇聚核** | ✅ 全部执行基座 + 内核原语 | `z42.core` |
| **② 可插拔工具 / 算法库** | ✅ 各库自持 | `z42.compression` / `z42.diagnostics` / `z42.test` / `z42.scripting` |
| **③ 纯脚本库** | ❌ 零 interop | 其余全部，含 **io / net / threading / json / text / crypto** |

三类的角色：

- **① `z42.core`** —— 唯一的 native 语义汇聚点。cross-cutting 内核原语（Object/String/数值协议、libm、
  时钟、位转换、熵、parse/format、反射、GC）**加上执行基座**（io / net / threading 的 OS 原语，住在
  `src/Native/NetNative.z42`、`src/Native/ThreadingNative.z42`、`src/IO/*Native.z42`）。core 是深度 0 +
  隐式 prelude，是唯一能承载共享 sink 的地方。
- **② 可插拔工具 / 算法库** —— native 是**可选插件**（codec、GC 内省、测试 harness、模块加载），正常
  程序执行不依赖；整库可单独编译、按需加载、从部署裁剪。
- **③ 纯脚本库** —— 要用 native 时**通过 core 或工具库的公开 API 间接调**。

### 每个能力拆两层

| 层 | 内容 | 归属 |
|---|---|---|
| **native 语义层** | `[Native]` / `extern` 原语，最低层 native 意义 | 执行基座 → core；可插拔工具 → 各自库 |
| **应用层** | 建在原语之上的**纯脚本**高层 API（`File` / `TcpClient` / `Thread` / `Stream` 家族） | 留在能力库；抽走语义后逻辑过薄的包直接删掉、并入 core |

这层「应用层 / 语义层」剥离是 z42 相对 .NET 的**额外**动作（.NET 的 native 与 BCL 同语言，没有
「脚本 vs native」之别），服务的是 Script-First 纪律。io / threading 语义入 core 对齐 .NET CoreLib
持有 `File` / `Thread`；compression / crypto / diagnostics 独立对齐 .NET 的独立程序集。

### 归属判据

```
这个 native 能力属于哪里？
  cross-cutting、全平台 VM 都能提供的基础原语
    （类型系统协议 / libm / 时钟 / 位运算 / 熵 / parse-format）    → z42.core
  执行基座：OS / socket / 线程 —— 缺了程序跑不起来                → z42.core（语义层）
                                                                  + 能力库（纯脚本应用层）
  可插拔工具 / 算法：codec / GC 内省 / test harness               → 独立能力库，整库自持 native
  纯计算逻辑                                                      → 纯脚本，零 interop
```

`Std.Diagnostics.Heap`（`DirectReferrers` / `RetainingRoots`，heap 反向可达查询）是「可插拔工具」的
样板：虽与 GC 深度耦合，但对正常执行非必需、可裁剪 → 语义 + 应用整体留 `z42.diagnostics`，不进 core。

**OS 熵是反向的样板**：`__crypto_random_bytes` 名字里带 crypto，但它是 OS 能力而非加密算法（同
`__time_now_*`），归 core `Std.Runtime.Entropy`——core 的 `Guid.NewGuid` 要用它，而 prelude 不能反
依赖 crypto。`z42.crypto.SecureRandom` 作为安全语义门面留在 crypto，委托 core 的熵原语。

### 配套纪律

1. **Script-First**：逻辑尽量放脚本；interop 只提供最小基础机制 / 原语，不在 native 侧堆高层逻辑。
2. **接口最小化**：interop 符号非必要不导出；对 interop 的包装保持薄封装，不叠便利方法。
3. **单一声明点**：每个 native 符号**跨包只声明一次**。cross-cutting 原语归 core；平台能力原语归其
   能力库。三族最容易被各处重复声明的符号，各自的唯一声明点是：
   - 位转换 `__single_*` / `__double_to_bits` / `__double_from_bits` → `Std.BitConverter`（`core/src/BitConverter.z42`）
   - 时钟 `__time_now_ms` / `__time_now_mono_ns` → `Std.Runtime.Clock`（`core/src/Clock.z42`）
   - OS 熵 `__crypto_random_bytes` → `Std.Runtime.Entropy`（`core/src/Entropy.z42`）

   收敛手法：调用方改调那个唯一声明点，需要保住调用点写法时加一层本地转发——
   `private static long __time_now_mono_ns() { return Clock.MonoNanos(); }`（`z42.test/src/Bencher.z42` 即此例）。

   **当前仍有一处跨包重复**：`__invoke_static` 在 `z42.test/src/ModuleLoader.z42` 与
   `z42.scripting/src/Engine.z42` 各声明一次。scripting 本就依赖 test，按本条应收敛到其中一处。

   > 这条规则管的是**跨包**。core 内部**同一符号被多个类型各绑一次**是刻意的，不算违规：8 个整型
   > （Int16/Int32/Int64/SByte/Byte/UInt16/UInt32/UInt64）的 `ToString` 都绑 `__int32_to_string`，
   > `Single.ToString` 借 `__double_to_string`，`Convert.ToInt32` 与 `Int32.Parse` 都绑 `__int32_parse`，
   > `String.Equals` 的两个重载共用 `__str_equals`。一个 native 实现服务多个脚本签名，正是
   > [Script-First](architecture.md#2-落点决策script-first) 想要的形状。
4. **性能升级阶梯**：脚本实现 → 优化脚本层 → 仍不达标才下沉 VM。详见
   [实现分层](architecture.md#2-落点决策script-first)。

## 4. R1 — 该进 `z42.core` 吗

```
该类型满足以下任一？
  (A) 是 VM intrinsic 接口（extern / [Native]）                     → ✅ 进 core
  (B) 是 primitive 类型的 stdlib 表示（Int32 / String / Double…）   → ✅ 进 core
  (C) 被类型系统 / 语言运行时直接消费的协议
      （Object 三件套、Exception 基类、IComparable<T> / IEquatable<T>
        —— 被泛型约束、catch 子句、is-pattern 触达）                → ✅ 进 core
  (D) 是基础容器类型本身（List<T> / Dictionary<K,V>）               → ✅ 进 core
                                                                      （只是类型本身；扩展方法 /
                                                                       比较器 / 序列化适配走外部包）
否则                                                                → ❌ 看 R2–R4
```

**设计目标：core 是「类型系统的物理底座」，不是「功能仓库」。**

两条彼此有优先级的规则：

- **Extension over Expansion（约束「未来新增」）**：新增类型方法 / 高层 trait 时，优先用外部包 +
  `impl Trait for Type` 扩展，而非塞回类型所在包。要求一个类型进 core 的人必须能回答*「为什么不用
  impl 在非 core 包里达到同样效果？」*——回答不出，归宿就在 core 之外。
- **不回溯迁移（优先级更高）**：core 内已有的接口 / 类型**不动**。`IComparer<T>` /
  `IEqualityComparer<T>` 迁不走，因为 core 容器的策略重载（`Sort(IComparer<T>)` /
  `Dictionary(IEqualityComparer<K>)`）必须能在 core scope 内引用它们，迁出即构成 core → 下游的反向
  依赖；`IFormattable` 与 core 的 ToString 协议内核耦合；`INumber<T>` 与 primitive 的
  `static override op_Add` 紧耦合，拆分工作量远大于收益。

这也是几条**明确不做**的事：不把 `IComparer<T>` / `IEqualityComparer<T>` / `IFormattable` /
`INumber<T>` 从 core 迁出；不把 Exception 子类从 core 拆散（高频用 + catch 友好）；不为「让 core 更瘦」
做破坏性重排。触发重新讨论的条件是具体痛点（core 编译时间过长 / prelude 加载体积超阈值），不是美学。

## 5. R2–R4 — 不进 core 的落点

| | 判定 | 例 |
|---|---|---|
| **R2 domain 包** | 只依赖 core（或少量同深度包）；内容形成一个自然 domain；纯脚本 | 次级集合 → `z42.collections`；文本 → `z42.text`；编解码 → `z42.encoding`；PRNG → `z42.random` |
| **R3 runtime 包** | 提供 OS / runtime 服务接口；**native 语义层在 core**，本包是纯脚本应用层 | 文件 / 流 / 进程 → `z42.io`；线程 / 锁 / channel → `z42.threading`；socket / HTTP → `z42.net` |
| **R4 领域包** | 依赖 runtime 服务，提供特定领域高层抽象；多数纯脚本 | JSON → `z42.json`；测试运行时 → `z42.test`；日志 → `z42.diagnostics`；正则 → `z42.regex`；CLI → `z42.cli` |

已经落在 core 而不是独立包的两处，别再提议搬出去：数学 libm 原语 `Std.Math`（随 CoreLib，等同
`System.Math`）、日期时间 `Std.Time`（`core/src/Time/`）。未来的非原语数值类型（BigInteger / 矩阵）
仍走 `z42.numerics`。

## 6. 命名

**包名**：全小写，顶级前缀恒为 `z42.`（多级允许但避免过深）。第三方包不限前缀。

**命名空间**：PascalCase，根命名空间 `Std`（对应 C# `System`）。

- 单包**可以**用多个命名空间——core 就横跨 `Std` / `Std.Collections` / `Std.IO` / `Std.Net.Sockets` /
  `Std.Reflection` / `Std.Runtime` / `Std.Threading` / `Std.Time`，因为它承载了全部执行基座语义层，而
  **语义层刻意保持与应用层同名空间**，这样应用层包的调用点无需改动。
- 单命名空间**可以**跨多个包（`Std.Collections` 由 core 的 `List`/`Dictionary` 与 collections 的
  `Queue`/`Stack`/`SortedSet`/`PriorityQueue`/`LinkedList` 共同提供）。合法的前提是类型名不冲突；同
  `(namespace, class-name)` 被两个包提供 → `E0601`。
- **`Std` / `Std.*` 是保留前缀**（同 Rust 保留 `std`/`core`/`alloc`），两层防线：第三方包源码声明
  `namespace Std.*` → `E0605` 硬错误；消费一个已构建的、NSPC 占用 `Std.*` 的第三方 zpkg → `W0603`
  软警告。E0605 保证程序里任何 `Std.*` 一定解析到官方 stdlib，永不被 shadow——这是 auto-import 安全的
  前提。

## 7. 新增包的 RFC 模板

任何 stdlib 新包提案必须在 `docs/spec/changes/<add-pkg>/proposal.md` 里回答：

```markdown
## R1 决策树
- (A) VM intrinsic？     [yes/no + 理由]
- (B) primitive stdlib？ [yes/no]
- (C) 被类型系统消费？   [yes/no]
- (D) 基础容器？         [yes/no]
- (E) 单 domain？        [yes/no + domain 名]
- (F) 需要 OS / runtime？[yes/no + 哪些服务]
→ **结论**：进 [z42.core / z42.<domain> / 不开新包]

## 依赖闭包
[列出本包依赖的所有上游 zpkg；必须无环，且要检查有没有把低深度包顶高]

## 可纯脚本化？
- 全部 .z42 ✅ / 否 ❌（说明哪些方法必须 native、按 §3 归到哪一类）

## 命名空间
- 选用：Std.<X>
- 理由：与 C# / Rust 对齐 / 现有 namespace 已饱和 / ...

## 替代方案
- 不开新包：在 z42.core 加 ... / 用 cross-zpkg impl 在现有包扩展 ...
- 选这个方案的理由：...
```

### 什么情况下不开新包

- **类型只有 1–3 个方法** → 放已有包，避免 zpkg 碎片化。
- **与现有包 80% 重合的 domain** → 合进去，用子命名空间区分（`Std.Collections.Concurrent`）。
- **只是想分离测试** → 测试不该是独立 stdlib 包；用 `z42.test` + 用户工程的 test 目录。
- **不是 z42 官方功能** → 不进 `z42.<x>`，用户自己开包。
- **新加的是「从某种源读 / 写」能力** → 实现一个 `: Stream` 适配器复用既有 operation，不开新包也不开
  一组 `XxxFromYyy` 接口，见 [API 设计准则](api-guidelines.md)。

## 8. z42 的选择落在哪

C# BCL 与 Rust std 是两个极端：C# 的分发单位是 assembly、CoreLib 很大（Console / File / Thread 全在
里面）、命名空间嵌套且与 assembly 解耦；Rust 的 std 是**单个 crate**、`core` 极小（无 OS 无堆）、
模块单层、扩展全交给第三方 crates。

**z42 取 C# 的分发模型 + Rust 的层次纪律**：包就是 zpkg 文件（C# 风的多包分发），命名空间 `Std.<X>`
可以跨 zpkg（与物理包解耦，同 C#）；但 core 的收缩纪律向 Rust 看齐——core 只放「类型系统底座 +
执行基座 native 语义」，功能一律外推，靠 cross-zpkg `impl Trait for Type` 而不是把接口塞进 CoreLib。

两个 z42 独有的约束把这个选择钉死：

1. **包名 = zpkg 文件名**，`z42.core.zpkg` / `z42.io.zpkg` 是物理产物。
2. **执行基座 native 语义层集中在 core**（见 §3）——这是 C# 没有、Rust 也没有的一刀，来自 z42
   「脚本 vs native」这条 C#/Rust 都不存在的分界。
