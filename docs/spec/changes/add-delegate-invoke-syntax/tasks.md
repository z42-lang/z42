# Tasks: 委托值上的 `.Invoke(args)` 显式调用语法

> 状态：🟡 DRAFT 待 User 确认 | 创建：2026-09-26
> 分支/worktree：`learn-ch19` @ `wt-learn19` | 基于：origin/main `4ae522839`（#854）
> 类型：`lang`（按 workflow 词汇警报走完整流程 1–9）——
> **零 VM 改动 / 零格式 bump / 零新诊断码**（E0401 / E1005 / E1006 全部复用）
>
> 起因：学习手册第 19 章（Lambda、闭包与委托）写作前逐段实跑核实参考手册，
> 发现 `delegates-events.md` 教的单播 event 触发模板照着写必崩。
> 相关线：[[z42-learn-book-line]]

## 进度概览

- [ ] 阶段 1: `MemberResolver` 加 `Z42FuncType` 分支（`.Invoke` → `BoundIndirectCall`）
- [ ] 阶段 2: 非 `Invoke` 成员名报 E0401
- [ ] 阶段 3: arity 校验（E1005 / E1006，三个产出点，型参分支除外）
- [ ] 阶段 4: **摸底** —— 全仓有多少站点今天靠「多传被静默丢掉」活着 + 指纹是否真的零 bump
- [ ] 阶段 5: e2e / golden（含 D4 的两条阴性对照）
- [ ] 阶段 6: 文档（`delegates-events.md` 四处谎报 + `closures.md` 复核 + 学习手册第 19 章）
- [ ] 阶段 7: GREEN + PR

## 阶段 1：`.Invoke` 脱糖

- [ ] 1.1 `MemberResolver.z42`：在 `:299` 数组分支后、`:311` fallthrough 前插入
      `rt is Z42FuncType` 分支；`mem.Name == "Invoke"` → `BoundIndirectCall(recv, args, argCount, ft.Ret, sp)`
- [ ] 1.2 确认返回类型取 `ft.Ret` 而非 `Z42UnknownType`（此前走 prim 兜底拿到 Unknown，
      导致 `int r = f.Invoke(1)` 这类赋值也不被检查）
- [ ] 1.3 四种载体各一条探针：局部变量 / 普通字段 / `event` 字段 / 数组元素

## 阶段 2：E0401

- [ ] 2.1 非 `Invoke` 成员名报 E0401，措辞点明「on delegate type `<名>`」
- [ ] 2.2 **不套用** prim 那条 stub 豁免（委托成员面是语言固定的，见 design D2）
- [ ] 2.3 `f.Invoke` 不带括号 → 报诊断（Out of Scope 项，须有用例钉住）

## 阶段 3：arity 校验

- [ ] 3.1 抽一个共用校验（`argCount` vs `ft.ParamCount`），少报 E1005 / 多报 E1006
- [ ] 3.2 接到三个 `BoundIndirectCall` 产出点：`MemberResolver.z42:588-594`、`:661-673`、
      阶段 1 新增分支
- [ ] 3.3 🔴 **型参分支 `:605-609` 不接**（返回 Unknown、擦除后形参表不可信 ⇒ 会误报）
- [ ] 3.4 确认 `params` / 默认值在函数类型上不存在（若存在，校验要放行）

## 阶段 4：摸底（**先摸后做的那一步，不可省**）

- [ ] 4.1 全仓统计今天有多少调用点靠「多传被静默丢掉」活着 —— 先加临时计数探针再跑
      `xtask test`，**命中非零就先报 User**，不得自行放宽判据
- [ ] 4.2 跑**不带过滤**的 `cargo test --lib`（debug，`--workspace`）——
      约 1400 条含多道棘轮对账，且不在 `xtask test` 的 15 个 stage 里
- [ ] 4.3 手动判指纹：`fingerprint` 门不在 `xtask test` 默认档（要 `--base` 参考树，只有 CI 跑）
- [ ] 4.4 存量测试优先于新测试：**先看存量会不会红**（#833 的教训）

## 阶段 5：测试

- [ ] 5.1 e2e：四种载体 × `.Invoke`；有/无返回值；多参数
- [ ] 5.2 诊断用例：E0401（`f.Bogus()`）/ E1005 / E1006 / `f.Invoke` 不带括号
- [ ] 5.3 🔴 阴性对照一：多播 `MulticastAction<T>.Invoke` 行为逐字不变
- [ ] 5.4 🔴 阴性对照二：反射 `MethodInfo.Invoke` / `ConstructorInfo.Invoke` 不被劫持
- [ ] 5.5 阴性对照做法 = **撤掉修复本身**再跑（不是改期望值）
- [ ] 5.6 ⚠️ 写优化/发射类用例记得挂 `opt_all` sidecar（golden 默认优化集关掉一批 pass）

## 阶段 6：文档

- [ ] 6.1 `docs/reference/src/language/delegates-events.md`：`:61` 的「等价」现在为真；
      `:62` 与 `:264-266` 的单播触发模板终于能跑 —— 对齐日期更新
- [ ] 6.2 `docs/reference/src/language/functions.md:283`：「函数值取出后不能就地调用」
      **边界记宽了** —— 实测**数组下标可以**就地调（stdlib 自己在用
      `MulticastAction.z42:122` `snapStrong[i](arg)`），只有 `List` 索引器不行。订正
- [ ] 6.3 `docs/reference/src/language/functions.md`：补「函数类型不能做数组元素类型」
      （`((int) -> void)[]` 连声明都不认，报 E0401）
- [ ] 6.4 补一条两页都没写的事实：`(T) -> R` 与 `Func<T,R>` **双向可互赋**
      （根因 `Z42FuncType.IsAssignableTo` 走结构比较、不看名字）
- [ ] 6.5 `docs/reference/src/appendix/error-codes.md`：E0401 / E1005 / E1006 三条词条
      各补「委托接收者」这一格
- [ ] 6.6 学习手册第 19 章据此落地（User 已裁决：**完整覆盖，含多播全套**）

## 阶段 7：GREEN + PR

- [ ] 7.1 `xtask test`（15 stage）+ `cargo test --lib` + `xtask test examples` + `test docs`
- [ ] 7.2 ⚠️ 改编译器后必须 `xtask build compiler` → `build sdk`，否则门禁验的是旧二进制
- [ ] 7.3 PR（并入 main 最新改动 + 重跑 GREEN）
- [ ] 7.4 🔴 **归档必须在 PR 内**（workflow 阶段 9 铁律，本仓已违反过一次）

## 并入本变更首个 commit 的归档（workflow 阶段 0）

粗扫 `docs/spec/changes/`，两个容器标 🟢已完成 且未勾项均为「PR/CI 盯着」这类流程项
（PR 早已合并）⇒ 归档：

- [ ] `fix-crosspkg-static-ns-collision`（🟢 完成 2026-07-16）
- [ ] `refactor-interp-boilerplate`（🟢 完成 2026-08-24，PR #285）

**不归档** `z42b-owns-test-targets` —— 6 条未勾全是「（刀二）」，是刻意未动的第二刀，
标题也只说「已完成（刀一）」；归了等于把刀二的计划埋掉。
