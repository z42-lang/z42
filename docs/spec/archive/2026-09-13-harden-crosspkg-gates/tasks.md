# Tasks: harden-crosspkg-gates

> 状态：🟢 已完成 | 创建：2026-09-13 | 类型：fix + test（收口 report-crosspkg-duplicate-* 的三条 Deferred）

**变更说明：** 把 `report-crosspkg-duplicate-type/-function`（#576/#577/#586/#589）留下的三条
Deferred 一次收口。三件事各自独立，共同点是「**让已有的门真的覆盖到、真的会响**」。

## ① 引用形态的覆盖补齐（compiler）

E0601/E0606 挂在 `TypeChecker._chkTypeRef` 上，那条路只覆盖 new / 局部变量 / 类型实参 / 模式 /
catch。**逐形态实测**后发现两条还漏着：

| 形态 | 修前 | 修后 |
|---|---|---|
| 裸静态调用 `Util.go()` | ✅ E0601（#589 已补） | ✅ |
| **ns 限定静态调用** `A.B.Util.go()` | ⚠️ **零诊断** | ✅ E0601 |
| **静态成员读** `Cfg.N` | ⚠️ **零诊断** | ✅ E0601 |
| ns 限定静态成员读 `A.B.Cfg.N` | `E0401`（该写法本就不受支持，与重复无关） | 不变 |
| new / typeof / is / as | ✅ | ✅ |

两条各走独立分支：ns 限定静态调用在 `_bindMemberCall` 的 qualified 分支（`_dottedPath`），
静态成员读压根不是「调用」（`BoundStaticGet` 那条）。各补一句 `_chkTypeRefPkg`。

> ⚠️ **量覆盖表时探针骗过我一次**：它只 grep `E0601|E0606|E0456`，于是把 `E0401` 显示成
> 「零诊断」，害我以为还有第三个洞、差点去"修"一个不存在的问题。
> **判定「有没有报」必须抓任何诊断码，不能只抓自己关心的那几个。**

## ② 两个「手工验证 fixture」转成自动门（test）

`class_internal_access` / `interface_internal_access` 是 E0404（跨包 internal 引用）的 fixture，
但它们**故意不放 `expected_output.txt`** —— 因为当年 runner 只支持「成功运行 + stdout 比对」，
表达不了「期望构建失败」。后果：runner 直接跳过它们，**连 FAIL 都不是，是根本没跑**，
两个 README 里白纸黑字写着「手工验证步骤」。

#576 给 runner 加的 `expected_build_error.txt` 约定正好补上这个能力 ⇒ 本 change 把两者转为
自动门。cross-zpkg 28 → 30。

## ③ 运行期 duplicate：**实测后决定不升 error**（runtime）

原 Deferred 记的是「是否升成 error，需先厘清合法重复场景」。厘清结果是**不该升**：

- **我先按 error 实现了一版，然后被实测否掉**：加载是**惰性且按包**的，两个包完全可能在一个
  程序从不触碰的名字上冲突（A 因 `A.X` 被加载、B 因 `B.Y` 被加载，而它们碰巧都有 `Ns.W`）。
  在 load 时报错 = **为一个 app 既没引用、也无权修的冲突把它挡死** —— 这正是编译期 E0601
  刻意用「使用位」原则避开的行为。自己定的原则不能在运行期反着来。
- **正解是「使用位报错」**，但那要求解析对歧义名**失败**（即不能保留 first-wins 条目），
  与这些注册表的 **append-only 不变量**冲突（`registry_fingerprint` 的负缓存正是靠「只增不减」
  才能用长度当指纹）。⇒ 登记 Deferred `runtime-ambiguous-symbol-use-site-error`。

**实际改的两件事**：
1. **补上一条完全静默的路径**：`register_loaded_artifact`（按路径临时加载：测试宿主 / REPL）
   此前对重复类型**一声不吭**地跳过，而 zpkg 加载那条是会 warn 的。现补 `debug!`——
   first-wins 在那里是**故意的**（REPL 每轮编新模块、后轮引用前轮），但「故意」不等于「该隐身」。
2. **告警文案改成可行动的**：点明「谁赢由加载顺序决定」+ 最常见真因（libs 里残留了一份被改名
   包的旧 zpkg），并在代码里写死「为什么这里不升 error」，免得下一个人再走一遍我这条弯路。

## ④ enum：本族里唯一「静默错**值**」的形态（收尾追加）

继续逐形态扫的时候翻出来的，**两个洞、两处根因**：

| 形态 | 修前 | 根因 | 修法 |
|---|---|---|---|
| enum 作**类型注解** `Color c;` | 零诊断 | `_mergeImportedEnums` **从不并 `EnumTypeNs`** ⇒ 导入 enum 的 `Z42ClassType.Enum(name)` 恒 `Namespace=""`、`Fqn()` 退化成裸名，与 `ClassPkgAll` 的 `ns.Name` 对不上 | 补数据（并 `EnumTypeNs`）—— 既有 `_chkTypeRefPkg` **自然点亮，零新检查** |
| enum **常量读** `Color.Green` | 零诊断 | 它绑成 `BoundLitInt`（成员就是编译期常量），压根不经任何类型引用检查 | 那条分支加 `ChkEnumOrigins` |

⚠️ **这条的危害与前面几种不同**：同名 enum 在两个包里**成员顺序可以不同**，于是
`Color.Green` 静默折出**另一个整数**（实测 `{Red,Green}` vs `{Green,Red}` ⇒ 1 vs 0）。
其它形态错的是「绑到哪一份」，这条错的是「算出什么数」。

⚠️ **单测原语造不出导入 enum**：`ExportedTypeExtractor.Extract` 从不抽取源码里的 enum
（硬编码 `BuiltinTypeDefs._builtinEnums()`），真实跨包 enum 由 `TsigReconcile` 从 zbc TYPE 段重建。
我第一版照搬 `dupImports` 写用例，**用例红而真实三包 e2e 绿**，差点回头去"修"一个不存在的问题。
⇒ 单测改**手工构造** `ExportedEnumZ`，TSIG 那条真路由新 fixture `dup_enum_crosspkg` 守。

## 验证

- [x] 逐形态覆盖表实测（8 种引用形态，抓**任何**诊断码）
- [x] 单测 +6（ns 限定静态调用 / 静态成员读 / enum 常量读 / enum 类型注解 + 2 负例），共 **20 条**全绿
- [x] 退回对照 ×2：① 撤掉两句 `_chkTypeRefPkg` → 2 条新正例红、负例照绿；② 撤掉 `EnumTypeNs` 并入 + `ChkEnumOrigins` → 2 条 enum 正例红、负例照绿
- [x] 两个 internal fixture 自校准：把 target 的 `Secret` 改 public → 判红；还原 → 绿
- [x] 新增 e2e fixture `dup_enum_crosspkg`（真实 TSIG 路径）；cross-zpkg **31/31**
- [x] `xtask test` **冷构建**全绿（13 stage，按 §3.1 事故二先清热产物）+ 自举不动点 3/3
- [x] 零格式 bump

## Deferred

- `runtime-ambiguous-symbol-use-site-error`：运行期把「歧义名」推迟到**使用位**报错。
  前置 = 解决 append-only 不变量（要么允许移除条目并改负缓存指纹，要么在 resolve 侧加一个
  「歧义集」判定并让消费点产出专门的错误消息，而非误导性的 `undefined function`）。
