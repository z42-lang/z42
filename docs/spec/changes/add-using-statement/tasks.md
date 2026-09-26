# Tasks: `using` 语句（批 3 = support）

> 状态：🔵 DRAFT 待审批 ｜ 前置：[design.md](design.md)
> ⚠️ **本文件只覆盖批 3（support）**。批 4（在 z42c/stdlib/示例里**使用** `using`）必须等
> 一个 nightly 发布之后另立 PR —— `bootstrap-seed.md:122-129`，违反即自举死锁。

## T0 —— 开工核查

- [ ] `gh pr list` 重查在飞 PR；**merge 前紧邻再查一次**
- [ ] 诊断码：E0498 是否仍空 —— 扫全源**不够**，还要 `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`
- [ ] 取**同命令**字节基线：`xtask build stdlib` → 25 包 sha256
      （⚠️ 别用 `build all` 取：它与 `build stdlib` 对 `z42.core` 产出不同字节，批 2 实测）
- [ ] **实测 L7 的猜测**：给 `z42.io/TextReader`（或 `TcpClient`）临时加 `: IDisposable` 看能不能编过
      ⇒ 坐实「当年放弃声明是撞了已修掉的跨包接口缺陷」还是「今天仍编不过」。**结论写回 design L7**
      （这决定 D3-1 选甲时的代价是「一条小 PR」还是「又一串缺陷」）
- [ ] **实测 prelude 缺口**：`BuiltinTypeDefs` 的 `IEnumerator` 无 base `IDisposable`，在冷启动路径上
      会不会让名义判据答错（D3-1 选甲/丙时是风险点）

## T1 —— 语法：`using` 进语句表（经机制，非新增 if 分支）

- [ ] `StmtRules.IsKeywordStmt` 加 `TokenKind.Using`
- [ ] `FeatureNames` 加 `UsingStmt`；`StmtRules.Feature(Using)` 返回它
- [ ] `LanguageFeatures.Phase1Profile()` 登记新名字（15 → 16）；抬头「真能关得掉的 5 个」→ **6 个**
- [ ] 🔴 **G2 反向**：`z42c.core/tests/features.z42:32` 的 `IsEnabled("using_stmt") == false` 断言改成 `true`
      并改注释 —— 那条墓志铭是 #783 留下的，现在幻影名以真名回来
- [ ] `Parser._parseKeywordStmt` 加分派 → `StmtParser._parseUsing()`

## T2 —— 🔴 E0209 拦截开口（L8：它在 `_steps` 之外、优先于表）

- [ ] 判据改为看 `using` 后**一个** token：`(` 或 `var` ⇒ 交给语句路径；其它 ⇒ 照旧 E0209
- [ ] **G1 扩充而非替换**：`tests/stmt.z42:158-190` 原四条 E0209 门全部保留，新增
      ① `using (r) { }` 在方法体里零诊断（正例）② `using var r = e;` 零诊断
      ⚠️ 判别力：只加正例等于把拦截删掉也能绿 ⇒ 原四条负例必须仍在
- [ ] 顺手修 `error-codes.md:90` 里 E0209 的过期行号（`Parser.z42:246` → 实际发射点）

## T3 —— 解析：三种形态（D3-2 建议只做前三种）

- [ ] `using (expr) { … }`
- [ ] `using (T v = expr) { … }`
- [ ] `using var v = expr;`（作用域到所在块末尾）
- [ ] AST：**不新增节点**，复用既有 `BlockStmt` / `VarDeclStmt` / `TryCatchStmt`（照 L9 的 foreach 模板）
- [ ] 缺初始化器 / 缺 `)` / 缺块 各一条恢复路径（别级联）

## T4 —— 降糖（binder 侧，照 `StmtBinder._bindForeachEnumerable` 的路子）

- [ ] `using (E) { B }` → `{ var __u_N = E; try { B } finally { if (__u_N != null) __u_N.Dispose(); } }`
      （临时名照 L9 用 `"__u_" + span.Start` 唯一化，防嵌套撞名）
- [ ] 只造 AST 再喂回 `_bindStmt` —— **零新 Bound 节点、零新 emitter、零新 IR 指令**
- [ ] `null` 判（D3-5）：C# 口径「null 则跳过 Dispose」
- [ ] 多个 `using var` ⇒ 嵌套 try/finally ⇒ **逆序释放天然成立**（D3-3）
- [ ] 判定「可 dispose 吗」：**复用 `ForeachProtocol` 那条判据**（D3-1 定的链），
      🔴 **必须问继承面**（#827 教训：查直接成员表会让继承来的 `Dispose` 被静默跳过）

## T5 —— 门（判别力，每条都要有「红的理由对不对」那一层）

- [ ] **五条退出路径各一条夹具**（正常落出 / return / break / continue / throw）—— 这张清单本仓
      **原先不存在**，`continue` 经 finally 此前只在注释里声明过
- [ ] **特性门**：`[syntax] using_stmt = false` ⇒ 用 `using` 必须报 E0301；不写 `[syntax]` 必须编得过
      （模板 = `scripts/test/xtask_test_incremental.z42` 的 `_syntaxKnobTakesEffect` 四判据）
- [ ] **消歧门成对**（T2）：`using (r) { }` 过 + `using Foo.Bar;` 仍 E0209
- [ ] **不可 dispose 的类型** ⇒ E0498；`Dispose` **继承**而来的类型 ⇒ 必须仍可用（两条形状：
      父接口继承 + 基类继承，这正是 #823 栽过的两种）
- [ ] **释放顺序门**：两个 `using var` + 记录顺序，断言 `b, a`
- [ ] `using` 嵌套 `using`（临时名唯一化真的有效）

## T6 —— 文档（G3：三处明文否认必须同批改）

- [ ] 新建 reference 页（或并入 `statements`/`exceptions` 相关页）讲 `using` 三形态 + 降糖 + 五条退出路径
- [ ] `delegates-events.md:338`「z42 没有 C# 的 `using (…) { }` 语句」→ 改成 scoped 订阅的新写法
- [ ] `process.md:352`「没有 `using (...)` 语句」→ 改
- [ ] `io-stream.md:374`「不实现 `Std.IDisposable`，也没有 `using (...)` 语句」→ 改；
      **顺带记一笔库侧账**：Stream 族没实现 `IDisposable`、只有 `Close()`（与 L7 同族，独立立项）
- [ ] `iteration.md:98` 的四条退出路径补成**五条**（漏了 `continue`）
- [ ] `syntax.md:37-49`（`using` 指令一节）旁边落新语句的歧义分析
- [ ] `z42-toml.md` 的 `[syntax]` 名单加 `using_stmt`；三处「真能关得掉的 5 个」→ 6 个
- [ ] `roadmap.md` / `syntax-customization.md`：`using` 作为「第一个经机制落地的新语法」记一行
- [ ] `binder-hierarchy.md:165-166` 那句「当 Phase 3 落地 `using Foo = Bar;` 语法时引入」**已过期**
      （类型别名今天已实现）⇒ 顺手订正

## V —— 验收

| # | 判据 | 怎么验「红的理由对」 |
|---|---|---|
| V1 | 字节：只有被改源码的包变（`z42c.syntax` / `z42c.semantics` / `z42c.core`）；**stdlib 25 包不变**（support 阶段没人用 `using`）| 不为零就逐 commit 二分 |
| V2 | 五条退出路径夹具全绿 | 把 `finally` 那步临时删掉 ⇒ 五条必须全红 |
| V3 | 特性门：关掉 ⇒ E0301；不写 ⇒ 编得过 | 阳性对照必须先绿 |
| V4 | 消歧：`using Foo.Bar;` 在方法体里**仍** E0209 | 把拦截整个删掉 ⇒ 这条必须红 |
| V5 | `Dispose` 继承而来的两种形状仍可用 | 判据换成查直接成员表 ⇒ 必须红（#823 的退回对照）|
| V6 | `xtask test bootstrap` 绿（support 阶段编译器自身不用 `using`）| — |
| V7 | 全绿：`xtask test` + `build sdk` → `test examples` | — |
| V8 | **不需要** bump `CompilerFingerprint`（I2：`using` 此前编不过 ⇒ 无缓存条目）| 若发现某种写法**修前编得过而结果变**，则必须 bump —— IMPL 时主动找一遍 |

## 顺序

T0 → T1 → T2 → T3 → T4 → T5 → T6。T2 必须早于 T3（拦截不开口，语句永远撞 E0209）。
