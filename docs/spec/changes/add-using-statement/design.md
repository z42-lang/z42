# Design: `using` 语句

> 状态：🔵 DRAFT 待审批 ｜ 前置：[proposal.md](proposal.md)

## 已核实的落位事实（2026-09-26 实读，不必重查）

| # | 事实 | 依据 |
|---|---|---|
| L1 | `IDisposable` **存在**且跨包可用：`z42.core/src/Protocols/IDisposable.z42:8`；跨包实现者 `z42.io/src/ProcessHandle.z42:13` 真的写着 `: IDisposable` | grep |
| L2 | 语句位的 `using` 今天被**无条件**拦下报 `E0209 DeclarationInStatement`（`Parser.ParseStatement` 开头那段，判据只看 token 是不是 `Using`）⇒ 本批必须把它改成「能区分 import 与语句」 | 实读 |
| L3 | 批 2 之后加一条语句 = `StmtRules.IsKeywordStmt` 加一项 + `StmtRules.Feature` 填名字 + `Parser._parseKeywordStmt` 加一条分派 + `StmtStep` 顺序表无需改（`KeywordRules` 已在第 3 位）| #847 |
| L4 | `Dispose` / `IDisposable` / `DisposableFrom` 常量已在批 1 收进 `z42c.core/src/ProtocolNames.z42` | #841 |
| L5 | 诊断码 main 到 **E0497**；在飞 #845 / #850 / #851 均不占新码 ⇒ E0498/E0499 可用（**合并前须逐个在飞分支重查**）| `gh pr diff` |
| L6 | 分两批是**硬纪律**（support 与 use 跨两个 nightly），违反即自举死锁 | `bootstrap-seed.md:122-129` |
| L8 | 🔴 **E0209 那段拦截在 `_steps` 循环之外**（`Parser.z42:295-314`，`:315` 才进循环）⇒ 它**优先于表**命中，必须先在那里开口，否则 `using (` 永远先撞 E0209 | 实读 |
| L9 | foreach 的合成模板：`StmtBinder._bindForeachEnumerable`（`StmtBinder.z42:83-114`）**只造 AST 再喂回 `_bindStmt`**，零新 Bound 节点、零新 emitter；`TryCatchStmt(body, [], 0, hasFinally:true, dispose, sp)`；临时名用 `"__fe_e_" + span.Start` 唯一化（防嵌套撞名）。不可 dispose 时**连 try/finally 一起省掉** | 实读 |
| L10 | 形状判据现成：`ForeachProtocol._hasDispose`（`ForeachProtocol.z42:136-154`）—— 接口交 `InterfaceClosure.FindMethod`、类沿 `BaseName` 链、`Z42InstantiatedType` 取 `.Def`，**判据不足一律保守 true** | 实读 |
| L11 | `Dispose` / `IDisposable` / `Disposable` / `DisposableFrom` 四个常量都已在 `ProtocolNames`（`:53-59,83-85`）⇒ **本批不需要新增常量** | 实读 |
| L12 | AST 是 `TryCatchStmt`（`Stmt.z42:219`），Bound 叫 **`BoundTry`**（不叫 `BoundTryStmt`，`BoundStmt.z42:117`）；finally 体被发**三份副本**（内联退出 / 合成 catch-all rethrow / fall-through），栈由 `_pushFinally`/`_emitPendingFinallys` 管（`StmtEmitter.z42:373-400`）| 实读 |

## 🔴 L7（本批最重要的一条实测）：名义 `IDisposable` 今天几乎没人声明

全仓声明了 `void Dispose()` 的类型 **24 个**，其中名义写了 `: IDisposable` 的只有 **5 个**
（`z42.core` 的 `Disposable` + 三个 Multicast 订阅 + `z42.io/ProcessHandle`）。
**z42 自己全部真实资源类型都在另外那 19 个里**：

```
TcpClient · TcpListener · UdpClient · TlsClient · WebSocketClient ·
WebSocketConnection · HttpClient · HttpServer · TextReader · TextWriter   （基表全为空）
```

⇒ **若 `using` 取 C# 的纯名义口径，它在落地当天对 z42 自己的每一个资源类型都不可用。**

⭐ 更糟的是**注释在撒谎**（这一程反复出现的形状）：
- `z42.net/src/TcpClient.z42:18`：「z42 没有正式的 IDisposable protocol，但 stdlib 约定带…」
- `z42.io/src/Stream.z42:28`：「z42 has no `IDisposable` yet; use Close()」

两句都是假的 —— 接口存在，且**同一个包里的 `ProcessHandle` 正在实现它**。
（合理猜测但**未验证**：这些类型当年放弃声明，是撞上了批 A–F 修掉的那批跨包接口缺陷
#778/#779/#784/#788/#789/#792；那些现已修好 ⇒ 现在大概能声明得出来。**IMPL 时实测一次**。）

## 🔴 三个会红的现存门 / 假话（本批必须一起处理，漏一个就是「改了但没改完」）

| # | 位置 | 性质 |
|---|---|---|
| G1 | `z42c.syntax/tests/stmt.z42:158-190` | **四条活的 E0209 门**（函数体里 `using Std.Toml;` 必须恰好一条 E0209 且零级联）。本批必须**扩充而不是删掉**它们：新增正例 `using (r) { }` 不报 E0209，原四条保持 |
| G2 | `z42c.core/tests/features.z42:23,32` | **一条活的负例断言** `Assert.Equal(false, f.IsEnabled("using_stmt"))` + 注释「z42 没有 using 语句」。若特性名取 `using_stmt`（D3-6 建议），这条会**直接反向** —— 这正是「幻影名以真名回来」的判别力落点 |
| G3 | reference **三处明文否认**：`delegates-events.md:338`「z42 **没有** C# 的 `using (…) { }` 语句」/ `process.md:352`「**没有 `using (...)` 语句**」/ `io-stream.md:374`「**不实现 `Std.IDisposable`，也没有 `using (...)` 语句**」 | 加了语句后三句全是假话，必须同批改。⭐ 第三条还暴露一笔**库侧账**：`z42.io` 的 Stream 族压根没实现 `IDisposable`、只有 `Close()`（与 L7 同族，独立记账） |

顺手一条：`error-codes.md:90` 给 E0209 标的发射点 `Parser.z42:246` **行号已过期**（真实是 `:308`）。

## 🔴 「五条退出路径」这张清单在本仓**不存在**

- 文档口径 `iteration.md:98` 只列**四**条（正常结束 / `break` / `return` / 抛异常）—— **漏了 `continue`**；
- 测试分散三处：`finally_nonlocal_exit`（return / break / 返回值不被 finally 污染）、`try_finally`、
  `finally_propagation`；`continue` 经 finally 只在注释里声明过，**没有夹具**。

⇒ 本批的 I5 门是**新建**的（不是补抄现成的）：`using` 五条退出路径各一条夹具。
顺带把 `iteration.md:98` 补成五条（foreach 与 `using` 共用同一套保证）。

## D3-1（**本批的核心裁决**）：可 dispose 的判定用名义、形状，还是有序回落链？

D3（批 1 已裁）说协议匹配是「每协议各自配置的**有序策略链**」，并指出 C# 自己的 `using`
= `[nominal(IDisposable), shape(仅 ref struct)]`。本批要把这条落成具体判据。

| 选项 | 行为 | 代价 |
|---|---|---|
| **甲 —— 纯名义** | 只认 `: IDisposable` | 🔴 落地当天对 z42 **全部**资源类型不可用（L7）；必须先给那 10 个类型补基表（动 stdlib TSIG 字节，独立 PR、可能再撞跨包接口面），`using` 要等它。🔴 **还有一个 prelude 缺口**：`BuiltinTypeDefs.z42:60-64` 登记的 `IEnumerator` **没有 base `IDisposable`**（源声明是 `IEnumerator<T> : IDisposable`）⇒ 名义判据在冷启动路径上可能答错，得先核实 |
| **乙 —— 纯形状** | 只要有 `Dispose()` 方法 | ✅ 当天可用；与 foreach 今天的判定一致（foreach 也是形状判定）；🔴 与 C# 偏离，且「声明 `: IDisposable`」变得毫无意义——名义实现拿不到任何好处 |
| **丙 —— 有序回落链（推荐）** | `[nominal(IDisposable), shape(Dispose())]`：先问名义，不成立再问形状 | ✅ 当天对 19 个形状类型全可用；✅ 名义仍是**首选路径**（声明它有意义、且诊断能说「建议声明 `: IDisposable`」）；✅ 形态正是 D3 裁决的「有序策略链」，与 `ForeachProtocol.Resolve` 同款；🔴 两条路要各有夹具，判别力成本翻倍 |

> **我的推荐：丙。** 理由：甲把一个纯语法特性绑上了一次 stdlib 基表变更（那是另一条线的工作，
> 且会把本批从「零字节风险」变成「动 stdlib TSIG」）；乙则让名义声明彻底失去意义 ——
> 而 z42 明确有 `IDisposable` 这个接口，让它对 `using` 不起作用等于宣布它是装饰。
> 丙还留了一条**将来收紧**的路：等 stdlib 资源类型都补上基表后，可以把形状那一档降级成
> 警告、再降级成错误，而不必回头改语法。

⚠️ **不论选哪个，判据都必须问「继承面」而不是直接成员表** —— #827 的教训：
`MethodsOf` 给的是类型自己声明的那张表，接口不含父接口成员、类不含基类成员；
foreach 的 `Dispose` 条件化第一版正是栽在这里（响亮错误变成**悄悄不释放资源**）。
本批直接复用 `ForeachProtocol` 那条判据（**同源**，不另写一份）。

## D3-2：三种形态本批做几种

D4（批 1 已裁）：`using (r) { }` + `using var r = ...;` 两种都要。还剩一个 C# 形态要裁：

| 形态 | 歧义？ |
|---|---|
| `using (expr) { … }` | ✅ 无歧义（`using` 后紧跟 `(`）|
| `using (T v = expr) { … }` | ✅ 无歧义（同上，判别在括号内）|
| `using var v = expr;` | ✅ 无歧义（`using` 后紧跟 **`var` 关键字 token**）|
| **`using T v = expr;`**（显式类型的简化形态）| 🔴 **与 import 真歧义**：`using Stream s = …;` vs `using Foo.Bar;` / `using X = Y;` —— 都是 `using` + 标识符。要认它必须做「跳过类型 → 见标识符 → 见 `=`」的前瞻 |

⇒ **建议本批不做第四种**（`using T v = e;`），只做前三种：前三种的判别都只看 `using` 后**一个** token，
零前瞻、零歧义。第四种要的前瞻与 `_isVarDeclStart` 那套同构，将来想加时是纯增量。

## D3-3：`using var` 的作用域与释放顺序

C# 的 `using var` = 「到所在块末尾」，多个时**逆序**释放（后获取的先释放）。
建议照抄：`using var a = …; using var b = …;` ⇒ 退出时先 `b.Dispose()` 再 `a.Dispose()`。
落法 = 降糖成嵌套的 try/finally（外层 a、内层 b），**逆序天然成立**，不需要额外机制。
⚠️ 需要一条门：两个 `using var` + 记录释放顺序的夹具，断言顺序是 `b, a`（不是 `a, b`）。

## D3-4：语句位 `using` 的消歧（L2 的改法）

今天那段拦截（报 E0209）的判据只看 token 是不是 `Using`。改成：

```
using 后的下一个 token：
  `(`   ⇒ using 语句（形态 1/2）
  `var` ⇒ using var 语句（形态 3）
  其它  ⇒ 仍是「放错位置的 import」⇒ 照旧 E0209
```

⚠️ **门要成对**：① `using (r) { }` 在方法体里必须**编得过**；② `using Foo.Bar;` 在方法体里
必须**仍报 E0209**（判别力：只验①的话，等于把拦截整个删掉也能绿）。

## D3-5：`null` 目标与新诊断码

- **`null` 目标**：C# 是「`null` 则跳过 `Dispose`」。建议照抄（降糖里加一次 null 判）——
  否则 `using (MaybeNull())` 会在 finally 里 NRE，把原始异常盖掉。
- **不可 dispose 的类型**：报新码。建议 **E0498**「`using` 的目标类型没有 `Dispose()`」，
  消息里带「若它本该可释放，声明 `: IDisposable`」这条建议（让名义路径被推荐）。
- `using var` 缺初始化器（`using var v;`）：复用既有的「局部声明必须有初始化器」诊断（IMPL 时核实是哪个码），
  不新占号。

## D3-6：特性名

建议 **`using_stmt`** —— 让 #783 删掉的那个**幻影名以真名回来**（那张表当年宣告它存在而实际没有，
是「三年无人发现的假话」里的一条）。按批 2 立的纪律，新增名字必须同时有
「parser 真的消费它」+「一条会红的门」，本批两者都给。
⇒ `Phase1Profile` 从 15 个名字变 16 个；真能关得掉东西的从 5 个变 **6 个**（死旋钮仍 10 个）。

## 不变量与风险

| # | 事项 |
|---|---|
| I1 | **零新 IR 指令、零格式 bump**：降糖全用既有 `try/finally` + 调用 + null 判 |
| I2 | **不需要 bump `CompilerFingerprint`**：`using` 语句此前**编不过**（报 E0209）⇒ 不存在「哈希不变而结果变」的缓存条目。这是 version-bumping.md 那条判据的正面例子 |
| I3 | support 阶段**编译器自身/stdlib/xtask 一处都不用** `using`（L6 的死锁纪律）⇒ 判据 = `xtask test bootstrap` 必须绿 |
| I4 | 语句位拦截改动**不得**让 `using Foo.Bar;`（放错位置的 import）静默通过 —— 门要成对（D3-4）|
| I5 | finally 覆盖五条退出路径这件事由既有 try/finally 保证，但要有夹具**逐条验**（return / break / continue / throw / 正常落出各一条，看 `Dispose` 是否都被调到）|
