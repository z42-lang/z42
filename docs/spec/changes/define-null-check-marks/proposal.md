# Proposal: 引用类型的空检查标记 `?`

## Why

`enforce-value-type-non-null` 堵住了值类型那半。**引用类型这半今天零约束**：

```z42
IPAddress? a = IPAddress.TryParse("bad");
print(a.ToString());        // 编译通过，运行期崩
```

`?` 在引用类型上被 `SymbolTable.z42:590` 擦除，不产生任何义务。
`IPAddress.TryParse` / `ProcessHandle.TryWait` 这些**正确标注了**可空返回值的 API，
标注对调用方没有任何约束力——标了等于没标。

### 诚实的账

**追溯价值低。** 全历史提 null 的提交只有 18 个，绝大多数是 GC / 运行期的
（park 窗口、GC 根缺口、stale-mark 竞态、lazy loader skew）——**流分析百分之百看不见**。
且全仓已有 974 处手写 null 检查，说明写码纪律本来就在。

**前瞻价值是真的。** 让库作者能表达「这里可能没有」并被编译器强制，
是今天完全缺失的能力。`?` 目前是个看起来存在、实际为零的特性。

⇒ 本变更定位为**空检查 lint**，**不叫「空安全」**。它不健全（未标的东西不强制），
叫安全会让人像信 Kotlin 一样信它，然后被 NPE 打脸——那比没有更糟。

### 逃逸口是设计缺陷，不是便利

- `??`（生产 69 处）—— 把「没有」静默变成一个默认值，下游无法区分
- `?.`（z42 代码里实际仅 3 处，均在 `tests/control_flow/null_conditional.z42`）——
  **检查了，但把 null 往下游传**，把 bug 挪到离现场更远的地方。这个没法修好，它的语义本身就是"悄悄跳过"

C# 的 `!` 更糟：纯编译期擦除，运行期什么都不做。
Rust 的对应物 `unwrap()` / `expect(msg)` **是真的 panic** —— 差别就在检查。

## What Changes

### `?` 的含义：编译期强制检查标记

> **`T?` 不是「这个可能为 null」（引用类型本来都可能），而是「编译器请在这里强制检查」。**

| | |
|---|---|
| 适用 | **仅引用类型**（值类型由 `enforce-value-type-non-null` 禁止） |
| 位置 | 形参 / 返回类型 / 字段。**局部变量不标**——其可空性由流分析推导 |
| 缺席的含义 | **不强制**（**不是**保证非空） |
| 不进入 | 类型身份、可赋性、重载决议、泛型代换、ABI、运行期表示 |
| 传播 | 形参标 `?` → 被调方须检查；返回标 `?` → 调用方须检查。**义务不传递**，链在第一个处理点停 |

「缺席 = 不强制」是消除传染的关键。C# 的痛来自「缺席 = 保证非空」：
改一个签名，调用方被迫继续往上标。这里给签名**加** `?` 只在直接调用点产生新诊断；
**去掉** `?` 永远不破坏任何人。

**红利**：`List<string>` 与 `List<string?>` 是同一个类型 —— 没有变型问题、
没有无约束 `T?` 的含义地狱、`default(T)` 不用解释；`new string[10]` 的元素就是 null，
本变更不声称保证，因此**不撒谎**（C# NRT 在这里公开说谎）。

### 流分析

- **义务点**：`x.M()` / `x.F` / `x[i]` / 算术与比较（`==`/`!=` null 除外）/ `foreach (var y in x)` / `throw x` / `lock x` / `using x`
- **唯一传播点**：`return x` 且返回类型未标 `?` —— 三条出路：检查掉、给返回类型加 `?`（自愿传播）、`Expect`
- **窄化**：`if (x == null) { return/throw/continue; }` 之后 / `if (x != null) {}` 内部 / `x is T` / `&&`、`||` 短路 / 三目 / 赋非空值
- **字段不窄化**：标 `?` 的字段必须先快照到局部（理由见 design §D4）

### ~~反向推导~~ —— ❌ 不做（User 裁决 2026-09-23）

> 原文：函数体里有 `return null;` 而返回类型未标 `?` → 诊断，让标注几乎自动完成。

实测全仓 **464 处**命中（174 个方法），形态全是正当的「没有」语义 —— 守卫早返回、
哨兵迭代、反射查找，无一处写错。更根本的是它与本文的支点冲突：既然「**缺席 = 不强制**
（不是保证非空）」，返回 null 而不标 `?` 就是合法表达（「调用方不必查」）；判成错等于
把缺席重定义成「保证非空」。它还会让下面版本语义表的「去 `?` 永远安全」失效。
完整论证见 design §D8。

**覆盖率改由人工标注 stdlib 的「可能没有」API 解决** —— opt-in 的机制只能靠作者 opt in。

### `Expect("理由")`：唯一逃逸口

```z42
var cfg = this._cache.Expect("LoadConfig 在构造器里已填过 _cache");
```

- 编译器 intrinsic，是**唯一**允许作用在 MaybeNull 值上的成员访问
- **消息参数必须给，且必须是字符串字面量** —— 没有无参版本。逼你写下「为什么你认为它非空」，
  这正是 C# 的 `!` 从来没有的东西
- 运行期**真检查**：为 null 就抛，消息 = 你写的理由 + 源位置
- 结果的事实是 NotNull，义务解除
- 可 grep、可计数、可在 review 里数

### 砍掉 `??` 与 `?.`

| | 处数 | 迁移 |
|---|---|---|
| `??` | 生产 69（compiler 47 / stdlib 22）+ 测试 42 | 改写为 if/局部变量 |
| `?.` | z42 代码里实际 3 处（均在 `tests/control_flow/null_conditional.z42`） | 删测试 + 删脱糖路径 |

> `?.` 的其余 grep 命中是注释，或 android/ios appbuilder 里**生成的 Kotlin/Swift 源串**
> （`dest.parentFile?.mkdirs()`），与 z42 语法无关，不动。

### override / 接口一致性

- 返回类型：可**去** `?`，不可**加**（否则经基类调用的人没有义务 → 洞）
- 形参：可**加** `?`，不可**去**

### 版本语义（残留传染的诚实记账）

| 改动 | 对调用方 | 对实现方 |
|---|---|---|
| 返回类型**加** `?` | 💥 破坏 | 安全 |
| 返回类型**去** `?` | 安全 | 安全 |
| 形参**加** `?` | 安全 | 💥 破坏 |
| 形参**去** `?` | 安全 | 安全 |

比 C# 好在**破坏不传播**：调用点加一次检查即可，不会因"我的返回值也没标"被迫继续往上标。
但它确实是破坏性变更，写进版本规则。

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/compiler/z42c.semantics/src/`（新增）`NullFlow.z42` 等 | 空值事实格 + 结构化数据流 + 窄化/失效规则 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | `:590` 引用类型 `?` 不再擦除，转为标记位（不进类型身份） |
| `src/compiler/z42c.semantics/src/Symbol.z42` / `BinaryTypeTable.z42` | 形参/返回/字段的 `?` 标记随签名跨包携带（`TsigTypeName` 已拼 `?`，`StubEmitter` 已拼回） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | `:385` 删 `?.` 脱糖；义务点检查 |
| `src/compiler/z42c.semantics/src/OperatorEmitter.z42` | `:231` 删 `??` 发射 |
| `src/libraries/z42c.syntax/src/Lexer.z42` / `ExprParser.z42` | 删 `??` / `?.` 词法与语法 |
| `src/libraries/z42c.syntax/src/Ast.z42` | `:227` 删 `?.` 节点 |
| `src/compiler/z42c.semantics/src/DiagnosticCodes.z42` | 新错误码（design §错误码） |
| `src/compiler/z42c.semantics/src/*` | `Expect` intrinsic（绑定 + 发射 + 运行期检查） |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | override / 接口的 `?` 一致性 |
| 全仓 `.z42`（69 + 42 处） | `??` 迁移 |
| `src/tests/control_flow/null_conditional.z42` / `null_coalesce.z42` | 删除 / 改写 |
| `docs/reference/src/language/*` | 空检查规则 + `Expect` + 版本语义表 |

## Dependencies

- **`enforce-value-type-non-null`** —— 值类型那半必须先定，否则 `?` 的适用范围说不清
- **流分析设施** —— 语义层现无 DA、无窄化、无 CFG。建议先做 `add-definite-assignment`
  （误报率天然≈0，是验证引擎的理想第一刀），本变更复用其设施。
  若 DA 延后，本变更须自建设施，届时 DA 变得很便宜。
  **好消息**：老的 C# 宿主编译器有过 `FlowAnalyzer.cs`，可作参考（见 `simplify-ref-parameters` 的移植线索）。
- **没有 `goto`** ⇒ 不必建 CFG，按 `BoundStmt` 结构化递归 + join 规则即可

## Out of Scope

- 值类型可空性（前一个 change）
- 悲观档（所有引用类型都强制检查，不看标记）—— **明确不做**。实测全仓 49275 个解引用点、
  974 处现存检查，悲观档需新增 8000~19000 处守卫（代码量 +8%~19%），而历史证据不支持
- 运行期 `NullReferenceException` 的位置信息改善 —— 独立 change，与本变更正交且更便宜

## Open Questions

**Q1：标 `?` 的字段，窄化规则取哪条？**

| | 规则 | 问题 |
|---|---|---|
| **(a)（推荐）** | 不窄化，必须先快照到局部 | 每次多写一行 |
| (b) | 允许窄化，任何方法调用后失效 | **不可预测**：检查完顺手写句日志就要重新检查 |

(a) 的三条理由：属性会被读两次（`if (this.P != null) this.P.M()` 是两次 getter 调用，
返回值可以不同）；多线程下只有快照安全；永不出现「我明明检查过了」的困惑。

**摩擦有多大只能实测** —— tasks 里安排在引擎上线后、全仓开闸前做 A/B。

### ✅ Q1 已实测定稿（2026-09-22）：取 (a)

两条规则各实现一遍、各跑一次全仓（25 包 + 编译器自身；用探针把「本方法里被拿去
和 `null` 比过的字段」视同标了 `?`，诊断降级成 warning 以便跑完全仓；两次均
`cached: 0/` 全量重编）：

| 规则 | 命中 | 形态 |
|---|---|---|
| **(a) 强制快照** | **51** | 单一：就地检查后直接再读字段 |
| (b) 调用即失效 | 7 | **全部是「检查完插了一次无关调用」** |

**判据不是命中数，是那 7 处长什么样**——它们正是 (b) 那栏预言的「不可预测」：

- `z42.collections/LinkedList.z42:85` — `node.SetNext(this.head);` 的**下一行**
  `this.head.SetPrevious(node)` 才报。相邻两行、同一个表达式，一行合法一行报错；
  而那次调用只是**把字段读出去传给别人**，并没有写它。
- `z42.net/Http/HttpClient.z42:775` — 查完 `_cookieJar` 取了个时间戳
  （`HttpClient._unixNow()`），再用就要重查。字面意义上的「检查完写句日志就要重检」。
- `z42c.syntax/Decl.z42:455` — `while (i < this.ParseDiags.Count())` 的**条件里**合法、
  **体内** `this.ParseDiags.Get(i)` 报错。**同一条 `while` 语句内**两种待遇。

⇒ (b) 少报的 44 处，买回来的是这种「我明明检查过了，为什么这里报那里不报」。
(a) 的代价则是齐整的一行快照，且**因为标记是 opt-in，存量代码的实际迁移量是 0**
（全仓现有 `?` 字段数 = 0；那 51 处是「若把你已经在查空的字段全标上」的上界）。

**顺带量到的**：同一次探针跑出 **16 处 E0479**，全是同一个形态——**记忆化惰性初始化**
（`if (cache != null) { return cache; } cache = compute(); return cache;`，见
`Reflection/{FieldInfo,MethodInfo,ParameterInfo,PropertyInfo}`、`Type`、`ProcessHandle`、
`TcpClient`、`TlsClient`）。标了 `?` 的缓存字段直接 `return` 给未标的返回类型会被 E0479 拦，
修法同样是先快照（`var c = this.__cache; if (c != null) { return c; }`）。
⇒ **这是字段标记最常见的落地形态，reference 文档要把它当样例写出来。**
