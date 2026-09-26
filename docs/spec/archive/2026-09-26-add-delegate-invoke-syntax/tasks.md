# Tasks: 委托值上的 `.Invoke(args)` 显式调用语法

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26 | 归档：2026-09-26 | PR #861
> User 已确认（含 D3 的 arity 范围决策）
> 分支/worktree：`learn-ch19` @ `wt-learn19` | 基于：origin/main `4ae522839`（#854）
> 类型：`lang`（按 workflow 词汇警报走完整流程 1–9）——
> **零 VM 改动 / 零格式 bump / 零新诊断码**（E0401 / E1005 / E1006 全部复用）
>
> 起因：学习手册第 19 章（Lambda、闭包与委托）写作前逐段实跑核实参考手册，
> 发现 `delegates-events.md` 教的单播 event 触发模板照着写必崩。
> 相关线：[[z42-learn-book-line]]

## 进度概览

- [x] 阶段 1: `MemberResolver` 加 `Z42FuncType` 分支（`.Invoke` → `BoundIndirectCall`）
- [x] 阶段 2: 非 `Invoke` 成员名报 E0401
- [x] 阶段 3: arity 校验（E1005 / E1006，三个产出点，型参分支除外）
- [x] 阶段 4: **摸底 —— 全仓零命中**（详见下方摸底结果）
- [x] 阶段 5: e2e golden + 10 条诊断单测（含 D4 的两条阴性对照）
- [x] 阶段 6: 文档（`delegates-events.md` + `functions.md` + `error-codes.md` + 学习手册第 19 章）
- [~] 阶段 7: GREEN 全绿；PR 待开

## 阶段 1：`.Invoke` 脱糖

- [x] 1.1 分支已插入；⭐ 实施中因行数门禁**整块移到 `MemberResolver.Func.z42`**，主文件只留一行派发
- [x] 1.2 返回类型取 `ft.Ret`；诊断单测 `test_invoke_result_has_the_declared_return_type` 钉住
      （`string s = f.Invoke(1)` 现在报 E0402 —— Unknown 时不会报）
- [x] 1.3 四种载体全通（e2e golden 的 ①②③⑤⑥⑦ 条）

## 阶段 2：E0401

- [x] 2.1 E0401 已发：`no method \`Bogus\` on delegate type \`Func<Int32, Int32>\` (only \`Invoke\` is available)`
- [x] 2.2 未套用 stub 豁免（理由写在 `MemberResolver.Func.z42` 就地注释）
- [x] 2.3 ✅ **已有既存诊断，不用新写**：`var g = f.Invoke;` 报
      `E0402: member access on non-class \`Func<Int32, Int32>\`` —— 它走**成员访问**路（不是调用路），
      那条路早就拦住了。⭐ 先查再写：这条任务本来预期要加代码，实测一行都不用加

## 阶段 3：arity 校验

- [x] 3.1 `_checkFuncValueArity`（`OverloadBinder.z42`）—— ⭐ **现成的 `_checkLocalFnArity` 正是它**，
      局部函数在用、间接调用没接；抽出共用后局部函数那条改为复用
- [x] 3.2 三个产出点全接（经 `BindIndirectArgs` 包装，让另两处**一行换一行**、不顶主文件行数）
- [x] 3.3 型参分支未接；诊断单测 `test_type_param_receiver_arity_is_not_checked` 钉住
- [x] 3.4 两者**都查了**，见下方「第三个既存缺口」：默认值被静默忽略、`params` 整条没实现。
      ⚠️ `ParamsFrom` 不随 delegate 声明填充 ⇒ 守卫不生效，`params` 委托的调用会多报一条 E1006；
      **不是回归**（用种子编译器实测：同一用例改动前就已报 E0402、根本编不过）

## 阶段 4：摸底（**先摸后做的那一步，不可省**）

- [x] 4.1 **零命中** —— 没加临时探针，直接让新诊断当探针跑全仓（更精确）：13→15 stage 全绿
- [x] 4.2 `cargo test --workspace --lib`（debug）**1442 passed / 0 failed**
- [x] 4.3 手动判定：无需 bump（理由见下方摸底结果）
- [x] 4.4 存量先跑，零红；新测试随后加

## 阶段 5：测试

- [x] 5.1 `src/tests/delegates/delegate_invoke_syntax.z42`（8 组 + 2 组阴性对照）
- [x] 5.2 `src/compiler/z42c.semantics/tests/typecheck/delegate_invoke/`（10 条，含 2 条正面对照）
- [x] 5.3 阴性对照一在 e2e golden 里（2 个 handler + `Count()`）
- [x] 5.4 阴性对照二在 e2e golden 里（`methodof(Api.Twice(int)).Invoke`）
- [x] 5.5 ⭐ 用了**更强的形态**：崩溃是在同一棵树、改动之前亲眼跑出来的；
      「静态方法组是既存缺口」也靠**未改动的种子编译器**跑同一用例证实
- [x] 5.6 不适用：本刀不碰优化 pass，e2e 走常规档即可

## 阶段 6：文档

- [x] 6.1 `docs/reference/src/language/delegates-events.md`：`:61` 的「等价」现在为真；
      `:62` 与 `:264-266` 的单播触发模板终于能跑 —— 对齐日期更新
- [x] 6.2 `docs/reference/src/language/functions.md:283`：「函数值取出后不能就地调用」
      **边界记宽了** —— 实测**数组下标可以**就地调（stdlib 自己在用
      `MulticastAction.z42:122` `snapStrong[i](arg)`），只有 `List` 索引器不行。订正
- [x] 6.3 `docs/reference/src/language/functions.md`：补「函数类型不能做数组元素类型」
      （`((int) -> void)[]` 连声明都不认，报 E0401）
- [x] 6.4 补一条两页都没写的事实：`(T) -> R` 与 `Func<T,R>` **双向可互赋**
      （根因 `Z42FuncType.IsAssignableTo` 走结构比较、不看名字）
- [x] 6.5 `docs/reference/src/appendix/error-codes.md`：E0401 / E1005 / E1006 三条词条
      各补「委托接收者」这一格
- [x] 6.6 学习手册第 19 章据此落地（User 已裁决：**完整覆盖，含多播全套**）

## 阶段 7：GREEN + PR

- [x] 7.1 **✅ GREEN — all stages passed**（15 stage / 8m44s）+ cargo lib 1442/0
- [x] 7.2 每轮都 `build compiler` → `build sdk` 后再验
- [x] 7.3 PR **#861**（基于 main `4ae522839`，开 PR 前已跑 GREEN）
- [x] 7.4 归档在本分支内完成、随 PR #861 一起合并（阶段 9 铁律）
      ⚠️ 自查：我是**先开 PR 再补归档 commit** 的，铁律原话是「步骤 1–4 必须在开 PR 之前
      就 commit」。PR 未合 ⇒ 落地形态相同（零多余 main 提交），但**顺序确实走反了**，下次
      先归档再开 PR。

## 并入本变更首个 commit 的归档（workflow 阶段 0）

粗扫 `docs/spec/changes/`，两个容器标 🟢已完成 且未勾项均为「PR/CI 盯着」这类流程项
（PR 早已合并）⇒ 归档：

- [x] `fix-crosspkg-static-ns-collision`（🟢 完成 2026-07-16）
- [x] `refactor-interp-boilerplate`（🟢 完成 2026-08-24，PR #285）

**不归档** `z42b-owns-test-targets` —— 6 条未勾全是「（刀二）」，是刻意未动的第二刀，
标题也只说「已完成（刀一）」；归了等于把刀二的计划埋掉。

---

## 摸底结果（阶段 4，2026-09-26）

**全仓零命中。** `xtask test all` 13 个 stage 里没有任何存量用例因本变更判红 ——
没有一处调用点靠「多传被静默丢掉」活着，也没有一处依赖 `.Invoke` 崩溃。
`cargo test --workspace --lib`（debug）**1442 passed / 0 failed**。

⭐ **「零命中」必须配正面对照**，否则交付的是恒不响的门。三条码都实测响了：

| 探针 | 实测 |
|---|---|
| `f.Bogus()` | `E0401: no method \`Bogus\` on delegate type \`Func<Int32, Int32>\` (only \`Invoke\` is available)` |
| `f(1, 2)`（形参 1 个） | `E1006: too many arguments to \`f\`: expects 1; got 2 argument(s)` |
| `add(1)`（形参 2 个） | `E1005: too few arguments to \`add\`: expects 2; got 1 argument(s)` |
| `f.Invoke(1, 2)` | E1006（新写法不绕过校验） |
| `f("str")` | E0402（不变） |

⭐ **阴性对照是最强的那种**：`.Invoke` 崩溃是我**在同一棵树、改动之前**亲眼跑出来的
（`VCall: expected object, got FuncRef(...)`），比事后撤回修复更硬。

指纹：不新增 IR 指令、不动 zbc/zpkg；新增的只有「原本崩」与「原本无诊断」两档 ⇒ 无需 bump。

## 🔴 行数门禁：正解是拆文件（这次差点又走歪）

第一版把新分支内联在 `MemberResolver.z42` 里 ⇒ 923 行，超 886 硬上限，`lines` stage 判红。

⭐ **根因不是我写多了：该文件在 main 上就已 882 行，离上限只剩 4 行** —— 任何改动都会顶出去。
按包内先例（`MemberResolver.Prim.z42` / `.Bare.z42` / `.Static.z42` / `.TypeParam.z42` / `.Subst.z42`）
拆出 **`MemberResolver.Func.z42`**（74 行），详细注释搬到新文件抬头，主文件只留一行派发。
另把「绑实参 + 校验个数」抽成 `BindIndirectArgs`，让另两个产出点**一行换一行** ⇒
主文件净增 **2 行**（882 → 884，余量 2）。

⚠️ **留给下一个人**：`MemberResolver.z42` 只剩 2 行余量，下次动它**必然**要先拆。
（清单没有 `[sources]` 段 ⇒ 新 `.z42` 按默认约定自动纳入，不用改 toml。）

## 🆕 本轮顺带发现的两个既存缺口（**未修**，与本刀无关）

1. **静态方法组的限定名形式取不了引用**：`Func<int,int> f = Api.Twice;` 报
   `E0401: undefined: Api`。自由函数与**实例**方法组 `o.Twice` 都正常。
   ⭐ 判据取证干净：**用未改动的种子编译器（`.z42/`）跑同一个用例，同样报错** ⇒ 既存。
   已在学习手册立活示例 `examples/types/lambdas/gaps/staticmg.z42` 钉住 + 参考手册记缺口。
   ⇒ 候选后续 `support-static-method-group-conversion`。
2. **委托形参的默认值被静默忽略**：`delegate void D(int a, int b = 5)` 声明能过、零诊断，
   `d(1)` 运行期得 `b=null` 而非 5；而**局部函数同样形状报 E1005**（两条路口径不一致）。
   本变更的 arity 校验让这种调用报 E1005（静默错值 → 编译错误，严格更好）。
   全仓 81 个 delegate 声明里带默认值的 **0 个**。⇒ 候选后续 `honor-delegate-param-defaults`。
3. **`params` 在委托类型上整条没实现**：`delegate void P(params int[] xs); P p = Impl; p(1,2,3)`
   报 `E0402: cannot assign int to Int32[]` —— 调用点**不做 params 展开**，且 `ParamsFrom`
   不随 delegate 声明填充（所以 `_checkFuncValueArity` 的 `params` 守卫对它不生效，会多报一条
   E1006）。⭐ **不是回归**：用未改动的种子编译器跑同一用例，改动前就已报 E0402、根本编不过；
   本刀只是在一个已经失败的程序上多报一条。⇒ 候选后续 `support-params-in-delegate-types`。
   ⚠️ 另记一条：**lambda 形参不支持数组类型**（`(int[] xs) => …` 报 E0202 系列解析错），
   我第一个 params 探针就栽在这上面 —— 当时把连带噪声误读成了本刀的问题，
   **靠「两个编译器输出逐字相同」才分辨出来**。

## 阶段 9 文档同步（doc-system 三问逐条过）

1. **用户能看见吗？** ✅
   - `docs/reference/src/language/delegates-events.md`：`.Invoke` 从谎报变准确；新增「实参个数」
     一行；方法组转换那格订正（见下）
   - `docs/reference/src/language/functions.md`：「函数值取出后不能就地调用」**边界记宽了**，
     订正为「只管 `List` 索引器，数组下标可以」+ 补「函数类型不能当数组元素类型」
     + 补「`(T) -> R` 与委托双向可互赋」
   - `docs/reference/src/appendix/error-codes.md`：E0401 / E1005 / E1006 三条各补「委托收者」这一格
   - 学习手册第 19 章 + `examples/types/lambdas/`（八场景 / 8 transcript）
2. **下一个接手的人不读文档能看懂吗？** ✅
   `docs/internals/src/runtime/delegates-events.md` 新增 §2.1a「`.Invoke` 为什么不派发到那个桩」
   —— 这是**反直觉决策**（仓里明明有个叫 `<FQ>.Invoke` 的合成函数，却不能当派发目标，
   因为 FuncRef 没有 TypeDesc），不写下来下一个人一定会去接那个桩。
3. **目录结构 / 对外入口 / 依赖变了吗？** ❌ 新增 `MemberResolver.Func.z42` 只是同一个
   `partial class` 的第 6 个文件，`src/compiler/z42c.semantics/src/` 无 README、清单也无
   `[sources]` 段（默认约定自动纳入）⇒ 无需改。

**正交三处**：根 README ❌（不影响仓库门面）；`docs/roadmap.md` ✅（Deferred Backlog 新增
三条：`support-static-method-group-conversion` / `honor-delegate-param-defaults` /
`support-params-in-delegate-types`）；`docs/agent/rules/` ❌（未改协作规则）。

## 🔴 归档前抓到自己一处「边界记窄」

初稿把静态方法组那条缺口写成「**限定名形式**取不了引用」—— **记窄了**。触发点是
`internals` §2.2 写着「静态方法组 → `LoadFnCached`」与我的实测冲突，逼我回头分形态实测：

| 形态 | 实测 |
|---|---|
| 自由函数 `Func<int,int> a = Free;` | ✅ |
| 实例方法组 `o.Twice` | ✅ |
| 类静态、限定名 `C.F` | 🔴 `E0401: undefined: C` |
| 类静态、类内不限定 `F` | 🔴 `E0401: undefined: F` |

⇒ 真相是**静态方法组整条没接**，两种拼写都不行；`internals` 说的「静态方法组」其实指
**自由函数**（已就地订正，并加 ⚠️ 说明为什么旧措辞会被读错）。
⭐ **教训：文档与实测冲突时，别急着判文档错 —— 先把形态拆开各测一遍**，
我这次差点把「记窄的边界」写进三本书。
