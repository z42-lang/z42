# Tasks: 引用类型的空检查标记

> 状态：🟡 实施中（阶段 6.5 gate 已过——User「请开始推进，持续实现，pr变绿自动合并」） | 创建：2026-09-21
> 分支/worktree：`ref-null-model` @ `/Users/d.s.qiu/Documents/z42-lang/wt-refnull`（基于 origin/main 187200c61 #748）
> 类型：`lang` —— 完整流程（阶段 1–9）
> **依赖**：`enforce-value-type-non-null`（定下 `?` 只适用引用类型）；流分析设施（建议先 `add-definite-assignment`）
> **不依赖** `simplify-ref-parameters`

## 进度概览
- [x] 阶段 0: User 审批 + Q1 实测计划确认
- [x] 阶段 1: A —— 流分析引擎（**`add-definite-assignment` 已建好设施**，本变更只加第二套事实）
- [ ] 阶段 2: B —— `Expect("理由")`
- [~] 阶段 3: C —— 标记 → MaybeNull，义务点全开（**PR 1 交付形参那半**，返回值/字段待后续）
- [ ] 阶段 4: Q1 实测（字段窄化 (a) vs (b)）
- [ ] 阶段 5: D —— 反向推导 + override 一致性
- [ ] 阶段 6: E —— 砍 `?.` 与 `??` + 全仓迁移
- [ ] 阶段 7: 自举 + GREEN
- [ ] 阶段 8: 文档同步 + 归档

### PR 切分（实施时定的）

原计划是「引擎 → `Expect` → 开闸」。实际上 `add-definite-assignment` 已经把引擎建好了
（结构化数据流 + 正常结束分析 + join 规则），所以改按**义务点的来源**切，一次开一个口子：

| PR | 范围 | 状态 |
|---|---|---|
| **1** | 标 `?` 的**形参** + 义务点 + 窄化（E0478） | 本 PR |
| 2 | 标 `?` 的**返回值**（要在调用点拿被调方签名，`BoundCall` 有 46 个构造点 → 按 `MethodTypeArgs` 的 post-construction 手法只改重载解析处）+ `Expect("理由")` | 待做 |
| 3 | 字段（先做阶段 4 的 Q1 实测定快照规则） | 待做 |
| 4 | 反向推导 + override 一致性 | 待做 |
| 5 | 砍 `??`（生产 69 处）与 `?.`（3 处） | 待做 |

**为什么 `Expect` 能推到 PR 2**：原计划要它「在开闸前落地，否则用户没有逃逸口」。
形参那半的逃逸口是**窄化**（`if (s != null)` / 早返回守卫），已经够用——
全仓 7 处引用类型 `?` 标注里没有一处需要 `Expect`。返回值那半才真的需要它
（调用结果没有名字可窄化），所以它和 PR 2 绑在一起才对。

---

## 阶段 0: 审批
- [x] 0.1 User 审批 proposal.md + specs/null-check-marks/spec.md + design.md
- [x] 0.2 确认流分析设施来源：**先做 `add-definite-assignment`**（已合并 #750，`FlowAnalyzer.z42`）
- [x] 0.3 User 明确「可以开始」→ 阶段 6.5 gate 通过

## 阶段 1: A —— 流分析引擎（零误报验证）
> 只报「亲眼看见被赋成 null 的值被解引用」。**任何误报都算引擎 bug，不算规则太严。**
- [x] 1.1 事实格 `NotNull` / `MaybeNull` / `Null` / `Unknown`（未约束泛型 `T` → Unknown）
- [x] 1.2 事实表：键 = 局部变量 / 形参（**字段不入表**，见 design §D4）
- [x] 1.3 结构化数据流（无 `goto` ⇒ 不建 CFG，按 `BoundStmt` 递归）
- [x] 1.4 「语句是否正常结束」分析（早 return/throw/break/continue）
- [x] 1.5 join 规则：if 两分支 / while（循环前与循环尾的 meet）/ switch / **try-catch（catch 入口取 try 区间所有点的 meet）**
- [x] 1.6 窄化：`== null` 早退 / `!= null` / `is` / `&&`、`||` 短路 / 三目 / 赋非空
- [x] 1.7 全仓跑，**命中必须全部为真阳性**；有误报 ⇒ 停下修引擎
  - 首次构建命中 1 处：`String.IsNullOrEmpty` 的 `value == null || value.Length == 0`。
    **是误报** ⇒ 按 D3 补了短路求值的窄化（`||` 右操作数只在左边为假时求值），不是放宽规则。
  - 补完后全仓 25 包零命中（清空 `.cache` 后重跑确认——**带缓存的「零命中」不作数**）。

## 阶段 2: B —— `Expect("理由")`
> 必须在阶段 3 开闸**之前**落地，否则用户没有逃逸口。
- [ ] 2.1 intrinsic 绑定：唯一允许作用在 MaybeNull 值上的成员访问
- [ ] 2.2 参数校验：必须给、必须是字符串字面量 → 否则 `ExpectRequiresLiteralReason`
- [ ] 2.3 发射：运行期真检查，为 null 则抛，消息 = 理由 + 源位置
- [ ] 2.4 结果事实置 NotNull

## 阶段 3: C —— 标记 → MaybeNull
- [x] 3.1 `SymbolTable.z42:590` 引用类型分支：仍返回 inner 类型（类型身份不变），沿途记标记位
  - 实施时改成在 `SymbolCollector._methodSymbol` 直接读 AST（`md.Params[i].Type is NullableType`），
    比在擦除点串标记位简单，且**不动类型身份**——`ParamIsNullable` / `RetIsNullable` 与
    `ParamIsRef` 一样是 `Z42FuncType` 的旁路数组，不进 `Name()` / `Dump()`。
- [~] 3.2 `MethodSymbol` 每形参一位 + 返回值一位；`FieldSymbol` 一位
  - 形参位 + 返回值位已加（`Z42FuncType.ParamIsNullable` / `RetIsNullable` / `IsNullableParam(i)`）；
    **返回值位本 PR 只存不用**（用它要在调用点拿签名，见 PR 2）。字段位留给 PR 3。
- [ ] 3.3 跨包：符号加载侧从签名文本的 `?` 解析回标记位（`TsigTypeName` / `StubEmitter` 已双向拼写）
- [~] 3.4 义务点全开（解引用七类 + `return` 传播）→ **E0478**
  - 已覆盖：成员访问 / 下标 / 方法调用接收者 / `foreach` 集合 / `throw` 操作数（均限**裸名**）
  - 未覆盖：`return` 传播（PR 2，依赖返回值标记）
- [ ] 3.5 标 `?` 的字段直接解引用 → `NullableFieldNotSnapshotted`（消息给快照写法）
- [ ] 3.6 跨包用例（参考 `tests/cross-zpkg/`）

## 阶段 4: Q1 实测 —— 字段窄化规则
> 在引擎可用之后、全仓开闸之前做。
- [ ] 4.1 实现 (a) 强制快照，跑全仓，统计命中数与形态
- [ ] 4.2 实现 (b) 调用即失效，同样统计
- [ ] 4.3 对比：(b) 是否出现「检查完写句日志就要重检」的不可预测形态
- [ ] 4.4 结论写回 proposal Q1 + design §D4；按结论定稿

## 阶段 5: D —— 反向推导 + override
- [ ] 5.1 函数体 `return null;` 字面量但返回类型未标 → `UnmarkedNullReturn`
- [ ] 5.2 全仓跑，按提示补标；记录补了多少处（覆盖率指标）
- [ ] 5.3 override / 接口：返回可去 `?` 不可加；形参可加不可去 → `NullableOverrideMismatch`

## 阶段 6: E —— 砍 `?.` 与 `??`
> `?.` 与 `??` 都**不依赖** `simplify-ref-parameters`，也不依赖彼此。
> `?.` 成本≈0，可随时单独拆出；`??` 须在阶段 2（`Expect`）之后。
- [ ] 6.1 `?.`：删 `ExprTyper.z42:385` 脱糖 + `Ast.z42:227` 节点 + 词法/语法；删 `tests/control_flow/null_conditional.z42`
- [ ] 6.2 `?.`：确认 android/ios appbuilder 里生成的 Kotlin/Swift 源串**不受影响**（那是字符串内容）
- [ ] 6.3 `??`：删 `OperatorEmitter.z42:231` 发射 + `ExprTyper.z42:551` 绑定 + 词法/语法
- [ ] 6.4 `??` 迁移 —— 生产 69 处：compiler/z42c.pipeline 23、z42c.semantics 19、z42c.driver 5、z42c.syntax 4、z42.scripting 4、z42.project 4、z42.core 4、其余 6
- [ ] 6.5 `??` 迁移 —— 测试 42 处；`tests/control_flow/null_coalesce.z42` 删除
- [ ] 6.6 `ObsoleteNullOperator` 诊断带迁移写法
- [ ] 6.7 全仓 grep 清零

## 阶段 7: 自举 + GREEN
- [ ] 7.1 按 `bootstrap-seed.md` 走冷种子（编译器自身改了 47 处 `??`）
- [ ] 7.2 `xtask build` + `xtask test all` 全绿；`cargo test -p z42 --lib`（**debug，不能 `--release`**）
- [ ] 7.3 golden 核对
- [ ] 7.4 确认无 zbc/zpkg 格式 bump（`?` 只在签名文本里，已有编码）
- [ ] 7.5 推 PR 过 CI

## 阶段 8: 文档 + 归档
- [ ] 8.1 `docs/reference/src/language/*` —— 空检查规则 / 义务点清单 / 窄化清单 / 字段快照 / `Expect`
- [ ] 8.2 **命名把关**：全文用「空检查」，**不得出现「空安全」**；显式写明不健全与会漏的形态
- [ ] 8.3 版本语义表（返回加 `?` 破坏调用方 / 形参加 `?` 破坏实现方 / 去 `?` 永远安全）
- [ ] 8.4 `docs/roadmap.md` —— 可空线的真实进度
- [ ] 8.5 归档

---

## 明确不做
- **悲观档**（所有引用类型强制检查，不看标记）—— 实测全仓 49275 个解引用点 / 974 处现存检查，
  悲观档需新增 8000~19000 处守卫（代码量 +8%~19%），而历史证据不支持（18 个 null 提交里绝大多数是 GC/运行期的）
- 运行期 `NullReferenceException` 位置信息改善 —— 正交且更便宜，独立 change
