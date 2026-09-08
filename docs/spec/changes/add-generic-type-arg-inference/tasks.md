# Tasks: 方法级类型实参推断 + 形参位类型实参代换

> 状态：🔴 DRAFT（待阶段 6.5 确认）｜ 创建：2026-09-08
> 分支：`tighten-bare-type-param-erasure` ｜ worktree：`../z42-erasure`（基于 `origin/main` 54c8a1df）

## 进度概览

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | 前置调研与爆炸半径实测 | 🟢 已完成（见下） |
| A | 泛型类实例方法形参位代换 | 🟢 已完成（欠债 0，三道对照全过，不动点 3/3） |
| B | 显式方法类型实参形参位代换 | 🟢 已完成（欠债 0，退回对照精确命中，不动点 3/3） |
| C | 方法级类型实参推断 + 推断调用的 where 校验 | 🟢 已完成（欠债 0，阳性+退回对照全过，不动点 3/3） |
| D | callee 消费型参时要求显式类型实参（User 裁决：**本轮做**） | 🟢 已完成（欠债 0，退回对照 7 负例全红，不动点 3/3） |
| E | 规范冲突处置 + `design/language/generics.md` **原样迁入** book | ⬜ |
| F | 完整 GREEN + 文档同步 + 归档 | ⬜ |

## 阶段 0 —— 已完成的实测（DRAFT 依据，勿重跑）

- [x] 0.1 环境：`../z42-erasure` 基于 `origin/main` 54c8a1df，nightly SDK（63d7d5fb）冷种子供种，
      两轮构建波收敛，基线 `xtask test` 全绿（3m22s，`REAL_EXIT=0`）
- [x] 0.2 **P3 探针**（roadmap 字面写法）：分支 B 判 None → `build stdlib` 头 3 包即崩，
      33 条 E0402 **全是合法代码**，真欠债 0 ⇒ 条目前提不成立
- [x] 0.3 **P1 探针**（根因修法）：形参位代换 → 欠债 **0**（stdlib 25 库 + compiler 全量自建）
- [x] 0.4 三道对照全过：阳性（`l.Add(42)` 报 / `l.Add("ok")` 不报）、退回（基线只剩无关 E0436）、
      **真实构建面破坏性**（改坏 `Z42cReplCompiler.z42:172` → `build compiler` REAL_EXIT=1）
- [x] 0.5 **P4 普查**：隐式泛型调用全仓 **112 处，全部是 `Array.Copy<T>`**，其 `T` 不被运行期消费
      ⇒ 回灌 `MethodTypeArgs` 是纯回归（design D4）
- [x] 0.6 规范冲突已定位：`design/language/generics.md:380` vs book `generic-methods.md:109` vs 实现

## 阶段 A —— 泛型类实例方法形参位代换

- [x] A.1 `MemberResolver.z42`：新增 `_substGenericSig(sig, inst)`，镜像 `_substSelfSig`；
      **`ParamsFrom` / `ParamDefaults` / `ParamCallers` 原样搬运**（漏搬 → params 退化成定长）
- [x] A.2 `MemberResolver.z42` `Z42InstantiatedType` 分支：`_withDefaults` 之后追加一次
      `CheckArgTypes(fa, rawArgs, argCount, substSig, env)`。**`_withDefaults` 入参保持原签名**（不变式 I1）
- [x] A.3 确认方法级型参在 `_substGeneric` 下退化为 `Z42UnknownType`（`MemberResolver.z42:365`）
      → `Conversion` 分支 A Absorb → 放行 ⇒ 类级+方法级混合泛型不产生假红。已写成调用点注释留档
- [x] A.4 单测 6 条：`generic_inference_tests.z42`（自建 `bodyDiags`/`countCode`，**不用 `FirstErrorCode`**）
- [x] A.5 **真实构建面破坏性对照**：`Z42cReplCompiler.z42:172` 实参改成 `12345` →
      `xtask build compiler` `REAL_EXIT=1` + `E0402: cannot assign int to String` → 已改回
- [x] A.6 **同源退回对照**：只回退消费端一处（保留 `_substGenericSig`）→ 单独 `build compiler` →
      **3 条负例全 FAIL、3 条正例仍 PASS** ⇒ 负例是真门、正例不是靠改动才绿
- [x] A.7 `xtask test compiler` 全绿：**663 PASS / 0 FAIL**；
      **自举字节不动点 3/3 `gen1==gen2`**（z42c.semantics / z42c.pipeline / z42c.driver）
- [x] A.8 **欠债实测 = 0**（`build compiler` + `build stdlib` 全量，E0402/E0439 各 0 条）

## 阶段 B —— 显式方法类型实参形参位代换

- [x] B.1 `MemberResolver._applyMethodTypeArgs`：用已解析的 `targs` 按 `ms.Decl.TypeParams.Names`
      做**方法级**型参代换（新增 `_substByName` + `_checkSubstMethodArgs`），产出诊断专用签名
- [x] B.2 就地追加检查。**未调整 `_applyMethodTypeArgs` 与 `_withDefaults` 的执行顺序** ——
      本方法本就在 `_withDefaults` 之后跑，代换结果天然进不了发射决策
- [x] B.3 单测 4 条：自由函数 / 静态方法 / 数组形参位 / 正例
- [x] B.4 **同源退回对照**：只回退 B.2 一处 → `build compiler` + `test compiler` →
      **恰好 3 条阶段 B 负例 FAIL，阶段 A 的 6 条全部仍 PASS**（证明两阶段互不依赖）
- [x] B.5 `xtask test compiler` 全绿：**669 PASS / 0 FAIL**；不动点 3/3 `gen1==gen2`
- [x] B.6 **欠债实测 = 0**（`build compiler` + `build stdlib` 全量）

### 🔴 阶段 B 期间修掉的一个阶段 A 缺陷（务必留档）

阶段 A 初版对**代换后的整条签名**再调一次 `CheckArgTypes` ⇒ 形参是**具体类型**的位
（`void PutAt(T v, int i)` 里的 `int`）被报**两次**（同 span 同消息）——`_withDefaults` 内部
那次已经查过并报过了。实测复现：`b.PutAt("ok", "bad")` → 2 条一模一样的 E0402。

**根治** = 新增 `OverloadBinder.CheckSubstitutedArgs`，按位门控
`Conversion._hasGenericParam(orig.ParamTypes[i])` ⇒ **只补查原先被擦除放行的那些位**，零重复由构造保证。

⚠️ **教训**：我最初的 6 条用例**全是泛型形参位**，正好绕开了这个形状 —— 单测只覆盖了我设想的
那条路。已补两条回归（`test_concrete_param_position_reported_once_not_twice` /
`test_generic_and_concrete_both_bad_report_one_each`）。**新增检查时要问：它与既有检查的
覆盖面重叠吗？重叠处会不会报两遍？**

## 阶段 C —— 方法级类型实参推断

- [x] C.1 新建 `src/compiler/z42c.semantics/src/TypeArgInference.z42`：结构化 unify + `TypeArgBindings`
- [x] C.2 递归面与 `Conversion._hasGenericParam` 对齐（裸型参 / 数组元素 / 实例化实参 / func 形参·返回）；
      已注释留档**唯一有意的不对称**：`_substGeneric` 今天没有 `Z42FuncType` 分支 ⇒ func 位推得出绑定
      却换不进去（少换一次，不出错）
- [x] C.3 保守收口三条：未绑定 → 整体失败；冲突绑定 → 整体失败；`Unknown`/`Error` 实参位 → 跳过。
      另加一条：**params 尾位整段跳过**（规范形态与展开形态在推断处无法区分，误判会污染绑定）
- [x] C.4 `MemberResolver._applyMethodTypeArgs` 早退分支接线。推断成功 → `ConstraintChecker.CheckMethod`
      + `_checkSubstMethodArgs`；**不写 `bc.MethodTypeArgs`**（design D4）
- [x] C.5 单测 6 条（累计 18 条）
- [x] C.6 **欠债实测 = 0**（`build compiler` + `build stdlib` 全量）
- [x] C.7 **阳性对照**：`test_inferred_type_arg_violating_where_is_reported` —— `g(new D())` 违反
      `where T : IFoo` 现在报 E0402，**改动前零诊断**。这条同时是「推断通道真的通了」的证据
- [x] C.8 **同源退回对照**：把 `if (inf.Ok)` 改成 `if (false)` → **恰好 2 条负例 FAIL**
      （where 违反 / 形状不匹配），4 条正例与保守收口用例仍 PASS
- [x] C.9 `xtask test compiler` 全绿：**675 PASS / 0 FAIL**；不动点 3/3 `gen1==gen2`

### 诚实记账：阶段 C 的实参检查覆盖面比看上去窄

推断成功 ⇒ 各**裸 T 位**按构造必然一致（不一致就是冲突 → 整体失败 → 静默）。所以
`_checkSubstMethodArgs` 在推断路径上**只在「形状不匹配」的位**才报得出来
（`void h<T>(T a, T[] b)` 调 `h(1, 2)`：`T[]` vs `int` 推不出信息，T 由前一位定为 int
⇒ 代换后 `int[]` vs `int` → E0402）。

⇒ **阶段 C 真正的新诊断是 where 约束校验**，不是实参检查。这一点在 proposal/design 里写得
比实际乐观，此处更正。

### 无误报守卫 ≠ 真门

`test_conflicting_bindings_fail_inference_silently` / `test_uncovered_type_param_fails_inference_silently`
断言的是「**不**报诊断」，退回后照样绿 —— 它们守的是保守收口边界（防过度报错），**不是**
阶段 C 的真门。真门只有 C.8 里变红的那 2 条。（同 #530「4 真门全红 / 3 无误报守卫全绿」。）

## 阶段 D —— callee 消费型参时要求显式类型实参（可裁）

> ✅ User 裁决（2026-09-08）：**本轮做**。理由——不做就等于明知有静默错值洞而不堵，
> 且它是 design D4「不回灌 `MethodTypeArgs`」的配套：不回灌把语义责任转移给了这条诊断。

- [x] D.1 grep 既有码表 —— E0446–E0454 全被占用，**确需新造 E0455**（按 E0449–E0454 手法，
      语义层发**字面量**而非常量引用，避 core→semantics 新跨成员符号撞 F2 冷启动 stale-cache）
- [x] D.2 新建 `MethodTypeParamUse.z42`（**实施期追加进 Scope**，见下「偏离」）：完整 AST walker
- [x] D.3 判定不到（无本地 `Decl`）→ 放行；已写成注释留档「= 改动前行为，严格无回归」
- [x] D.4 单测 10 条（累计 28 条），含 walker 递归面的 3 条嵌套用例（控制流 / lambda / try-finally）
- [x] D.5 **欠债实测 = 0** —— 关键在于全仓 112 处 `Array.Copy` 调用**没有**被判红
- [x] D.6 **同源退回对照**：`if (Consumes(...))` → `if (false)` → **7 条负例全红**，
      3 条无误报守卫仍绿（显式写 `<T>` / 非消费 callee / 纯类型注解位）
- [x] D.7 `xtask test compiler` 全绿：**685 PASS / 0 FAIL**；不动点 3/3 `gen1==gen2`

### 🔴 实施期偏离：判定设施比 DRAFT 估计的重（已回阶段 3 更新 Scope）

DRAFT 假设能「复用现成入口的判据」。实测 **z42c 没有表达式遍历设施** —— `z42c.syntax` 只有
statement 级（`AnalyzerDriver._walkStmt`），而消费形态分散在 `TypeofExpr` / `ObjNewExpr` /
`DefaultExpr` / `ArrayNewExpr`，表达式面共 **36 个节点类**。按规矩停下来报告，User 裁决走
**D-1（补完整 AST walker）**。新增文件已追加进 proposal 的 Scope 表。

**覆盖基线（2026-09-08 从源码机械枚举，非凭印象）**：Expr 36 / Stmt 16 / Pattern 10 / TypeExpr 6。

**判定方向刻意过近似**（宁可多判「消费」）：多判只会要求用户显式写 `<T>`（响亮、可改），
漏判会产生静默错值（安静、难查）。同 #530「守禁令的一侧必须比替换的一侧更宽」。

**⭐ 第五种、最容易漏的消费形态**：`Foo<T>() { Bar<T>(); }` —— 把自身型参**转发**给嵌套泛型
调用（emit 期发 `$mta:<idx>`，运行期按调用方 frame 解析）。只盯 typeof/new/default/new[] 会漏掉它。

**⚠️ 一处差点写错**：Pattern 数组有显式 `Count` 字段而我最初用了 `.Length` —— 这些数组按增长
策略**超额分配、尾部是 null**。写遍历前先读清楚每个节点的计数字段。

### 🔴 诚实记账：walker 的完备性**没有自动门盯着**

覆盖是靠 2026-09-08 从源码机械枚举建立的，**AST 将来加新节点类不会让任何测试变红**。
这正是本程序反复吃亏的形状（「没有东西盯着的约定迟早会烂」）。
已登记 Deferred `ast-walker-completeness-gate`。

## 阶段 E —— 规范冲突处置 + 文档迁移

> ✅ User 裁决（2026-09-08）：取「**原样迁入 + 修正失效段**」，**不借机按 book 口径重写**
> （重写与本 change 主线无关，成本不相称）。

- [ ] E.1 `docs/book/src/language/generics.md` NEW：`docs/design/language/generics.md` 内容**原样**迁入
- [ ] E.2 修正失效段：`:380`「T 从实参推断」按本 change 落地后的真实语义改写；
      `:382 ### 限制（本阶段）` 补上此前漏列的边界
- [ ] E.3 `docs/design/language/generics.md` DELETE
- [ ] E.4 `docs/book/src/SUMMARY.md` 挂载新页；`docs/book/src/language/README.md:31` 迁移状态打勾
- [ ] E.5 grep 全仓对 `design/language/generics.md` 的引用，逐条改指 book 新页（清零）

## 阶段 F —— 验收

- [ ] F.1 `cargo build`（runtime 未改动，确认无连带破坏）
- [ ] F.2 `xtask test compiler`
- [ ] F.3 `xtask test e2e` + `xtask test e2e --dir cross-zpkg --mode jit`
- [ ] F.4 `xtask test stdlib --mode jit`（本地 GREEN 只跑 interp，派发面改动必补）
- [ ] F.5 `xtask test bootstrap`（先确认分支不落后 main，否则报错极具误导性）
- [ ] F.6 spec scenarios 逐条覆盖确认（`specs/generic-type-arg-inference/spec.md` 共 15 个 Scenario）
- [ ] F.7 文档同步（按 workflow 阶段 9 触发矩阵）：
      `book/language/generic-methods.md`（`:109` 改写）、
      `book/language/generic-constraints.md`（已知限制 §2 + `:58` 表格 + `:217-226` 形参位边界）、
      `docs/features.md`、所改页页头「对齐」日期刷新
- [ ] F.8 `docs/roadmap.md`：改写 `tighten-bare-type-param-target-erasure` 的根因与前置；
      关掉 `where-constraint-future-inferred-method-args` 与 `generic-methods-future-type-inference`；
      新增 4 条 Deferred（见 design 末表）
- [ ] F.9 归档：`git mv docs/spec/changes/add-generic-type-arg-inference docs/spec/archive/2026-09-XX-add-generic-type-arg-inference`
      —— **必须在开 PR 之前 commit 到本分支**，禁止合并后单独推 `docs: 归档`
- [ ] F.10 **最终态重跑一次完整 GREEN**（中途做退回对照重建过编译器 ⇒ 早先的绿不算数）

## 验收标准

1. 阶段 A/B/C 各自欠债经实测确认（非 0 则逐条核对真假，真欠债就地修、假红回设计）
2. 三道对照（阳性 / **真实构建面破坏性** / **同源退回**）每阶段都真跑
3. 自举字节不动点：`build compiler` 两轮 gen1 == gen2
4. `xtask test` 全绿（最终态重跑）+ jit 双补 + `test bootstrap` 绿
5. spec 的 15 个 Scenario 逐条有对应测试或明确的「本轮不做」标注
