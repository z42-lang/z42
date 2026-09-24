# Tasks: `switch` 表达式无匹配臂时抛异常

> 状态：🟢 已完成（#760 已合）| 创建：2026-09-22 | 归档：2026-09-23
>
> ⚠️ 归档**晚了一步**：#760 合并时没带上归档，违反 workflow 阶段 9 铁律
> 「归档必须在 PR 内」。按阶段 0「扫描可归档变更」并入下一个 change 的首个 commit 补上
> （而不是单独推一条 `docs: 归档` 到 main —— 同一条铁律禁止那样做）。
> 分支/worktree：`fix-switch-expr-no-match` @ `wt-swxfail` | 基于：origin/main `18a64904e`（#756，rebase 后）
> 类型：`lang`（**无 zbc·zpkg 格式 bump** —— 编译期合成，全用现有指令；**VM 一行不改**）
> User 裁决已收到：① 运行期抛（非产默认值、非升级 W0700）② 用新建的 `Std.SwitchExpressionException`

> ⚠️ **基线注意**：`origin/main` 当前 **CI 红**，与本变更无关 ——
> #755（砍 `??`）漏了 `scripts/build/xtask_golden_assets.z42:45`，被自己的 E0480 拒掉 ⇒
> `xtask.zpkg not produced` ⇒ compile-toolchain / verify-selfhost / 四个 OS 的 test-host 全红。
> 已有 `fix-scripts-null-coalesce` / `fix-remaining-null-coalesce-usages` 两个 PR 在修（别的会话）。
> ⇒ 本变更 GREEN 判定时这条记为 **pre-existing**；合并前按 parallel-development 并入 main 最新改动重跑。

## 进度概览
- [x] 阶段 1: `SwitchExpressionException` + `CompilerFingerprint`
- [x] 阶段 2: `ExprEmitter._emitToStr` 抽出（插值与本变更共用）
- [x] 阶段 3: `_emitSwitchExpr` 落空点发 new + throw
- [x] 阶段 4: **摸底** —— 结论：一条都没真被执行（5 个 golden 站点都是真穷尽）
- [x] 阶段 5: e2e 用例（12 条，含 5 条阴性对照）
- [x] 阶段 6: examples + 三书文档
- [~] 阶段 7: GREEN 全绿；PR 待开

## 阶段 1：异常类 + 指纹
- [ ] 1.1 `src/libraries/z42.core/src/Exceptions/SwitchExpressionException.z42`
      —— 与 `InvalidCastException.z42` 同形（`: Exception`，一个 `(string message)` ctor，`ToString` override）
- [ ] 1.2 `CacheStore.CompilerFingerprint` 8 → 9 + 原因注释（照既有行格式追加）
      > 依据：`CacheStore.z42:6`「codegen/优化/typecheck 变化即令旧条目作废」。本变更改的是
      > **lowering**，含不穷尽 switch 表达式的用户文件源码哈希不变而发码变 ⇒ 必须 bump。
      > （#746 的 1.2 判「不需要」是对的——它只改运行期 Rust，编译器一行没动。）
- [ ] 1.3 CI 的 `guard-compiler-fingerprint` 门会独立复核这个判断，红了即说明判反了

## 阶段 2：`_emitToStr` 抽出
- [ ] 2.1 `ExprEmitter` 新增 `internal TypedReg _emitToStr(TypedReg reg, Z42Type t)`，
      内容即 `_emitInterpolation` hole 分支现有的三分支（`:296-317`）：
      enum → `_boxEnumForStr`；blob struct → `_emitStructToStr`；其余 → `ToStrInstr`
- [ ] 2.2 `_emitInterpolation` 改调它 —— **必须 byte-identical**（纯提取，不改行为）
- [ ] 2.3 验：`xtask test compiler` 的 zbc golden hex 单测 + 任一含插值的 golden 不变

## 阶段 3：落空点发射
- [ ] 3.1 `OperatorEmitter._emitSwitchExpr`（`:149-186`）把 `if (!Ended) { Br(endL) }` 改为
      `ConstStr` → `_emitToStr(subj)` → `StrConcat` → `ObjNew` → `EndBlock(ThrowTerm)`
- [ ] 3.2 ctor FQ 名用 `"Std.SwitchExpressionException.SwitchExpressionException$1"`
      （规则见 `IrInstrObject.z42:54`；`$1` = 一个形参）
- [ ] 3.3 消息串 = `"switch expression did not match any arm; value: "`（**不含源码路径**，D4）
- [ ] 3.4 **阴性对照**：有 `_ =>` / `default` / 裸绑定兜底臂时，编译产物必须 byte-identical
      > 这条是「pass 真的接通了吗」的判据。⭐ 记忆教训：「全量零命中」不能证明改动生效，
      > 只有阴性用例能分辨（见 `z42-definite-assignment`）。所以**正反两侧都要钉**。
- [ ] 3.5 `--dump-ir` 人工看一眼落空块的指令序列

## 阶段 4：摸底（全仓跑，看谁的落空路径真被执行）
> 静态扫描已做完（proposal 的影响面表）：产品代码 0 个、e2e golden 5 个、examples 4 个。
> 这一阶段要回答的是**动态**问题：那 9 个站点里，落空路径有几条真被执行到。
- [ ] 4.1 `src/tests/pattern-matching/{pattern_core,pattern_exhaust_sealed,pattern_generic}.z42`
      逐个跑，记录哪些现在抛了
- [ ] 4.2 真抛的：判读该用例原本在断言什么 —— 若它一直在验一个垃圾值，改成验异常
- [ ] 4.3 `xtask test examples` 逐条实跑，收集变红的例子
- [ ] 4.4 结论写回 proposal 的影响面表

## 阶段 5：e2e 用例
- [ ] 5.1 `src/tests/types/switch_expr_no_match/` NEW，覆盖 spec 全部场景：
      int / bool / string 三形态落空、守卫全假、`catch (Exception)` 抓得到、
      `GetType().FullName` 对、**`Message` 非空且含落空值**（钉 `CtorKnown`）
- [ ] 5.2 阴性对照用例：有兜底臂 → 不抛、值正确
- [ ] 5.3 switch **语句**无匹配 case → 不抛（钉住「只改表达式形态」）
- [ ] 5.4 **interp + jit 双模式**都跑
      > ⚠️ 记忆教训：golden 只走 `--emit-zbc`，其默认优化集关掉 Inline/PureCall/DeadBranch 等；
      > 本变更会影响内联与纯度判定 ⇒ **必须挂 `opt_all` sidecar**，否则等于没测
      > （见 `z42-golden-opt-gate-gap`）
- [ ] 5.5 ⚠️ 用例里转换/求值结果**必须被使用**，否则 DCE 消掉整条指令、用例变摆设
      （见 `z42-hard-cast-never-throws`）

## 阶段 6：文档
- [ ] 6.1 `examples/types/patterns/exhaust/missing.z42` + `closed.z42` 改写成「示范异常」，
      重生 `run.console`
- [ ] 6.2 `docs/learn/src/types/patterns.md` §「穷尽性」（`:52-75`）+ 章末要点（`:204-205`）
      改写 —— 当前把「静默产 null / 垃圾值」当规则写着
- [ ] 6.3 `docs/reference/src/language/pattern-matching.md`：switch 表达式无匹配语义
- [ ] 6.4 `docs/reference/src/appendix/error-codes.md` 的 W0700 词条补一句
      「表达式形态在运行期抛 `SwitchExpressionException`」
- [ ] 6.5 reference 的异常清单页加新类；**顺带记「含不穷尽 switch 表达式的函数不可内联、判非纯」**
      （`IrInline:316` / `IrPureFunctionTable:62`）
- [ ] 6.6 doc-system「三道门」的门③ doc-check 清单逐项核对

## 验证期踩到的坑（留给下一个改 emitter 的人）

- 🔴 **自举不动点第一轮的红是假的**：`z42c.semantics` gen2 比 gen1 大 69 字节。
  真因是「我在上一次 `build compiler` 之后又改了源码」⇒ `artifacts/` 里的编译器落后于源码，
  gen1 由旧编译器编、gen2 由 gen1 编。**重建 compiler 后第二轮 3/3 全绿。**
  ⇒ 规矩：**改完 emitter 必须先 `build compiler` 再看不动点**，否则那条红只在告诉你
  「你刚改过源码」。（形态同 `bootstrap-seed` 的种子漂移，但触发原因不同。）
- 🔴 **用例全过时 stdout 是空的、exit 0 —— 这跟「断言压根没跑」长得一模一样。**
  必须做阴性对照：故意把一条期望改错，确认打出 `TestFailure` 且 exit 1。
- **供种**：从别的树拷来的 `xtask` apphost **自带旧 VM**（报
  `zpkg minor 49 not supported (writer is at 0.43)`）。用当前树的
  `artifacts/build/runtime/release/z42vm` 直接跑 `artifacts/xtask/xtask.zpkg`。
- **`xtask` 内部调 cargo 用默认 rustc（1.88 < MSRV 1.95）会静默失败还 exit 0** ⇒
  所有 xtask 命令前挂 `RUSTUP_TOOLCHAIN=1.98.1`。
- **z42 的模式语法不是 C#**：绑定用 Rust 裸标识符（**无 `var`**）、守卫用 **`if`**（不是 `when`）。
  照 C# 写用例会得到 `E0202: expected pattern` + 一串级联错误。

## 阶段 7：GREEN + PR
- [ ] 7.1 `xtask build stdlib` + `build compiler` + `test compiler`（自举字节不动点）
- [ ] 7.2 `xtask test all` + `cargo test --lib`（**debug，不加 `--release`** —— 见
      `z42-cargo-test-signal-helper-wedge`）
- [ ] 7.3 `xtask test bootstrap`（越界检查：上一 nightly 仍能编当前源）
      —— 预期过，因为编译器源码不引用新异常类（D5）
- [ ] 7.4 并入 origin/main 最新改动 + **在新基线上重跑 GREEN**
      （⭐ 记忆教训：cherry-pick 换基后只在旧基线绿过 = 测的不是要合的东西）
- [ ] 7.5 PR（body 写跑 GREEN 时的 `base: <sha>`）
