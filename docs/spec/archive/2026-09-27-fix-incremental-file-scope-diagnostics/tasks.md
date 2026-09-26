# Tasks: 增量构建下 E0436 永不发射 + E0436 打绝对路径

> 状态：🟢 已完成 | 创建：2026-09-27 | 完成：2026-09-27 | 归档：2026-09-27
> 分支/worktree：`learn-ch21` @ `wt-learn19` | 基于：origin/main `e706be9bc`（#863）
> 类型：`fix`（编译器诊断投递；**无格式 bump、无新诊断码、不改 emit 字节**）
> User 已裁决：**A + B 一起修，B'（缓存身份）另立**

**变更说明：**
- **A**：把文件级 `using` 检查（E0436）从 per-file 并行体里移到 `UsedDepNs` 回填之后，
  让 **cached 文件也能发射 E0436**。
- **B**：E0436 的消息改用 `cu.Span.File`（与该文件其它诊断同源），不再打绝对路径。

**原因：**

**A** —— `IrDump.z42:48` 在 cached 分支构造 `CompiledModuleZ` 时把 `usedDepNs` 传成**空数组**，
而 `_enforceFileScope`（E0436）就在 `:55` 紧接着无条件执行 ⇒
`while (u < cm.UsedDepNsCount)` 一次都不进 ⇒ **任何 cached 文件永远不可能报 E0436**。
真正的回填在更晚的 pipeline 层（`PackageCompile.z42:268-278`），那时检查已经跑完。

⭐ 我最初的判断是「global using 的变化没参与失效传播」—— 那条**成立但不是决定性原因**
（`prelude.z42` 只有 `namespace` + 一行 `global using`、零定义名 ⇒ `_seedChangedNames` 为空 ⇒
不传播）。真正的根因是上面那条，**范围大得多：与 global using 无关，任何 cached 文件都中**。
⇒ **教训：「我的假说能解释现象」不等于「它是根因」—— 得沿调用链往上读到检查的输入端。**

**B** —— `CuPreprocess.z42:186` **手工拼字符串**塞进 `cm.DiagMsgs`，绕过了 `DiagnosticBag`；
其它诊断在 `IrDump.z42:362` 用 `dg.Span.File` 渲染（相对路径）。同目录同调用方式实测对照：
`E0401` → `cmp.z42(1,15)`（相对）／`E0436` → `/Users/.../noimport.z42(1,1)`（绝对）。
⇒ 学习手册第 21 章的 transcript 无法钉住 E0436（沙箱路径每次都不同），这是发现它的契机。

**当年没裁过这件事**：`docs/spec/changes/add-global-using/proposal.md` 整篇未提增量/缓存。
`:27-29` 写着「只读 `UsedDepNs` + 追加诊断，**不改任何 emit 字节**」—— 这句在全量路径成立，
**也正是它让人放心地没去看 cached 分支**。（另：`archive/2026-07-08-add-file-level-incremental`
里刻意延后的 `incremental-future-tsig-level-invalidation` 方向相反，管的是「少编」，与本缺陷无关。）

**文档影响：** `docs/reference/src/language/namespaces.md`（5 处不准，见下）、
`docs/internals/src/devinfra/build.md` §增量（补「cached 文件的诊断重放边界」）、
学习手册第 21 章（另一个 commit）。

## 任务

- [x] 1.1 **B** 完成。实测：同目录同调用方式下 `E0436` 从 `/Users/.../noimport.z42(1,1)`
      变成 `noimport.z42(1,1)`，与同文件的 `E0401`/`E0404`/`E0442` 一致
- [x] 1.2 **A** 完成：新增 `IrDump.EnforceFileScopeAll(cus, files, cms, n)` 公开入口
- [x] 1.3 **A** 完成。⚠️ 上界用 `srcCount` 而非 `cms.Length` —— 与紧邻的
      `ModuleInitScan.CheckExePackage(cus, srcCount, …)` 同口径；generator 路径下
      `cus`/`files`/`srcCount` 是三者一起被替换的（`:254` 的 `ro.Cus` 那一支），所以安全
- [x] 1.4 核实完毕：`BuildPackage` 只是 `BuildPackageCus` 的转发，**生产调用点只有
      `PackageCompile.z42:266` 一处**；其余调用者全是 semantics 单测。
      ⚠️ 按「先看存量测试会不会红」查过：**没有任何 semantics 单测断言 E0436**（只有注释提及），
      而依赖 E0436 行为的 `pkgcompile_tests.z42:649` 走 `PackageCompile.Compile` ⇒ 被新调用点覆盖
- [x] 1.5 阴性证据取的是**更强的形态**：假绿是我在**同一棵树、改动之前**亲眼跑出来的
      （删掉 global using 不清缓存 → 照旧编译运行；`rm -rf artifacts` 全量 → 正确报 E0436）。
      ⭐ 那一轮还顺手排除了「文件没被重读」的假说：往 `prelude.z42` 塞语法垃圾 → 正常报 E0103
- [x] 1.6 两条单测进 `z42c.pipeline/tests/pkgcompile/`：`test_global_using_covers_sibling_file`
      （prelude 写 `global using`、Main 一条 using 都没有 → `ErrorCount == 0`）+
      `test_missing_global_using_reports_e0436`（**只把 `global` 去掉** → 断言出现 E0436）。
      后者是前者的判别性对照 —— 没有它，前者在「E0436 整条不发火」的实现下也会绿。
      ⚠️ 必须给 `LibsDirs`：E0436 只对**跨包**依赖 ns 发火，没有真实依赖世界时 `Console`
      压根解析不到、报的是 E0401。
      另：学习手册第 21 章 `examples/types/organization/globalusing/` 是首份**端到端**覆盖
- [x] 1.7 文档同步完成（见下）

## 🔴 `global using` 目前零 e2e 覆盖（本刀要补）

全仓 grep `global using`：只在 `z42c.syntax/tests/stmt.z42` 的 **parser 单测字符串**里出现；
`src/tests/` 一个 golden 都没有。`add-global-using/proposal.md:53` 承诺过
「`examples/global_using/` + 跨文件 golden」，**两样都没建**。
⇒ 学习手册第 21 章的 `examples/types/organization/globalusing/` 是首份端到端覆盖。

⚠️ 定位调查曾报「`examples/types/organization/globalusing/` 只剩一个 z42.toml、src/ 空」——
那是它扫到了**我正在建的中间状态**，HEAD 上该目录并不存在。已核对 `git ls-tree` 撤回该说法。

## `xtask test incremental` 为什么照不到（三个独立原因）

`scripts/test/xtask_test_incremental.z42` 的变异算子只有三个：追加一行注释（`:357`）、
追加一个新自由函数（`:395`）、删 `dist/` 保留 cache（`:422-439`）。

1. **三个算子都不碰 using / global using 集**
2. **判据只比 dist 字节**（`_incrBuild` 把 stdout/stderr 都接到 `Stdio.Null()`，只取 exit code）
   —— 而本缺陷缺的恰恰是**诊断**，字节一模一样
3. **语料里根本没有 `global using`**（三份语料 demo / demo-packed / xtask 自身都没有）

⇒ **变异算子的覆盖面决定了门禁能抓什么。** 这是「写了但抓不到 bug」的又一个样本，
而且原因可指名道姓。

## 🆕 同源缺陷（**本刀不修**，登记后续）

`IrDump.z42:49` 把 cached 文件的 `DiagMsgs` 整体清空 ⇒ **cached 文件的警告在增量构建中
静默消失**（带 error 的构建不落 cache，所以只丢 warning）。与 A 同一处代码、另一半后果。
⇒ 候选后续 `replay-cached-file-warnings`。

## 🆕 `namespaces.md` 五处不准（写第 21 章逐段实跑核实时发现）

| 位置 | 文档说 | 实测 |
|---|---|---|
| `:13` | `namespace` 必须在所有 `using` **之前** | 🔴 **未强制**；`using` 在前照样编过，且 namespace **确实生效**（栈里是 `Late.Boom`）。⭐ 而且 parser 单测 `stmt.z42:231-232` 白纸黑字写着「using / global using / using alias **不算**声明：`namespace` 排在它们之后合法」⇒ **这是刻意设计且有单测的，文档写错了** |
| `:154` | `E0602`「有码无发射点，`using NoSuch.Pkg;` 静默通过」 | 🔴 **已实现**：报 `E0494`（#793 加的） |
| `:164` | 消息 `namespace declaration must appear before any top-level declarations` | 🔴 实际 `E0457: \`namespace\` must appear before any type or function declaration in the file`（措辞不同，表里也没给码） |
| `:165` | 消息 `duplicate namespace declaration` | 🔴 实际 `E0457: a file may declare only one namespace (\`A\` was already declared above) — split the file, or move all declarations under a single namespace` |
| `:166` | `using` 出现在顶层声明之后会报错 | 🔴 **未强制**，实测编过并运行 |

---

## 验证

- `xtask test all` → **✅ GREEN — all stages passed**（**16** stage，含 main 新加的 `stage2` 门）
- `cargo test --workspace --lib`（debug）→ **1442 passed / 0 failed**
- `xtask test examples` → 95 transcript / 153 步（第 21 章 7 条全新）
- 两个修复各自实测（见 1.1 / 1.5）

### 🔴 踩了一次「xtask 二进制过期」的新变体

第一轮 `xtask test all` **0 个 stage 就退出**，报「GREEN gate stage 清单与 test-gate.md 不一致」
（代码 15 项 / 文档 16 项）。**我两个文件都没动** —— 根因是复用同一棵 worktree 跨分支时
`git checkout -B learn-ch21 origin/main` 带进了 #862（`scripts/` 有改动），而 `xtask`
二进制还是上一章建的。`./.z42/z42 publish scripts/xtask.z42.toml` 重建后 16/16 全绿。

⭐ 已知纪律是「改完 `scripts/` 要重建 xtask」；**新变体是「源码没动、是分支动了」**。
判据：报错指名两个文件，而 `git status` 说我都没动 ⇒ 先怀疑二进制过期，用
`git show origin/main:<文件>` 确认它们在 main 上其实一致。已记入备忘录（第七类）。

📌 顺带给 #862 一句好评：它把这类问题挡在**构建波之前**（几毫秒），而不是让人等十几分钟
再从假红里反推。

## 阶段 9 文档同步（doc-system 三问）

1. **用户能看见吗？** ✅
   - `docs/reference/src/language/namespaces.md`：**5 处订正**（见上表）+ 对齐日期
   - 学习手册第 21 章 + `examples/types/organization/`（另一个 commit）
2. **下一个接手的人不读文档能看懂吗？** ✅ 两处就地注释承载了「为什么必须在回填之后调」
   与「为什么路径要取 `cu.Span.File`」；`docs/internals/src/devinfra/build.md` §增量
   补「cached 文件的诊断重放边界」
3. **目录结构 / 入口 / 依赖变了吗？** ❌ 未增删文件（`EnforceFileScopeAll` 是既有类的新方法）

**正交三处**：根 README ❌；`docs/roadmap.md` ✅（登记后续 `replay-cached-file-warnings` +
`incremental-gate-using-mutation`）；`docs/agent/rules/` ❌。
