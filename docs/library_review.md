# z42 标准库分析报告（对标 C# BCL）

> 生成日期：2026-08-30
> 范围：`src/libraries/` 全部 25 个用户 stdlib + 工具链库。
> 视角：zpkg 组织 / 实现合理性 / 性能，对标 C# BCL。
> 性质：**纯分析**，本文档不改动任何代码。

## 总体判断

stdlib 现状比预期好：

- **核心算法扎实** —— `List<T>` 自顶向下归并排序（stable, O(n log n)），`Dictionary<K,V>` 开放寻址
  + 存储 hash 短路 + 回填式删除（无 tombstone）。这两个不需要动。
- **广度够** —— Time 家族（DateTime/DateTimeOffset/TimeSpan/Stopwatch/TimeZone）、ValueTuple 2..8、
  StringBuilder、BigInt、Uri、lambda/委托（Func/Action/Predicate）都已落地。
- **近期性能整理是教科书级** —— #301–#305（`ToCharArray` 物化、JsonValue O(n²)→O(n)、List.Sort 归并、
  Dict.Remove 免重算 hash）。

真正的短板集中在一个**结构性主题**，其余多是低成本补齐：

> **两个协议族被定义了却完全悬空**（IEnumerable/IEnumerator、IComparer/IEqualityComparer），把 z42 卡在
> "每个具体类型各写各的"层级，进不了"面向序列/比较器抽象 + LINQ"的生态层。而大量高频小缺口
> （TryParse、IsNaN、String 补齐、Array 算法）**并未被语言特性阻塞**，只是还没人写。

---

## 一、zpkg 组织：基本健康，两处该动

**结论先行**：`z42.core` 作为"上帝包"（296 个唯一 native 符号，#307 后 io/net/threading 的 syscall 全上移
进来）是**刻意对齐 .NET CoreLib** 的选择，不是缺陷 —— List/Dict/File/Console/DateTime 在 BCL 也都在
CoreLib。prelude 体积已有实测证明非启动瓶颈（`+500KB≈+0.2ms`，启动地板是进程 + VM init）。
**不建议为"瘦身"做破坏性重排。**

比 BCL **更优**的一点：把每个程序都用的 List/Dict 放进恒加载 prelude、次级容器（Queue/Stack/LinkedList/
SortedSet/PriorityQueue）放 opt-in 的 `z42.collections` —— 在 lazy-load 模型下比 BCL 把整个
`System.Collections.Generic` 塞一个 assembly 更合理。唯一代价是命名空间 `Std.Collections` 跨两个 zpkg，
属可接受的认知成本。

### 依赖图概览

```
L0  z42.core (无声明依赖，隐式 prelude)
       Primitives Exceptions Protocols Reflection Delegates
       Collections(List/Dict/KVP) IO Time GC Native + 顶层松散文件

L1  仅依赖 core：  collections · text · encoding · random
    L1→L1 交叉：   uri→text · numerics→random · crypto→(encoding,numerics)
                   regex→collections【失效依赖，见 O1】

L2  io→(encoding,text) · io.binary→(io,encoding) · toml→io · json→(text,io)
    yaml→io · cli→(text,io) · diagnostics→io · threading→diagnostics · compression→io

L3  net→(io,encoding,random,crypto,threading,compression)  ← 依赖面最宽
    test→io

工具链库（命名空间 Z42.*，与用户 Std.* 混住同一目录/同一 flat dist）：
    z42c.core(无依赖) ← z42c.syntax
    z42.ir→(core,encoding,io,crypto) · z42.project→(core,io,toml) · z42.build→(core,io,project)
    z42.scripting→(core,io,ir,build,test,z42c.core,z42c.syntax,threading)
```

### 该动的两处（都小）

| # | 问题 | 证据 | 建议 | 代价 |
|---|------|------|------|------|
| **O1** | **z42.regex → z42.collections 死依赖** | toml 声明了，但 regex 源码里 Stack/Queue/… **零引用**（W1 把 List 上移 core 后遗留的死边）| 删掉 toml 那一行 | ≈0 |
| **O2** | **z42.io.binary 是依赖 io 的 3 文件碎包** | 只有 BinaryReader/Writer/Exception，用 io 的 Stream/MemoryStream，命名空间已同为 `Std.IO`。**违反 organization.md 自己的反例条款**（"跟现有包 80% 重合 → 合并"）。.NET 的 BinaryReader 本就在 System.IO 同一 assembly | 并入 z42.io，删该包 | 中小（改下游 using + workspace member）|

### 次要（文档层，非代码）

- `organization.md` 有几处 stale：line 166 说 z42.text 含 Regex，实际早独立成 `z42.regex` 包；
  line 3 称 core "~123 个文件"，实测非测试源文件约 96 个。
- "L1 只依赖 core"规则与现实的 `numerics→random`、`crypto→numerics` mesh 背离 —— 建议把规则降级为
  "stdlib 依赖图必须是 DAG（无环）+ 尽量浅"，承认 L1 mesh 的现实（接近 Rust std 各 module 互引）。
- **工具链库命名混乱**：`z42.ir/project/build`（命名空间 `Z42.*`，编译器内部件）顶着 `z42.*` 包名，
  从包名看像用户 stdlib，误导。建议统一到 `z42c.*` 前缀（与 `z42c.core`/`z42c.syntax` 对齐）。
  **但 rename 触及自举种子轴**（`_ensureBootstrapZ42Ir` 等按包名供种）+ workspace + 大量依赖声明，
  半径大 —— 宜作独立 change 走完整流程评估，而非顺手做。
- **core/IO "薄/厚"切分口径主观**：`Console/File/Directory/Environment/Path` 完整应用层类在 core，
  `FileStream/Stream/Process` 却留 z42.io，靠"逻辑很薄→并 core"的人为判断，易随后续改动漂移。
  建议在 `organization.md` 给出可判定口径（如"零依赖其他 io 类型 + <N 行 → core；依赖 Stream 家族 → io"）。

---

## 二、实现合理性 / API 面：核心问题在悬空的协议

### 🔴 结构性缺口（投资回报最高）

**1. IEnumerable/IEnumerator 零实现**
`List<T>` 连接口声明都没有（`class List<T> where T: IEquatable + IComparable`），foreach 走纯鸭子协议
（Count + `get_Item(int)`）。后果：**无 LINQ；泛型算法无法 `where T: IEnumerable<U>` 抽象消费序列；
Dictionary 不能直接 foreach**（只能 `.Keys()/.Entries()` 取快照数组，每次分配）。
blocker 不是 lambda（已可用），而是 **foreach codegen 未识别 IEnumerator 路径** —— 编译器 + stdlib 协同改动。

**2. IComparer/IEqualityComparer 完全悬空**
全仓库零引用。`List.Sort()` 只有无参重载写死走 `CompareTo`，**没有** `Sort(comparer)` / `Sort(Comparison)`；
Dictionary 无自定义相等 ctor。用户被迫把唯一排序逻辑硬编码进类型自身的 `CompareTo`。

**3. `List<T>` 过度约束**（`src/libraries/z42.core/src/Collections/List.z42:12`）
`where T: IEquatable<T> + IComparable<T>` 把"部分方法（Contains/Sort）的需求"错误提升为"整个类型的约束"：
**一个只想装载、从不排序的 DTO 列表都建不了**。C# `List<T>` 无约束（用 `EqualityComparer.Default` 运行时
兜底）。根因是缺 `EqualityComparer.Default` 机制。Dictionary 要求 `TKey: IEquatable<TKey>` 同类问题，半径较小。

### 🟡 高频痛点（大多不被语言阻塞，可低成本补）

- **TryParse 缺失** —— 所有 Parse 抛异常，无 `TryParse`。但 `IPAddress.TryParse(s) → IPAddress?` 已证明
  "返回 nullable 替代 out 参数"可行，所以 `Int32.TryParse(s) → int?` **现在就能建**。
- **Double.IsNaN / IsInfinity 缺失** —— 用户判 NaN 只能自己写 `x != x`。纯脚本可加。
- **String 缺** PadLeft/PadRight、IndexOf(char)、Split(char[])、Trim(char)、LastIndexOf、Insert/Remove ——
  全不被阻塞，照现有 char[]-脚本范式就能补。
- **Array 静态算法几乎全空** —— Sort/IndexOf/Copy/Fill/Reverse 都无，只有反射壳方法。
- **StringBuilder 只有 Append(object)** —— 传字符串也走 Convert.ToString 虚分发；传 char 还要装箱。
  缺 Append(char)/Append(string) 重载（也是下面性能问题 P2 的根因）。
- **Convert 仅 4 个方法**（ToInt32/ToInt64/ToDouble/ToString），缺 ToBoolean/ToByte/ToInt16/ToSingle/ToChar。

### 🟢 有前置依赖，需先解锁

- `MaxValue`/`MinValue` 常量全缺（MemoryStream 里被迫写魔数 2147483647）—— 需静态常量字段访问，
  可能语言级前置（与 `Path.Separator` 同 blocker）。
- `Format` 无格式说明符 `{0:F2}` / `{0,10}` —— 需 IFormattable 接入（协议已定义但悬空）。
- `Sort(IComparer<T>)` 接口版 —— 被"泛型接口 TypeArgs 分发"的 TypeChecker bug 阻塞（README backlog 有记）。
  **但委托版 `Sort(Comparison<T>)` 现在就能做，绕开该 bug。**

### 缺失的 BCL 类型（次要）

| 类型 | z42 现状 | 前置依赖 |
|------|---------|---------|
| Guid | 无 | 无（可基于 z42.crypto SecureRandom 建）|
| Version | 无 | 无 |
| Span\<T> / Memory\<T> | 无用户面（仅编译器内部 z42c.core/Span.z42）| 需语言级 ref/生命周期设计（重）|
| Nullable\<T> 显式类型 | 语言级 `T?` 已覆盖多数场景，缺 `.HasValue`/`.Value` 显式 API | 显式类型留待系统设计 |
| LINQ | 不存在 | 阻塞于 IEnumerable 接入 |
| Uri / BigInteger | ✅ 已有（z42.uri / z42.numerics）| — |

---

## 三、性能：近期整理很扎实，剩三处

**已健康**：encoding 三件套（Hex/Base64/Base32）、JsonWriter、所有 number/plain-scalar 扫描都是
"预分配 buffer + 一次填充"的正确写法。BigInt 算术主循环（limbs/divMod/Karatsuba）也没问题。

剩下三处（按严重度）：

| # | 问题 | 位置 | 量级 | 修法 |
|---|------|------|------|------|
| **P1** | **3 个解析器的引号字符串逐字符 Append** | json ParseString / toml ParseBasicString·ParseLiteralString·多行 / yaml 双引号·单引号，共 6 处 | 每字符一次堆分配（O(N) 分配）| **照搬同仓库 `JsonWriter.z42:102 QuoteString` 已有的 run-flush 模式**：普通字符只推游标，遇引号/转义才 `Substring` 整段冲刷。分配降到 O(转义数)。写入侧优化了、解析侧漏了，不对称 |
| **P2** | **StringBuilder 无 char 缓冲**（P1 的根因）| StringBuilder.z42 | Append(char) = 一个堆字符串 + 数组槽 | 增设 char[] 缓冲 + `Append(char)`（`buf[_len++]=c`，2× grow）。独立演进项 |
| **P3** | **BigInt 十进制化 `s = s + 片段`** | BigInt.z42 ToString / ToBase / ToHex | 位数的 O(n²)（ToBase 每位一拼，每次 StrConcatInstr 拷贝整串）| 预分配 char[] 尾填 + 一次 FromChars。divmod 主循环已用对 buffer，唯独最后字符串组装退化 |

**次要**：`Strings.Repeat/PadLeft/PadRight` 从源串取字符走 `CharAt`（builtin）而非 `ToCharArray` 一次
（char[] 走 ArrayGet 快 ~9×）。低风险机械优化。

**补充观察**：StringBuilder `ToString()` 内层 `s.CharAt(j)` 对非 ASCII 串可能是 O(index) 的 UTF-8 walk
→ 整体退化。改用每 part 一次 `ToCharArray()` 再批量拷贝更稳（与 P2 同向）。

---

## 优先级建议

排"接下来做什么"：

1. **`List.Sort(Comparison<T>)` 委托版排序** —— lambda 已就绪，绕开接口分发 bug，**现在就能做**，
   立即消除"排序准则写死"痛点。投入小、收益立现。
2. **性能 P1（解析器 run-flush）** —— 零设计风险，模式已在 JsonWriter 验证，直接复制到 6 处。
3. **一批纯脚本小补齐** —— TryParse→`T?`、Double.IsNaN、String 的 PadLeft/IndexOf(char)/Split(char[])、
   Array.Sort/IndexOf。逐条低成本，显著提升日常可用性。
4. **组织清理 O1+O2**（删 regex 死依赖、并 io.binary）—— 小改，干净。
5. **（大工程，需评估）IEnumerable 端到端接入** —— 投资回报最高但要编译器 + stdlib 协同（foreach codegen
   升级 + List/Dict implements）。这是解锁 LINQ 与统一迭代的唯一路径，值得单独立项讨论。

**关键结论**：z42 stdlib 的**广度**其实不错。真正的短板是**两个悬空的协议族（IEnumerable、
IComparer/IEqualityComparer）导致的结构性断层** —— 它们让 z42 停在"具体类型各写各的"，无法进入
"面向序列/比较器抽象编程 + LINQ"的生态层级。而大量高频小缺口**并未被语言特性阻塞**，只是"还没人写"，
可低成本快速补齐。
