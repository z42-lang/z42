# z42 标准库模块划分规则

> **目的**：给出 z42 stdlib 包（zpkg）划分的可重用规则，参考 C# BCL + Rust std 实践，
> 让后续添加新包（z42.threading、z42.json、z42.linq 等）有明确的依据。
>
> **受众**：stdlib 设计者、新包提案者、reviewer。
> **不是**：用户使用文档（用户视角看 `docs/design/language/language-overview.md`）。

---

## TL;DR — 核心规则

1. **每个 zpkg 是分发单元，对应 `[project] name = "z42.<domain>"`**；命名空间可跨 zpkg。
2. **z42.core 是隐式 prelude** — 所有程序自动加载 + 不可声明依赖。
3. **interop（extern/[Native]）只允许在 `z42.core` + 独立平台能力库**（io / net / threading / compression 等承载平台相关、某平台可能缺席的能力的库）；**其余库一律纯脚本、零 interop → 全平台共享**。详见下「[平台边界库 vs 全平台共享库](#平台边界库-vs-全平台共享库interop-收缩规则)」。（取代旧「VM extern 只能在 core，io 唯一例外」的表述。）
4. **包按层叠层分类**（L0 → L1 → L2 → L3），上层依赖下层，**禁止反向依赖** —— 这是硬约束，决定了哪些接口必须在 core。stdlib 层级是更通用规则"包依赖必须无环"（见 [project.md L5 — 依赖必须无环](../compiler/project.md#依赖必须无环no-circular-dependencies)）的特化形式：stdlib 不仅要求 DAG，还规定了固定的层级顺序。
5. **「Extension over Expansion」**：**未来新增**类型方法 / 高层 trait 时，优先用外部包 + `impl Trait for Type` 扩展（L3-Impl2 已支持），而非塞回类型所在包。**已有 core 内容不做回溯迁移**（见规则 #6）。
6. **不回溯迁移 core 内已有的接口/类型**：core 自身的实现（如 List 的 IEquatable / IComparable 约束、Dictionary 的 IEquatable 约束）以及未来可能添加的策略重载（`Sort(IComparer<T>)` / `Dictionary(IEqualityComparer<K>)`）都需要这些 protocol 在 core scope 内；迁出会让 core → L1 形成反向依赖，违反规则 #4。**这条规则比 #5 优先级高。**
7. **每个新包必须明确层级 + 依赖闭包 + 是否可纯脚本化**；不满足全部的不开新包，留 backlog。

---

## 平台边界库 vs 全平台共享库（interop 收缩规则）

> **2026-08-03 确立。** 取代旧「VM extern 只能在 z42.core，z42.io 唯一例外」规则——后者过窄
> （现实中 math / time / threading / net / compression / crypto / io.binary / test 都含 interop）
> 且基于一个误解（见下「澄清」）。这是 interop 归属的**唯一 SoT**，`libraries/README.md §2` 只摘要 + 链接此节。

### 规则

stdlib 的每个库属于且仅属于以下两类之一：

| 类别 | 允许 interop？ | 全平台可用？ | 成员 |
|------|:---:|:---:|------|
| **① 平台边界库** | ✅ | ❌（某平台可能缺席/不同）| `z42.core`（特殊，见下）+ 独立能力库：`z42.io` / `z42.net` / `z42.threading` / `z42.compression` … |
| **② 全平台共享库** | ❌（纯脚本，零 interop） | ✅（VM 能跑就能跑） | 其余全部：collections / math / text / encoding / time / toml / json / yaml / uri / regex / cli / diagnostics / random / numerics / io.binary … |

**① 平台边界库**又分两种角色：

- **`z42.core`** —— 承载**全平台通用的基础机制原语**（cross-cutting：Object/String/数值等类型系统
  协议、libm 数学、时钟、位转换、数值 parse/format）。这些能力**每个平台的 VM 都能提供**，是"人人都用"
  的底座。core 是 L0（在所有库之下）且是隐式 prelude，所以**它是唯一能承载 cross-cutting 原语的地方**
  ——core 之下没有更底的层可放共享 sink。
- **独立能力库**（io / net / threading / compression …）—— 承载**平台相关、某些平台可能缺失或不同**
  的能力（OS 文件/进程、socket、线程、native codec）。各自封装自己的 interop，**本身就不保证全平台可用**
  （如 browser 无多线程 → `z42.threading` 在该平台缺席）。

**② 全平台共享库**要用 native 能力时，一律**通过调用 core 或某个平台能力库的公开 API 间接使用**，
自身不声明任何 extern/[Native]。

### 归属判据（新库/新方法）

```
这个 native 能力属于哪里？
  是 cross-cutting、全平台 VM 都能提供的基础原语
    （类型系统协议 / libm / 时钟 / 位运算 / parse-format）？   → z42.core
  是平台相关、某平台可能缺席/不同的能力（OS / socket / 线程 / codec）？
    → 归入对应的独立能力库（无则按 R3/R4 评估新开一个能力库）
  两者都不是（纯计算逻辑）？                                    → 纯脚本，放全平台共享库，零 interop
```

### 配套纪律

1. **Script-First**：逻辑尽量放脚本；interop 只提供**最小基础机制/原语**，不在 native 侧堆高层逻辑。
2. **接口最小化**：interop 符号**非必要不导出**；对 interop 的包装保持**薄封装**，不叠便利方法。
3. **单一声明点**：每个 native 符号**全仓库只声明一次**。cross-cutting 原语归 core；平台能力原语归其能力库。
   （现存违规：`__double_to_bits`/`__single_*`/`__double_from_bits` 在 z42.io.binary + z42.ir 双声明；
   `__time_now_ms` 在 z42.time/z42.io/z42.net 三声明；`__time_now_mono_ns` 在 z42.time + z42.test 双声明——待收敛。）
4. **性能升级阶梯**：**脚本实现 → 持续优化（JIT / 算法 / VM 调用机制提速）→ 仍不达标 → 才下沉为 VM 内置实现**。
   VM 内置是最后手段，不是默认——优先投资"让脚本层本身更快"的通用机制，从而**尽量保持逻辑在脚本层**。

### 澄清：收缩 interop 与「zpkg 跨平台字节相同」的关系

zpkg 是可移植字节码；z42c 把 `[Native]` 只降低成 **按名字（字符串池下标）** 引用的 BuiltinInstr /
CallNativeInstr，native 函数在 **VM 加载期按名解析**，编码里无任何 target/arch/位宽/字节序信息。stdlib
源码零条件编译。**因此所有 stdlib zpkg（含 core）本就跨平台字节相同**，与 interop 声明在哪儿无关。真正随
平台变的是 **Rust VM 二进制 + native 动态库**（如 `libz42_compression.*`），不是 zpkg。

所以本规则收缩 interop 面**不是**为了"让其他库字节相同"（那已成立），而是为了：

1. **可运行性契约显式化**：纯脚本库 = "VM 能跑的地方就能跑"的保证；平台能力库 = "可能在某平台缺席"的显式边界
   （这是"不同平台实现不同"——如 browser 无线程——真正落点的地方）。
2. **单一、可审计的 native ABI 契约**：native 表面集中、可数、可 review。
3. **消灭重复声明**（纪律 3）。

---

## 现状（2026-04-26）

| 包 | 层级 | 内容（节选） | extern？ |
|----|----|----|----|
| `z42.core` | L0 | Object、primitive 类型（int/long/double/float/char/bool/string）、Type、Convert、Assert、Exception 树、核心接口（IComparable / IEquatable / IComparer / IEqualityComparer / IFormattable / IDisposable / IEnumerable / INumber）、Collections/{List, Dictionary} | ✅ VM intrinsic |
| `z42.collections` | L1 | Stack、Queue、LinkedList、SortedSet（计划：SortedDictionary、PriorityQueue） | ❌ 纯脚本 |
| `z42.text` | L1 | StringBuilder（纯脚本，2026-04-26 迁移）；Regex 占位 | ❌ 纯脚本 |
| `z42.encoding` | L1 | Hex、Base64（RFC 4648 §4）、Utf8 | ❌ 纯脚本 |
| `z42.io` | L2 | Console、File、Path、Environment | ✅ host FFI（仅此包例外） |
| `z42.time` | L1 | DateTime（UTC 时刻）、TimeSpan（时间段）、Stopwatch（单调计时器） | ❌ 纯脚本（时钟经 core `Std.Runtime.Clock`——A1 起，不再自声明 `__time_now_*`） |
| `z42.toml` | L1 | TomlValue（discriminated union）、TomlException、TOML 1.0 subset reader/writer | ❌ 纯脚本 |
| `z42.json` | L1 | JsonValue（discriminated union）、JsonException、JSON RFC 8259 reader/writer | ❌ 纯脚本 |
| `z42.random` | L1 | Random（PCG-XSH-RR 64→32），seeded deterministic PRNG | ❌ 纯脚本（wall-clock seed 走 z42.time） |
| `z42.uri` | L1 | Uri / UriException / UriCodec，RFC 3986 子集 parser + percent codec | ❌ 纯脚本 |
| `z42.io.binary` | L1 | BinaryReader / BinaryWriter / BinaryException：LE+BE int16/32/64 + UTF-8 string + byte[] helper | ❌ 纯脚本 |
| `z42.diagnostics` | L1 | Log / LogLevel：全局 facade，5 level，stderr 输出 | ❌ 纯脚本 |
| `z42.regex` | L1 | Regex / Match / RegexException：backtracking NFA，字面+`.`+量词+字符类+分组+alternation | ❌ 纯脚本 |
| `z42.cli` | L1 | ArgParser / ParseResult / CliException：flag + option + positional + auto -h/--help（Phase 0 of script self-hosting） | ❌ 纯脚本 |
| `z42.threading` | L2 | Thread / ThreadException + Mutex<T> / RwLock<T> (多读单写) / Channel<T> (unbounded + `new Channel<T>(N)` bounded with back-pressure) / ChannelDisconnectedException：OS 线程 spawn / join + 同步原语 | ✅ VM intrinsic（`__thread_*` / `__mutex_*` / `__rwlock_*` / `__channel_*`） |

> 历史角度：W1（2026-04-25）把 List / Dictionary 从 z42.collections 上提到 z42.core/Collections/，对齐 C# BCL 把 `System.Collections.Generic.List<T>` 放在 CoreLib 的做法（每个程序都在用）。

---

## 参考体系：C# BCL vs Rust std

### C# BCL（CLR base class library）

**结构**：
```
System.Private.CoreLib (CoreLib)
  ├── System.{Object, String, Int32, Boolean, ...}
  ├── System.{IComparable, IDisposable, IEquatable<T>, ...}
  ├── System.Collections.Generic.{List<T>, Dictionary<K,V>, ...}
  ├── System.IO.{File, Path, Stream, Console, ...}
  ├── System.Text.{StringBuilder, Encoding, ...}
  └── System.Threading.{Thread, Monitor, ...}
System.Console.dll                  ← 现代 .NET 拆出来
System.Linq.dll                     ← LINQ 扩展
System.Net.Http.dll                 ← HTTP
System.Text.Json.dll                ← JSON
```

**关键观察**：
- **CoreLib 巨大** —— 所有"每个程序都可能用到"的东西都进 CoreLib（包括 File / Console / StringBuilder / Thread）
- **namespace ≠ assembly** —— `System.IO.File` 在 CoreLib，`System.IO.FileSystemWatcher` 在另一个 dll。namespace 是组织逻辑，assembly 是分发单元
- **后续拆分** —— 新 .NET 把 `System.Console`、`System.Linq`、`System.Net` 等拆成独立 dll，让 trim / publish 能去掉不用的部分。但 namespace 没变。
- **顶级层级**：`System` (≈ core) → `System.<domain>` (IO / Text / Collections / Threading / Net / Linq / Json)

### Rust std

**结构**：
```
core (no_std, no allocator)
  ├── core::{Option, Result, mem, ptr, slice, str, ...}
  ├── core::cmp / hash / fmt / iter
  └── core::ops / convert
alloc (with allocator, no OS)
  ├── alloc::{Vec, String, Box, Rc, BTreeMap, HashMap, ...}
  └── alloc::fmt
std (full OS)
  ├── std::io::{stdin, stdout, File, BufReader, ...}
  ├── std::fs / path / env / process / thread / sync / time
  └── std::net / collections (re-export)
```

**关键观察**：
- **三层**：core (零依赖) → alloc (需要堆分配器) → std (需要 OS)。每层是单独的 crate，但用户大多透明用 std
- **std 是单一 crate**，里面按 module 分（`std::io`、`std::collections`）；所以"包"就一个，"模块"很多
- **第三方负责扩展**：`tokio` / `serde` / `reqwest` 不在 std，由 cargo 生态处理。和 .NET 把 `System.Text.Json` 放官方 dll 风格不同
- **顶级 module 命名**：io / fs / path / env / collections / sync / thread —— 都是单层

### 对比小结

| 维度 | C# BCL | Rust std |
|---|---|---|
| 分发单位 | dll (assembly) | crate |
| 命名空间深度 | 嵌套（System.IO.File） | 单层（std::io::File） |
| Core 内容 | 大（Console/File/Thread 都在） | 极小（core 无 OS、无堆） |
| 扩展机制 | 官方 NuGet 包 + 第三方 | 第三方 crates |
| trait/interface 实现位置 | 跟着类型定义 | trait/类型可以在不同 crate（孤儿规则约束） |

---

## z42 的设计选择（差异化）

z42 处于 **C# 风（包就是分发单位 + 大 stdlib 集合）** 与 **Rust 风（小 std + 第三方繁荣）** 之间，但加了两个独有约束：

1. **包名 = zpkg 文件名** — `z42.core.zpkg`、`z42.io.zpkg` 是物理产物。命名空间 `Std.<X>` 可以跨多个 zpkg（W1 决策）
2. **VM extern 只在 z42.core**（host FFI 仅 z42.io 例外） — 决定了"哪些功能必须捆在哪个包"

z42 的层级模型把 C#/Rust 两边的精华都拿过来：

```
L0  z42.core              (隐式 prelude，VM intrinsic 唯一来源)
        ↑ 依赖
L1  z42.collections, z42.text, z42.encoding, ...
    （纯脚本，按 domain 分；可任意组合用）
        ↑
L2  z42.io                (host FFI 例外，OS 能力)
    z42.threading         (待 L3 并发模型；走 host FFI / VM 协作)
    z42.async             (待 L3 async；同上)
        ↑
L3  z42.net, z42.linq, z42.json, z42.test, z42.diagnostics, ...
    （依赖 L2 runtime 服务）
```

---

## 划分规则（推荐操作清单）

### R1 — 「应该进 z42.core 吗？」决策树

判断一个类型/接口是否进 z42.core：

```
该类型是否满足以下任一条件？
  (A) 是 VM intrinsic 接口（extern / [Native]）        → ✅ 必须进 z42.core
  (B) 是 primitive 类型的 stdlib 表示（struct int / class String）  → ✅ 进 z42.core
  (C) 是被 **类型系统/语言运行时** 直接消费的协议
      （Object 三件套：Equals/GetHashCode/ToString；Exception 基类；
        IComparable<T>/IEquatable<T>：被泛型约束、catch 子句、is-pattern 等触达）
      → ✅ 进 z42.core
  (D) 是基础容器类型本身（List<T>、Dictionary<K,V>） → ✅ 进 z42.core
      （只是**类型本身**；它的扩展方法 / 比较器 / 序列化适配走 L1 impl）
否则 → ❌ 不要进 z42.core，按下游决策树看 R2-R4
```

> **设计目标**：z42.core 是"类型系统的物理底座"，不是"功能仓库"。
> 凡是可以通过 `impl Trait for Type` 在外部包追加的能力，**默认放外部**。

**与 C# BCL 的差异化（适用于"未来新增"，非回溯迁移）**：

C# CoreLib 把 `IComparer<T>` / `IFormattable` / `IConvertible` / 大量 `IEnumerable<T>` 扩展方法
都塞在了 CoreLib，是因为 C# 没有真正的"跨 assembly trait 扩展"机制（extension method 只是
语法糖，不能让 `int` 后期才"实现"`INumber<int>` 接口）。z42 有了 cross-zpkg `impl Trait for
Type`（L3-Impl2，2026-04-26），**未来新增** trait / 类型时可以让 core 比 C# CoreLib 更瘦：

| 协议 / 类型 | C# 放哪 | z42 应该放哪 | 理由 |
|---|---|---|---|
| `Object` 协议（Equals 等）| CoreLib | `z42.core` ✅ | 类型系统直接消费 |
| `Exception` 基类 + 常用子类 | CoreLib | `z42.core` ✅ | catch 子句 + 控制流核心 |
| `IComparable<T>` / `IEquatable<T>` | CoreLib | `z42.core` ✅ | List/Dictionary 自身约束依赖（**不能迁**）|
| `IComparer<T>` / `IEqualityComparer<T>` | CoreLib | `z42.core` ✅ | core 容器的 strategy 重载（`Sort(IComparer<T>)` / `Dictionary(IEqualityComparer<K>)`）必须能在 core scope 内引用，否则 core → collections 反向依赖 |
| `List<T>` / `Dictionary<K,V>` 类型 | CoreLib | `z42.core` ✅ | "实质类型"被 syntactic-sugar / 泛型推断 hardcode |
| `IFormattable` | CoreLib | `z42.core` ✅ | core 的 String / `__str_format` builtin 内核 ToString 协议（保守留 core） |
| `IEnumerable<T>` 类型本身 | CoreLib | `z42.core` ✅ | foreach 鸭子协议依赖 |
| `IEnumerable<T>` LINQ 扩展（Where / Select / OrderBy 等）| System.Linq.dll | `z42.linq` 🔄（未来新包）| 高层扩展，纯 impl 套路 —— 不动 core 任何已有内容 |
| `INumber<T>` / 数值 trait | CoreLib | **可以** 留 core ✅ 或迁 z42.numerics 🔄 | 不被 core 内部消费，理论可迁；但与 primitive `int.op_Add` 紧耦合，**实际落地工作量 vs 收益不划算**，按"不回溯"规则保持 |
| 全新数学类型 BigInteger / Decimal / 复数 | — | `z42.numerics` 🔄（未来新包）| Pure new domain，从开始就走外部包 |
| 全新文本协议（如 IRegexProvider 等） | — | `z42.text` 🔄 | 同上 |
| `int.op_Add` 等 primitive 数值算子 | CoreLib（运算符重载语法）| `z42.core` ✅ | 与 primitive 类型一同声明，跨 zpkg 拆分代价大；保守留 core |

🔄 = 未来新增可考虑的差异决策；✅ = 与 C# 一致 / 必须留 core。

**判断规则**：
- **新增**：要求一个类型/接口进 z42.core 的人，必须能回答 *"为什么不用 impl 在非 core
  包里达到同样效果？"*。回答不出 → 该接口/方法的归宿是 L1+。
- **已有**：core 内已有的接口/类型 **不做回溯迁移** —— 价值在于"未来添加新方法时
  默认放外部包"，而不是把现有结构推倒重来。

### R2 — L1 domain 包划分

L1 包的判定：

- 只依赖 z42.core，**不依赖**别的 L1 / L2 / L3 包
- 内容形成一个**自然 domain**（collections / math / text / numerics / time / encoding / ...）
- **纯脚本**实现（无 extern，无 host FFI）
- 以"每个特性为一组类型"为单位，**单包内类型彼此互引** OK，跨 L1 包零依赖

| domain → 包 |
|---|
| 次级集合（Stack / Queue / LinkedList / PriorityQueue）→ `z42.collections` |
| 数学 libm 原语 `Math`（= `System.Math`）→ **`z42.core`**（move-math-to-core A2 迁入，随 CoreLib）；未来非原语数值类型（BigInteger / 矩阵）→ `z42.numerics` |
| 文本（StringBuilder / Regex / Encoding） → `z42.text` |
| 数值扩展（BigInteger / Decimal / 复数）→ `z42.numerics`（未来）|
| 日期时间（DateTime / TimeSpan）→ `z42.time`（未来；纯脚本可行性待评估）|

### R3 — L2 runtime 包

L2 包的判定：

- 依赖 z42.core（必然）+ 可选依赖其他 L2（如 threading 依赖 io 做 stderr 输出）
- 提供 **OS / runtime 服务接口**（文件、网络、线程、计时器）
- 通常需要 host FFI（z42.io 已是例外） OR 与 VM 紧密协作

| runtime → 包 |
|---|
| 文件 / 控制台 / 环境 / 进程 / 时间钟 → `z42.io` |
| 线程 / 锁 / 原子 / channel → `z42.threading`（需 L3 并发模型） |
| async / await runtime / Task → `z42.async`（需 L3 关键字 + scheduler） |

### R4 — L3 外部包

L3 包的判定：

- 依赖 L2 runtime 服务（io / threading / async）
- 提供**特定领域的高层抽象**（HTTP / JSON / LINQ / 测试框架）
- 多数纯脚本即可，少数性能敏感的可下沉到 z42.core 加 intrinsic（但需要单独 RFC）

| 高层 → 包 |
|---|
| HTTP / Socket / URL → `z42.net` |
| JSON 序列化 / 反序列化 → `z42.json` |
| LINQ 风格 IEnumerable 扩展 → `z42.linq` |
| 测试运行时（Test 注解、Assert 扩展）→ `z42.test` |
| 调试 / 计时 / 日志 → `z42.diagnostics` |

---

## 命名规则

### 包名

- 全小写：`z42.core` / `z42.io` / `z42.net.http`（多级允许，但避免过深）
- 顶级前缀始终 `z42.` —— 区分官方 stdlib vs 第三方
- 第三方包不限前缀（用户自由）

### 命名空间

- 大小写按 PascalCase，根命名空间 `Std`（与 C# `System` 对应）
- 单包**可以**用多个命名空间（罕见，需理由）
- 单命名空间**可以**跨多个包（如 `Std.Collections` 在 z42.core + z42.collections）—— 但**新代码倾向单包单命名空间**

| 包 | 命名空间 |
|---|---|
| z42.core | `Std` (primitive + protocols), `Std.Collections` (List, Dictionary) |
| z42.collections | `Std.Collections` (Stack, Queue, ...) |
| z42.io | `Std.IO` |
| z42.text | `Std.Text` |
| z42.threading | `Std.Threading` |
| z42.net | `Std.Net.Sockets` (K1: TCP), `Std` (exceptions: SocketException / SocketClosedException / NetException / NetUnsupportedException) |
| z42.numerics | `Std.Numerics` (v0: BigInt) |
| z42.async（未来）| `Std.Async` |

---

## 决策模板：新增包 RFC

任何 stdlib 新包提案必须回答以下问题（在 `docs/spec/changes/<add-pkg>/proposal.md` 里）：

```markdown
## R1 决策树
- (A) VM intrinsic？     [yes/no + 理由]
- (B) 基础协议？         [yes/no]
- (C) 每个程序都用？     [yes/no + 频率估计]
- (D) primitive stdlib？ [yes/no]
- (E) 单 domain？        [yes/no + domain 名]
- (F) 需要 OS / runtime？[yes/no + 哪些服务]
→ **结论**：进 [z42.core / z42.<domain> / 不开新包]

## 依赖闭包
[列出本包依赖的所有上游 zpkg，必须形成 DAG，不允许圈]

## 可纯脚本化？
- 全部 .z42 ✅ / 否 ❌（说明哪些方法必须 native + 走哪条通道）

## 命名空间
- 选用：Std.<X>
- 理由：与 C# / Rust 对齐 / 现有 namespace 已饱和 / ...

## 替代方案
- 不开新包：在 z42.core 加 ... / 用 cross-zpkg impl 在现有包扩展 ...
- 选这个方案的理由：...
```

---

## 应用：当前布局复审 + 建议变更

### 复审结论（2026-04-26 修订：不回溯迁移）

| 包 | 范围 | 建议 |
|---|---|---|
| `z42.core` | 类型 + 类型系统协议 + 容器策略接口 | ✅ **保持** —— 已有内容不迁出 |
| `z42.collections` | 次级容器（Stack / Queue / 未来 LinkedList / PriorityQueue）| ✅ 保持 |
| ~~`z42.math`~~ | 数学函数 | ⟶ **已并入 z42.core**（`Std.Math` = System.Math，move-math-to-core A2）；新数值类型走 `z42.numerics` |
| `z42.text` | 文本处理（StringBuilder / 未来 Regex）| ✅ 保持 |
| `z42.io` | OS 服务 | ✅ 保持；Console 走隐式 prelude using（见末尾决策） |

### z42.core 已有内容：不迁出（rationale）

最初本节有"瘦身候选迁出清单"，复审后**取消所有迁出建议**：

| 接口 / 类型 | 最初设想迁出到 | 取消理由 |
|---|---|---|
| `IComparer<T>` | z42.collections | core 容器未来扩展 `Sort(IComparer<T>)` 重载需要它在 scope 内；core → collections 反向依赖不允许 |
| `IEqualityComparer<T>` | z42.collections | 同上：`Dictionary(IEqualityComparer<K>)` 重载将引用它 |
| `IFormattable` | z42.text | core 的 `__str_format` 内核 ToString 协议保守留 core；迁出收益小 |
| `INumber<T>` | z42.numerics | 与 primitive `int` / `double` 的 `static override op_Add` 等紧耦合，跨 zpkg 拆分工作量 vs 收益不划算；按"不回溯"规则保持 |

> **核心结论**：**core 内已有的接口/类型不动**。"Extension over Expansion" 原则的价值在
> **未来新增**时（新方法 / 新协议 / 新类型）默认放外部包，而不是回溯重排既有结构。
> 这与 W1（List/Dict 上提 core）的决策**不矛盾** —— 两次决策都遵循"基础设施一旦稳定就不
> 折腾"的工程原则。

### z42.core 仍可"瘦"的位置（如果将来动）

非建议立即做，仅作记录 —— 未来真要瘦身时可考虑：

- **Exception 子类拆分**：当前 `z42.core/Exceptions/` 有 9 个常用子类；若发现某些极少用（如 `KeyNotFoundException` 仅 Dictionary 用、`IndexOutOfRangeException` 仅数组用），可拆到对应 domain 包。但要先有用例驱动，不主动拆。
- **`Convert` 静态类**：`Convert.ToString` 等可能将来与 `IFormattable` 一起评估归属，但仅当 `IFormattable` 真要迁出时才动。

**触发条件**：除非出现具体用户痛点（如 core 编译时间过长、prelude 加载体积超阈值），否则不动。

### Console 是否上提到 z42.core？

**论据 PRO（上提）**：
- 几乎每个 z42 程序都用 `Console.WriteLine`
- C# 把 `System.Console` 放在 CoreLib（虽然 .NET 拆出来过，但仍然是 prelude 体验）
- Rust 的 `println!` 是 std 内置（虽然 std 也含 io），等同 prelude 调用
- 现状很多测试代码确实直接 `Console.WriteLine` 没显式 `using Std.IO;`，说明编译器把 Console 当作隐式
- 移到 z42.core 后用户写 hello world 完全不需要任何 `using` —— 与 C# 顶级语句体验一致

**论据 CON（保持）**：
- Console 实现需要 host FFI（stdin/stdout） —— 把 host FFI 引入 z42.core 会破坏当前"VM extern 集中在 z42.core，host FFI 集中在 z42.io"的清晰边界
- 现在 `Console` 走 host FFI（不是 VM intrinsic），与 z42.core 的 `[Native(...)]` 是两套通道；上提需要 z42.core 同时承载两类 native 接口
- 拆得太细可能反而增加规范复杂度

**初步建议**（待 User 拍板）：
- **方案 A（保持现状）**：Console 留在 z42.io，但**让编译器把 `Std.IO` 加进隐式 prelude using 列表**（与 z42.core 一起自动加载）。结果：用户体验同"在 core"，物理边界仍清晰。 **推荐。**
- **方案 B（上提 z42.core）**：把 Console.z42 + 相关 `__println` / `__print` / `__readline` builtin 从 z42.io 搬到 z42.core，更新"VM extern 集中在 z42.core" 规则的措辞为"VM intrinsic + console host FFI 集中在 z42.core"。物理统一，但规则措辞需要细分。
- **方案 C（不变）**：保持现状，文档说明"Console 需要 `using Std.IO;`"（与现状不一致，需改测试代码）。 **不推荐**。

File / Path / Environment **不应**上提（不是每个程序都需要，不在 prelude 路径上）。

### Console 是否上提到 z42.core？

**论据 PRO（上提）**：
- 几乎每个 z42 程序都用 `Console.WriteLine` —— 满足 R1 (C)
- C# 把 `System.Console` 放在 CoreLib（虽然 .NET 拆出来过，但仍然是 prelude 体验）
- Rust 的 `println!` 是 std 内置（虽然 std 也含 io），等同 prelude 调用
- 现状很多测试代码确实直接 `Console.WriteLine` 没显式 `using Std.IO;`，说明编译器把 Console 当作隐式
- 移到 z42.core 后用户写 hello world 完全不需要任何 `using` —— 与 C# 顶级语句体验一致

**论据 CON（保持）**：
- Console 实现需要 host FFI（stdin/stdout） —— 把 host FFI 引入 z42.core 会破坏当前"VM extern 集中在 z42.core，host FFI 集中在 z42.io"的清晰边界
- 现在 `Console` 走 host FFI（不是 VM intrinsic），与 z42.core 的 `[Native(...)]` 是两套通道；上提需要 z42.core 同时承载两类 native 接口
- 拆得太细可能反而增加规范复杂度

**初步建议**（待 User 拍板）：
- **方案 A（保持现状）**：Console 留在 z42.io，但**让编译器把 `Std.IO` 加进隐式 prelude using 列表**（与 z42.core 一起自动加载）。结果：用户体验同"在 core"，物理边界仍清晰。 **推荐。**
- **方案 B（上提 z42.core）**：把 Console.z42 + 相关 `__println` / `__print` / `__readline` builtin 从 z42.io 搬到 z42.core，更新"VM extern 集中在 z42.core" 规则的措辞为"VM intrinsic + console host FFI 集中在 z42.core"。物理统一，但规则措辞需要细分。
- **方案 C（不变）**：保持现状，文档说明"Console 需要 `using Std.IO;`"（与现状不一致，需改测试代码）。 **不推荐**。

File / Path / Environment **不应**上提（不是每个程序都需要，不在 prelude 路径上）。

---

## 反例（什么情况下不开新包）

- **类型只有 1-3 个方法**：放在已有包里，避免 zpkg 碎片化
- **跟现有包 80% 重合的 domain**：合进现有包，用子命名空间区分（`Std.Collections.Concurrent` vs `Std.Collections`）
- **只是想分离测试**：测试不该是一个独立 stdlib 包；用 `z42.test` 框架 + 用户项目的 test 目录
- **第三方功能**：不是 z42 官方的，**不进 z42.<x>** —— 用户自己开包

---

## 与既有规范的关系

- 本文档：**规则与决策模板**（pre-write 检查清单）
- `src/libraries/README.md`：**实现规范**（Script-First / VM extern 边界 / 构建流程）—— 偏向"怎么写"
- 单包内的设计：每个 stdlib 包自己的 README（如 `z42.core/README.md`）讲该包内部的拆分、API 风格、迁移历史

---

## 后续动作（建议）

### 现阶段（不做回溯迁移）

1. **prelude-implicit-using**（独立可做）：编译器层把 `Std.IO`（以及 `Std.Collections` / `Std.Math` / `Std.Text`）加进隐式 prelude using 列表
   - 解决 Console "hello world 不需要 using" 体验
   - 与 z42.core 自动加载机制类似，纯编译器改动
   - 不影响任何包的物理结构

2. **未来开新包时**：按 R1 决策树 + 「Extension over Expansion」原则
   - 新 trait / 新方法默认放外部包
   - 通过 `impl Trait for Type` 给 core 类型扩展（不改 core）
   - 必须填 RFC 模板（本文档"决策模板"小节），归到 `docs/spec/changes/<add-pkg>/proposal.md`

3. **流程**：与 `libraries/README.md` 实现规范交叉引用 + 把"层级模型 + 不回溯迁移"原则加到 README（精简版）

### 不做的事（明确约束）

- ❌ 不把 `IComparer<T>` / `IEqualityComparer<T>` 从 core 迁出（会卡 `Sort(IComparer<T>)` 等未来扩展）
- ❌ 不把 `IFormattable` / `INumber<T>` 从 core 迁出（与 core 内核协议 / primitive 类型紧耦合）
- ❌ 不把 Exception 子类从 core 拆分（高频用 + catch 子句友好）
- ❌ 不为追求"core 更瘦"做任何破坏性重排

---

## 历史决策对照

- **W1（2026-04-25）**：把 `List<T>` / `Dictionary<K,V>` 从 z42.collections **上提到** z42.core/Collections/，理由是"对齐 C# BCL"。
- **本文 v1（2026-04-26 早）**：曾建议进一步把 `IComparer` / `IEqualityComparer` / `IFormattable` 从 core 迁出 —— 后撤回。
- **本文 v2（2026-04-26 晚，当前版本）**：明确**"已有不动 + 未来按 Extension over Expansion 走外部包"**两条规则。

三次决策的连续逻辑：**"基础设施一旦稳定就不折腾，但增量空间留给外部包"**。

---

## Primitive vs Feature（BCL/Rust 对标）

> **2026-04-26 新增**：与 Script-First 原则配套的执行准则。
> 本节与"不回溯迁移"规则**正交** —— 后者约束**类型在包之间搬家**（防止反向依赖），
> 本节约束**单个 API 的实现层级**（VM intrinsic vs 脚本）。两条规则同时生效。

### 核心准则

> **Runtime 提供 primitive，feature 一律脚本实现。**

- **Primitive**：JIT 无法消除的硬性能力 —— 内存布局 / 原子操作 / GC barrier / syscall /
  libm FPU 指令 / UTF-8 codepoint 访问 / 类型元数据 / parse 数值字面量
- **Feature**：集合算法 / 格式化 / Assert / 字符串拆分拼接 / Path 字符串操作 / 算术
  辅助（abs/max/min）/ Bool 三件套（Equals/HashCode/ToString） —— **必须**脚本实现

### BCL / Rust 标杆

两边对"什么算 primitive"画得很死，z42 沿用同一边界：

| 子系统 | .NET CoreLib | Rust std/core | z42 应当 |
|---|---|---|---|
| `List<T>` / `Vec<T>` | C# 源码 atop `T[]` | Rust 源码 atop `RawVec<T>` | ✅ 已脚本 atop `T[]` |
| `Dictionary` / `HashMap` | C# 源码 atop `Entry[]` | Rust 源码（hashbrown SwissTable）| ✅ 已脚本 |
| `StringBuilder` | C# atop `char[]` chunks | Rust `String` 自身 | ✅ 已脚本（2026-04-26）|
| `Debug.Assert` / `assert!` | C# 一句 `if (!cond) throw` | Rust 宏展开 `if !cond { panic!() }` | 🔄 待 Wave 1 |
| `String.Split` / `str::split` | C# Span 扫描 | Rust iterator | 🔄 待 Wave 1 |
| `Math.Abs(int)` / `i32::abs` | C# 三元 if | Rust intrinsic-free | 🔄 待 Wave 1 |
| `Path.Combine` / `Path::join` | C# 纯字符串拼 + 平台分隔符 | Rust 纯 OsStr 拼 | 🔄 待 Wave 1 |
| `bool.ToString` | C# 字面量 `"True"`/`"False"` | Rust `Display` 写 `"true"`/`"false"` | 🔄 待 Wave 1 |
| `Math.Sqrt` / `f64::sqrt` | C# extern → libm | Rust intrinsic → libm | ✅ 必须保留 builtin |
| `File.ReadAllText` / `fs::read_to_string` | C# extern → syscall | Rust extern → syscall | ✅ 必须保留 builtin |
| `Unsafe.As` / `core::intrinsics::*` | C# JIT intrinsic | Rust compiler intrinsic | ✅ z42 对应 array_get / array_set IR |

**两边都把 CoreLib / std 里的 "primitive 集合" 卡在 ~50 个量级**：
- .NET CoreLib 的 `[Intrinsic]` / `[MethodImpl(MethodImplOptions.InternalCall)]` 标注约 200 项，
  但去掉 SIMD / Span / Unsafe 等专家通道后，**普通用户能感知**的 primitive 不到 80 个。
- Rust `core::intrinsics` 约 150 个，但其中超过一半是数值算术 / 浮点边界，对应 z42 的 IR 指令而非 builtin。

z42 当前 ~80 个 builtin 偏多，目标是收敛到 **~40-50 个**，与 BCL/Rust 比例对齐。

### 与 Script-First 现有表述的关系

本节是 [philosophy.md §8](../philosophy.md) "Script-First, Performance-Driven Specialization" 与
[stdlib.md "Per-Package Extern Budget"](overview.md) 的**执行清单化**：
前者讲"什么时候**新增** extern"，本节讲"**已有** extern 应当持续审计、按 BCL/Rust 标杆消减"。

### 审计与执行流程

1. **现存 extern 清单**：维护在 [src/libraries/README.md "Extern 现状审计表"](../../src/libraries/README.md)
2. **每次 stdlib 改动起手**：先看审计表，第一选择是消减一个 extern 而不是新增
3. **新增 extern**：必须在 PR 描述里回答"为什么 BCL/Rust 把它当 primitive？"，回答不出 → 拒绝
