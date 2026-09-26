# Design: L2 规则表

> 状态：🔵 DRAFT 待审批 ｜ 前置：[proposal.md](proposal.md)

## 已核实的落位事实（2026-09-26，本 worktree 实读，不必重查）

| # | 事实 | 依据 |
|---|---|---|
| L1 | **z42 无 delegate、无泛型类字段** ⇒ 表不能存函数指针。仓内先例是 **int tag + if-else `Lookup` + rule class**，抬头明写「加一个运算符 = 加一行 if」 | `BinaryTypeTable.z42:1-6`；[[z42c-no-cross-pkg-delegates]] |
| L2 | ⇒ `syntax-customization.md:140-146,177-193` 画的 `nud:`/`led:`/`handler:` **函数指针形态在 z42c 里写不出来**（那三张表是照 C# 设计稿画的）。落地形态必须换成 int tag + 一个集中 switch/if 链 | 由 L1 推出；本 change 要顺手订正该页 |
| L3 | 语句关键字派发**全仓唯一一处**：`Parser.ParseStatement`（`Parser.z42:287-340`）。`StmtParser.z42` 无第二条链 | 实读 + grep |
| L4 | 表达式侧 bp 有**九种形态**、外加三处跨构造魔数（`11`/`45`/前缀 `85`），清单见 proposal Why #1 | 实读 `ExprParser.z42` 主循环 `:11-234`、`_parsePrefix:333-410`、`_infixBp:770-783` |
| L5 | 特性门现只两个调用点：`Parser.z42:319`(`control_flow`) / `:322`(`exceptions`)；表喂入在 `IncrementalDriver.z42:63,438` 两处；建表在 `Main.z42:384-402` | grep 全 `src/` |
| L6 | `Phase1Profile()` = **15** 个名字；`Has`/`NameAt` 已存在；`Main._build:388` 已对未知名报错退出 | `LanguageFeatures.z42:86-105,56,66` |
| L7 | `MinimalProfile()` **零生产调用方**（只有 `z42c.core/tests/features.z42:42`），且 `[syntax]` 语法里**选不到 profile**（`Main._build:384` 固定 `Phase1Profile()` 起底） | grep |
| L8 | 门的模板：`scripts/test/xtask_test_incremental.z42:210` `_syntaxKnobTakesEffect`，四条判据（阳性对照 / 负例报 E0301 / 未知名报错 / 只改 toml 字节要变），每条都验「**红的理由对不对**」 | 实读 |

## 🔴 L9（顺带核出的一条洞，未实测，本批可顺手补）：特性门覆盖面不全

`ParseStatement` 的门只盖 `if/while/for/foreach/do/switch`（`:317-319`）与 `try`（`:321-322`）。
**`break`（`:331`）/ `continue`（`:332`）/ `throw`（`:333`）没有挂门**：

- `control_flow = false` 时 `break;` / `continue;` 仍照常解析 ⇒ 用户拿到的是语义层
  「break 不在循环里」之类的诊断，**不是 E0301**；
- `exceptions = false` 时 `throw` 同理。

⚠️ 这正是 `:314-316` 那段注释自己警告的形状 ——「部分接线会让特性名变成一句假话」。
表化后这三项各是一行表项、`feature` 字段一填即补齐 ⇒ 成本近零。

**必须实测后再写结论**（本程序纪律：agent/推理提出的缺陷一律自己复现）：
判别 = 夹具只写 `break;`（无循环）+ `[syntax] control_flow = false`，看诊断是 E0301 还是别的；
两种结果都「红」，**红的理由不同**。

## D2-0：PR 切分

批 1 是 User 裁的「一条 PR 全做」，代价是 review 里纯收敛与行为变更混在一起（靠 T1/T2/T3
各自单独字节对账缓解）。批 2 的体量与字节风险都更大（表达式主循环是全编译器最热的解析路径）。

| 选项 | 内容 |
|---|---|
| **甲** | 一条 PR 全做（表达式 + 语句 + lookahead + 门 + 文档） |
| **乙（推荐）** | **2a** = 表达式规则表（纯收敛，字节必须不变）；**2b** = 语句表 + lookahead 顺序表 + 顺序门 + 特性门补洞（L9）+ 文档订正 |

推荐乙的理由：2a 是「九种形态收敛成一张表」，判据单一（**字节一个不许变**）；2b 才引入新行为
（新 E0301 发射点、顺序门）。混在一条 PR 里时，字节对账一旦不为零，要先排除「是不是 2b 那部分
干的」——批 1 已经在这上面吃过一次基线取错的亏（见 [[verify-conclusion-after-reseeding]]）。

## D2-1：表的形态

三种，按「多少东西真变成数据」递增：

### 甲 —— 只收数值（`ParseTable.LeftBp(kind)`），派发链保持手写

主循环的九处内联守卫 `minBp <= N` 改成查表 `minBp <= ParseTable.LeftBp(k)`，`_infixBp` 并入同一张表。

- ✅ 最小改动、字节风险最低。
- 🔴 拿不到「顺序是数据」：`Lt` 泛型回溯必须先于二元 `<` 这条约束仍然只靠**代码位置**。
- 🔴 `feature` 字段没有自然归宿（守卫还是手写的 ⇒ 每处得自己 `&& IsEnabled(...)`，又是「挂钩挂一半」
  的形状，见 [[parallel-pass-sequences-miss-new-hook]]）。

### 乙 —— 全 Pratt：一张表 + 一个守卫 + 按 led tag 派发（**推荐**）

```
// z42c.syntax（表与解析器同层；不需要 core，因为语义层不消费它）
class ParseRule { public int LeftBp; public int LedKind; public string Feature; }
static class LedKind { Binary=1; Assign=2; Ternary=3; IsPattern=4; As=5;
                       PostfixSwitch=6; PostfixWith=7; ObsoleteNullCoalesce=8;
                       Member=9; Call=10; Index=11; PostIncDec=12; GenericCallOrLt=13; }
ParseTable.Lookup(int kind) -> ParseRule   // if-else 串，照 BinaryTypeTable
```

主循环变成：查表 → `if (r == null || r.LeftBp < minBp) break;` → `if (!feature) break;` →
`switch/if` on `LedKind`（各分支体**原样搬**，一行不改）。

- ✅ 九处守卫塌成**一处**；`feature` 是表项字段（`syntax-customization.md:148` 的设计原话）。
- ✅ 后缀链（`.`/`()`/`[]`/`++`）给 bp **90**（严格高于 85）⇒ 行为等价：今天它们无条件执行，
  而所有调用点的 `minBp` 最大是 85（`_parseExpr(85)` 六处）⇒ `90 >= minBp` 恒真。
  **这是把「最紧」从『没有数值』变成『数值最大』**，等价性可用一句断言守住（见 tasks V2）。
- 🔴 `Lt` 那条**回溯**消歧（`:116-146`：试解析 `<T,...>(` 失败则 `_reset`/`TruncateTo`/回填
  `_pendingGt`）不是普通 led ⇒ 它作为一条表项（`LedKind.GenericCallOrLt`）但内部仍是两段式。
  表能记录「它在 `<` 这个 kind 上、bp 与二元 `<` 同为 60」，**记录不了「先试后退」**。
  ⇒ 这一条的顺序约束仍在代码里，但**从「隐式的行文位置」变成「一条表项里的显式两段」**。
- 🔴 主循环重排是全编译器最热路径之一 ⇒ 字节对账必须逐 commit 做，不能只对总账。

### 丙 —— 连前缀（nud）也进表

`_parsePrefix` 的一元 `85` 六处也表化（`NudKind` tag）。

- ✅ 形态最完整，「加一个前缀运算符 = 加一行」才真正成立。
- 🔴 四种 cast（`:359/371/385/408`）各自带回溯与 follow-集判断，**不是同构的 nud**，压进一张表
  只会得到四条「指回代码」的表项 —— 与批 1 否掉「形状甲」的理由同款（表达力，不是工作量）。

> **推荐：乙。** 理由与批 1 的 D-new-1 一脉相承 —— 让**能成为数据的部分**成为数据
> （bp / feature / led 角色），让**本质是有状态回溯的部分**留在代码里并把它的顺序显式化。
> 丙的四条 cast 表项会重复批 1 已经否过的错误。

## D2-2：三处跨构造魔数怎么表达「相对关系」

| 今天 | 真正的约束 | 建议落法 |
|---|---|---|
| `_parseExpr(11)` ×2（switch arm guard/body，`ExprParser:96,99`）| 「排除赋值」= 赋值 bp + 1 | `ParseTable.AboveAssign()`（= `LeftBp(Eq) + 1`）|
| `_parseExpr(10)`（lambda 体，`:852`）| 「含赋值层」= 赋值 bp | `ParseTable.LeftBp(TokenKind.Eq)` |
| `_parseExpr(45)`（模式常量，`PatternParser:94`）| 「高于 `\|`、低于 `^`」= 挡掉 or-模式的 `\|`、允许 `^`/`&`/算术 | `ParseTable.BetweenOrAndXor()`（= `LeftBp(Pipe) + 1`），**并加一条不变式断言** `LeftBp(Pipe) < it <= LeftBp(Caret)` |
| `_parseExpr(85)` ×6（前缀一元，`_parsePrefix`）| 「紧于所有二元、松于后缀链」| `ParseTable.UnaryBp()`（常量 85），断言 `UnaryBp > LeftBp(Star)` 且 `UnaryBp < PostfixBp` |

⭐ 关键在于：这些函数**必须从表里算出来**，而不是各自写死同一个数值 ——
否则「改一个数值另一处不跟」这件事会原样保留，只是换了个地方。

## D2-3：lookahead 顺序表 + 那道会红的顺序门

**顺序表**（`Parser.z42:309,334,335,336` 四个探测位）落成一张有序 int tag 数组：

```
static class ProbeKind { DeconstructBrace=1; LocalFn=2; VarDecl=3; DeconstructParen=4; }
StmtProbes.Order = [DeconstructBrace, LocalFn, VarDecl, DeconstructParen]
```

`ParseStatement` 尾部改为「按 `Order` 逐个试」，每个 tag 一个 `_probe(tag)` / `_parse(tag)` 分派。

**门（这是本批的判别力核心）**：顺序是手写 if 链时测试注入不了错误顺序；成为数据后可以。
单测拿**两张顺序表**跑同一批夹具：

| 夹具 | 正序应解析为 | 反序（把 `LocalFn` 放到 `VarDecl` 之后 / 把 `DeconstructParen` 提到 `VarDecl` 之前）应发生 |
|---|---|---|
| `int F() { return 1; }`（方法体内局部函数） | LocalFn | 被 `_isVarDeclStart` 抢走 ⇒ 解析成变量声明 ⇒ **报错** |
| `(int, string) t = e;`（元组 var-decl） | VarDecl | 被 `DeconstructParen` 抢走 ⇒ **报错** |
| `(a, b) = e;`（元组解构） | DeconstructParen | 正序下**不**被 VarDecl 抢（互斥判据：`)` 后是 `=` 还是 Identifier）—— 阳性对照 |
| `{ X: x } = e;` vs `{ ... }` 普通块 | DeconstructBrace / Block | 反序 ⇒ 解构被当成块 ⇒ **报错** |

⚠️ 判别力要求（批 1 教训）：**每条都要跑正反两遍**，只验「正序能过」等于没测门。
且反序那遍要检查「红的理由对不对」（诊断码/消息，不只是退出码）。

## D2-4：本批让哪些运算符级特性门变活

表建起来后，`feature` 字段一填就生效 ⇒ 边际成本几乎只是**那条负例门**。

| 候选 | 表项 | 负例门 | 建议 |
|---|---|---|---|
| `bitwise` | `\|`(44) `^`(46) `&`(48) `<<`(65) `>>`(65) | `a & b` ⇒ E0301 | ✅ 本批做 |
| `ternary` | `?:`(20) | `a ? b : c` ⇒ E0301 | ✅ 本批做 |
| `control_flow` 补洞（L9）| `break` / `continue` | `break;` ⇒ E0301（今天不是）| ✅ 本批做（实测后）|
| `exceptions` 补洞（L9）| `throw` | `throw e;` ⇒ E0301 | ✅ 本批做（实测后）|
| `pattern_match` | `is`-pattern / 后缀 `switch` / switch 语句 | — | ⬜ 记账：它横跨 `PatternParser` 整个文件与语句侧 switch，且与 `control_flow` 的边界要先裁（switch 语句归哪个名字？）|
| `cast` / `lambda` / `tuples` / `nullable` / `generics` / `oop` / `arrays` / `interpolated_str` / `delegates` / `reflection` / `pattern_match` | 散在 nud / 类型解析 / 声明层 | — | ⬜ 记账：**11 个仍是死旋钮**，各需自己的挂点与门 |

⇒ 本批后：15 个名字里 **4 个**真的关得掉东西（今天 2 个）。
🔴 **文档三处（`LanguageFeatures.z42` 抬头 / `z42-toml.md` / `syntax-customization.md`）的
「其余 13 个是死旋钮」必须同步改成 11**，否则这张表又多一句假话。

## D2-5：`MinimalProfile` 是第三个死物

L7：零生产调用方，且 `[syntax]` 里**选不到 profile**（只能逐个名字覆盖）。
`syntax-customization.md:311` 还把它当教学场景的主线（「从 `MinimalProfile()` 起步，每周多打开一项」）。

| 选项 | 说明 |
|---|---|
| **甲（推荐）** | 本批**不接**，但把事实写进文档：profile 不可从 manifest 选择，教学场景今天要靠逐项 `false`。顺带修正 `:311` 的措辞 |
| 乙 | 本批接上 `[syntax] profile = "minimal"` ⇒ 需要新的 manifest 键 + 与逐项覆盖的合成顺序（profile 起底、逐项覆盖）+ 一条门 |

推荐甲的理由：乙会把「批 2 = 表化」扩成「批 2 = 表化 + 新 manifest 键」，
而 `MinimalProfile` 的 4 个名字里有 3 个（`interpolated_str`/`cast`/`bitwise`）今天仍是死旋钮
⇒ **接上了也裁不掉任何语法**，等于再造一句假话。它应当在「死旋钮清零」那条线上做。

## 字节不变怎么守（照批 1 的手法）

1. **同基点基线**：先在 `origin/main`（`4b0d7a018`）上全量构建，留 25 包 zpkg 的 sha256。
   ⚠️ 换基点后基线作废、必重取（批 1 第一次就取错了）⇒ [[verify-conclusion-after-reseeding]]。
2. **逐 commit 对账**，不只对总账（D2-0 选乙时天然满足）。
3. 自举不动点 3/3 + golden 全绿 + `xtask build sdk` → `xtask test examples`
   （examples 门用 `artifacts/.z42` 的 SDK，`build all` 不含 sdk 阶段）⇒ [[local-green-misses-examples-gate]]。
4. 改了派发面 ⇒ 补 `--mode jit` 那一轮（[[local-green-misses-jit-and-lines]]）。
   本批只动解析器 ⇒ 预期不涉派发键，但 `_parseExpr` 是热路径，bench 门若判红按**字节对账**归因
   （[[investigate-micro-ab-false-regressions]]）。
5. 🔴 **行数：`ExprParser.z42` 今天 864 行，硬限 886 ⇒ 只剩 22 行余量**（实测 2026-09-26；
   软限 500 只出 advisory，硬限 886 变红，见 `code-organization.md:49-52,90`）。
   ⇒ **表必须落新文件**（`ParseTable.z42`），且主循环改写后 `ExprParser.z42` 的净增必须 ≤ 22 行。
   乙方案主循环塌成「查表 + 一处守卫 + tag 派发」应当是**净减**（九处守卫各省 1 行），
   但 `switch`/`with`/`is` 那三大段分支体原样搬进 tag 分派时缩进会变 ⇒ 行数不变、diff 很大。
   ⚠️ 若净增超限，拆文件（`ExprParserLed.z42`）是既有手法（该包已有 `ExprParserInterp.z42` 先例）。
   `Parser.z42` 581 / `StmtParser.z42` 427 / `PatternParser.z42` 202 均有余量。
