# tasks: add-missing-return-check

> 类型：**fix**（补上一条从未发射的诊断）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「32 个零发射诊断码」里最要紧的一条。
> User 口径：**先修正确性的 bug，然后再进行性能优化**。

## Why

漏 `return` 的非 void 函数**编得过、零诊断**：

```z42
int NoReturn(int x) { if (x > 0) { return 1; } }   // 编译通过
int AlsoNoReturn()  { int y = 5; }                 // 编译通过
```

运行期返回 `Null`，然后在**调用方的某个毫不相干的位置**崩：

```
Error: VCall: expected object, got Null
  at Test.Main (line 15, col 5)          ← 报的地方与真正的错相距很远
```

这是审计 R3「`Value::Null` 六义哨兵」的标准形态：**缺失的诊断被翻译成一个貌似合理的值**。

`DiagnosticCodes.MissingReturn = "E0403"` 一直摆在 z42c 里、`error-codes.md` 也记着它
（标注「⚠️ 零发射点」，**没说谎**），而归档稿 `2026-04-17-compiler-analysis-passes` 里
「**1.3 发射 `E0403 MissingReturn` 错误**」自始至终是**未打勾**的 —— 按纪律先查了 archive：
**是当年没做完，不是刻意裁掉**。

## 🔴 最大的坑：不能复用 `FlowAnalyzer.AlwaysReturns`

`AlwaysReturns` 看起来正好是要的东西，但它的**保守方向对本检查是反的**。它的注释明写：

> 保守方向：拿不准就返 false（= 会正常结束）。返 false 只会让 join 更严（少传播赋值），
> 那是漏报方向。

对定值分析（E0407）而言 `false` 是**漏报**方向、安全。而漏 return 检查的形状是
`!X(body)` ⇒ **拿不准就误报**。实测它对这些**合法**形态都返 `false`：

- `while (true) { return x; }`（注释里明写「判它需要常量求值 + break 分析，收益小于风险，**留白**」）
- 每臂都 return 的 `switch`

直接复用会把大量合法代码判红。**同一个谓词对不同消费方回答的是不同问题** ——
与 #892 修的 `_depHasFunction` 是同一族。

## What Changes

`FlowAnalyzer` 新增**反向保守**的谓词 `NeverCompletes(BoundStmt)`：「这条语句必定不会正常结束吗」。

⚠️ 它**没有「拿不准」档** —— 漏一种语句类型就是误报，所以全部 **16 种 `BoundStmt`** 逐个分类：

| 语句 | `NeverCompletes` |
|---|---|
| `BoundReturn` / `BoundThrow` | true |
| `BoundBlock` | 任一子语句 true ⇒ true |
| `BoundIf` | 有 else 且两支皆 true |
| `BoundTry` | (try 与**每个** catch 皆 true) 或 finally 自己 true |
| `BoundWhile` / `BoundDoWhile` | 条件是字面量 `true` 且体内无能逃出本循环的 `break` |
| `BoundFor` | 无条件或条件字面量 `true`，且同上 |
| `BoundSwitch` | 有 default + 每臂皆 true + 无 `break` 逃出 + 无守卫臂 |
| `BoundForeach` / `BoundBreak` / `BoundContinue` / `BoundExprStmt` / `BoundVarDeclStmt` / `BoundDeconstructDeclStmt` / `BoundLocalFunction` | false |

⚠️ **`break` / `continue` 一律算 false**：它们虽然也不「正常结束」，但算成必定退出会把
`while (true) { break; }` 判成永不落下来 —— 反而错得更远（这条陷阱本仓已有案底）。
循环的判定改用独立的 `_canBreakOut` 扫描，它**不下沉到嵌套的循环 / switch / 局部函数**
（那里的 `break` 绑定到内层构造）。

报点接在 `FlowAnalyzer.Check` 末尾（单一入口，属性/索引器四处访问器在 #880 已并入同一条路）。
ctor 的 `RetType` 是占位 `NamedType("")`、void 是 `NamedType("void")` ⇒ 两者跳过。

## 指纹 39 → 40

**bump 的理由是诊断变**：漏 return 的源文件此前**编得过、零诊断**，现在报 E0403，而它们的
哈希一字未变 ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉。

⚠️ **发码零变化**（诊断不改 IR）⇒ **CI 的 fingerprint 守门对这一档是瞎的**，手动 bump。

## Scope

- `src/compiler/z42c.semantics/src/FlowAnalyzer.z42`
- `src/compiler/z42c.semantics/tests/typecheck/missing_return_tests.z42`（新，22 条）
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（指纹 39 → 40）
- `docs/reference/src/appendix/error-codes.md`（E0403 状态：⚠️ 零发射 → ✅）
- `docs/reference/src/language/functions.md`（「所有路径必须 return」小节）

## Tasks

- [x] `NeverCompletes` + `_canBreakOut` + `_checkMissingReturn`，接进 `FlowAnalyzer.Check`
- [x] 22 条测试（**7 阳性 + 15 阴性**；阴性是重心）
- [x] **全仓误报面清查：零 E0403** —— 编译器 ~10 万行 / stdlib 25 包 / xtask 79 文件 /
      workload + toolchain / 382 个 golden / 347 个 stdlib 测试文件 / examples
- [x] 自举不动点 3/3 gen1==gen2 + `z42c [Test]` 24 unit 全过
- [x] `xtask test e2e` 740 passed, 0 failed
- [x] `xtask test diagcodes`：**113 live / 31 zero-emission**（E0403 转活码）、0 violation
- [x] `CompilerFingerprint` 39 → 40
- [ ] GREEN：CI 全矩阵绿

## 如实记下：本刀在仓里**零真阳性**

全仓一条 E0403 都没有 —— 没人写过漏 return 的函数（那种代码会崩，早被发现）。
所以这是一条**预防性**检查，不是「修掉了 N 个既存 bug」。**别把它写成后者。**
它的价值在于：把一个「静默返回 Null、崩在远处」的失败模式变成一条指着问题本身的编译错误。

## 我栽的一次（已修正）

测试第一版用了 `Std.Exception`，4 条红。原因：`SemanticDump.FirstErrorCode` 走的是
**独立 Infer 路径、不链 stdlib**（`exhaust_tests.z42` 头注早写了「测试源不用 Std」），
`Exception` 解成 `<unknown>` 并报一条**与 E0403 无关**的错。改用本地声明的 `Ex1` 后全绿。
实测口径：`--dump-bound` 对 `throw new Exception("x")` 报 `(1 error(s))`、对 `throw new Ex1()` 报 0 条。

## 行数门禁：拆成 partial（CI 第一轮判红后补）

第一轮 CI 的 `test-host` 三条腿红在 **`xtask test lines`** —— 不是我的检查出错，是
`FlowAnalyzer.z42` 从 807 行被我推到 **954**，越过硬限 **886**
（`scanned 883 file(s): 7 over 886 lines (6 known, 1 new/grown)`）。

按仓里既有判据 **行数门禁挡路的正解是拆文件，不是压注释**：新块整段搬到
`FlowAnalyzer.Reachability.z42`，两边同一个类（`public sealed partial class FlowAnalyzer`），
照 `MemberResolver.Func.z42` 的先例 —— 那个文件的头注也写着「partial 拆分只为守行数上限」。

拆后 808 + 161 行，`test lines` 绿（6 known / **0 new/grown**）；拆分后复跑
`test compiler`：22 条测试仍全过、不动点 3/3、`test walkers` 绿。

## 不做（Out of Scope）

- **不做「不可达代码」告警**（`return` 之后还有语句）。那是另一个码、另一条判据。
- **不把条件常量求值做深**：只认字面量 `true`。`while (1 == 1)` 仍会要求 return —— 保守方向、
  且真要做得接常量折叠，收益不值。
- **不碰 lambda / 局部函数体的 return 检查**：它们的返回类型是推断的，判据不同；
  `_canBreakOut` 已经明确不下沉进 `BoundLocalFunction`。
