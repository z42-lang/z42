# Tasks: fix-parse-diags-dropped（parse 诊断丢失 + 关键字占名恢复）

> 状态：🟢 已完成 | 创建：2026-07-19 | 完成：2026-07-19

**变更说明：** 修两个 z42c 前端缺陷：① parse 诊断被整体丢弃（各编译点只为 TypeChecker
新建 DiagnosticBag，Parser 的 bag 无人消费）→ 畸形 parse 的垃圾 AST 流入 codegen，
typecheck 恰好不报错时产出**静默 miscompile** 的包（实测：`report(string module, ...)`
零报错成功写包、方法被无声吞掉——`module` 是保留关键字，`_parseParamList` 名字位置
不消费关键字 token 致解析脱轨）；② 关键字占参数名/成员名位置时无精准诊断、恢复策略
（不消费）导致级联垃圾错误（"unknown type in `new`: ]" / "duplicate top-level function `=`"）。

**原因：** rebuild-bench-structured-output 实施期发现（原判「数组参数误解析 / 多参
brace-body 丢导出」两 bug，探针二分后收敛为同一根因：`module` 保留关键字 + parse 诊断
丢失）。root-cause 修复：诊断上浮 = 产出端修复（Parser 挂 bag 到 CU，7 个 typecheck
汇点合并）；关键字恢复 = 精准报错 + 消费 token 保持同步。

**文档影响：** 无外部行为/机制变更文档需求（错误恢复改进 + 诊断补漏；错误码复用
E0202 ExpectedToken，不新增码）。design 记录见
`docs/spec/archive/2026-07-19-rebuild-bench-structured-output/design.md` Decision 5 修订。

**子系统：** `compiler`（锁被 split-irgen-class 占用；User 授权短占预抢，隔离 worktree
`z42-bench` off main，归档即还）。

- [x] 1.1 `Decl.z42`：CompilationUnit 加 `ParseDiags` 字段 + `MergeParseDiags(bag)` helper
- [x] 1.2 `Parser.z42`：ParseCompilationUnit 收尾挂 `cu.ParseDiags = this._diags`
- [x] 1.3 `AttributeSynth.z42`：重建 CU 时传递 ParseDiags
- [x] 1.4 `IrDump.z42` ×3 + `SemanticDump.z42` ×4：typecheck bag 建立后 `cu.MergeParseDiags(diags)`
- [x] 1.5 `MemberParser.z42`：参数名/成员名位置关键字 → 精准诊断（"cannot use keyword 'X' as a
      parameter/member name"）+ 消费 token 防脱轨；`_isWordKeyword`（TokenKind 9..93 + true/false/params）
- [x] 1.6 验证：
      - bug 复现件 B.z42（`string module` 参数）：修复前零报错静默吞方法 → 修复后
        `E0202: cannot use keyword 'module' as a parameter name`，构建失败 ✓
      - u1.z42（module + Row[]）：级联垃圾 → 3 条合理错误（首条精准）✓
      - 阴性对照（`tag` 参数名）照常编过 ✓
      - `xtask build compiler`（修复版 z42c 编译自身）✓
      - `xtask test compiler`：单测全绿（各文件 0 failed）+ e2e 6/6 + **自举不动点
        7/7 gen1==gen2 byte-identical** ✓
      - stdlib canary：新 z42c 重编全 stdlib + z42.test 测试全绿 ✓
- [x] 1.7 完整 gate 以 CI 为权威（冷环境本地不可验完整自举链，沿用短占先例）

## 备注
- 「数组参数误解析 new」与「多参 brace-body 丢导出」两个先前判定的独立 bug 均为本根因
  的表象——`module` 关键字触发脱轨后的不同下游形态；修复后 Deferred
  `bench-structured-future-report-envelope` 的前置（z42c 修 parser）已达成，TestReport
  可在后续 change 收敛回 `report(TestResult[])` 自然 API（本次不动，避免再占 stdlib 锁）。
- `module` 保留关键字（TokenKind.Module=65，与 fn/let/mut/trait/impl/use/spawn 同批）
  本身保留与否是语言设计决策，不在本 fix 范围。
