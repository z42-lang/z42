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
- [~] 阶段 3: C —— 标记 → MaybeNull，义务点全开（形参 #752 / 返回值 #753 / **字段本 PR**；余 3.3 跨包 + 3.6 跨包用例）
- [x] 阶段 4: Q1 实测（字段窄化 (a) vs (b)）—— **取 (a)**
- [ ] 阶段 5: D —— 反向推导 + override 一致性
- [ ] 阶段 6: E —— 砍 `?.` 与 `??` + 全仓迁移
- [ ] 阶段 7: 自举 + GREEN
- [ ] 阶段 8: 文档同步 + 归档

### PR 切分（实施时定的）

原计划是「引擎 → `Expect` → 开闸」。实际上 `add-definite-assignment` 已经把引擎建好了
（结构化数据流 + 正常结束分析 + join 规则），所以改按**义务点的来源**切，一次开一个口子：

| PR | 范围 | 状态 |
|---|---|---|
| **1** | 标 `?` 的**形参** + 义务点 + 窄化（E0478） | ✅ #752 已合 |
| **2** | 标 `?` 的**返回值** + `PossibleNullReturn`（E0479） | ✅ #753 已合 |
| 2b | `Expect("理由")` | 待做（见下） |
| **3** | 砍 `?.`（E0480） | ✅ #754 已合（先于字段那批——它只有 3 处、且带一个真 bug） |
| **5** | **字段 / 属性（E0484）+ Q1 实测定稿** | 本 PR |
| 6 | 反向推导 + override 一致性 | 待做 |
| 7 | `Expect("理由")`（emitter 要合成 `new` + `throw`，无先例） | 待做 |
| **4** | 砍 `??`（100 处） | ✅ #755 + #756 已合 —— 与 `?.` 同码 E0480 |

**`Expect` 又往后挪了一格，理由变了**：原以为「返回值那半需要它，因为调用结果没有
名字可窄化」。实际上逃生口是**「先存进局部再检查那个局部」**，完全够用且更直白
（`string v = C.Find(k); if (v != null) { … }`）。`Expect` 真正不可替代的场合是
**砍 `??` 之后**——那时才会出现「我确知非空但无处安放这次检查」的形态。
所以它挪到紧邻 PR 5（砍 `??`）之前，design §D6 的「Expect 先落地再砍 `??`」顺序不变。

另外 `Expect` 的实现成本被低估了：它要在 emitter 里**合成 `new` + `throw`**，
而那条路在 z42c 里没有先例（同 `switch` 无匹配臂那个问题）。单独一个 PR 更合适。

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
  - 形参位 + 返回值位已加（`Z42FuncType.ParamIsNullable` / `RetIsNullable` / `IsNullableParam(i)`），
    **三者均已接通**；字段位 = `FieldSymbol.IsNullable`（本 PR）。
  - 返回值位到调用点靠 `BoundCall.RetIsNullable`（post-construction，经 `BoundCall.Marked`
    在 **20 个手里有已解析签名的构造点**写入，其余构造点不动）。
    ⚠️ 考虑过让 FlowAnalyzer 自己按 `(OwnerClass, RegKey)` 反查符号表（只改一处），
    **否决**：那等于再实现一遍重载解析，查错就是**误报**（D3 零容忍）；
    显式接线漏点只会漏报（D4 允许），方向安全。
- [ ] 3.3 跨包：符号加载侧从签名文本的 `?` 解析回标记位（`TsigTypeName` / `StubEmitter` 已双向拼写）
  - ⚠️ **字段那半还差一截**：字段的 TSIG 拼写走 `SurfaceTypeName(ft)`（已解析类型，`?` 已擦除），
    `MemberCollector` 与 `ImportedSymbolLoader` 两侧都是。导入字段一律 `IsNullable = false`
    ⇒ **漏报**方向，D4 允许；但要补的话得动字段拼写，那会改 TSIG 文本、牵扯
    `AddOwnField` 喂的继承字段展开与 struct 布局判别，**不该顺手塞进本 PR**
- [~] 3.4 义务点全开（解引用七类 + `return` 传播）→ **E0478** / **E0479**
  - 解引用：成员访问 / 下标 / 方法调用接收者 / `foreach` 集合 / `throw` 操作数
    （限**裸名**与**调用结果**）
  - `return` 传播：**E0479**，可空值不得从未标 `?` 的返回类型漏出去（**字段也算来源**，本 PR 接通）
  - 未覆盖：跨包字段标记（3.3）
- [x] 3.5 标 `?` 的字段直接解引用 → **E0484** `NullableFieldNotSnapshotted`（消息给快照写法）
  - `FieldSymbol.IsNullable` 旁路位（与 `Z42FuncType.ParamIsNullable` 同构，不进类型身份），
    `MemberCollector` 直接读 AST（`fd.Type is NullableType`）；**属性同样置位**——D4 的第一条
    理由（「会被读两次」）说的正是属性
  - 标记**从接收者的类型上查**（`m.Target.Type()` → `Z42ClassType.Fields`），于是 `this.F` 与
    `other.F` 走同一条路，不必把 enclosing 类当参数串下来
  - **快照**（`var v = this.F;`）把义务转移到局部 → 复用形参那半的窄化机器；这条不是优化，
    是字段标记的**唯一逃生口**
  - `return this.F;` 漏给未标 `?` 的返回类型仍由 E0479 接住
- [ ] 3.6 跨包用例（参考 `tests/cross-zpkg/`）

## 阶段 4: Q1 实测 —— 字段窄化规则
> 在引擎可用之后、全仓开闸之前做。
- [x] 4.1 实现 (a) 强制快照，跑全仓 → **51 处**，形态单一
- [x] 4.2 实现 (b) 调用即失效，同样统计 → **7 处**
- [x] 4.3 对比 → **(b) 的 7 处全是那个不可预测形态**：`LinkedList:85` 相邻两行同一表达式
      一行报一行不报（中间那次调用只是把字段**读出去**传人）、`HttpClient:775` 取个时间戳
      就要重查、`Decl.z42:455` `while` 条件合法而体内报错（**同一条语句内**）
- [x] 4.4 结论写回 proposal Q1 + design §D4；**取 (a)**，(b) 的实现随探针一并删除
  - ⚠️ 实测方法：探针把「本方法里被拿去和 `null` 比过的字段」视同标 `?`（全仓现有 `?` 字段 = 0，
    不造标记就是两边都零命中、什么也分辨不出），诊断降级 warning 以跑完全仓，两次均全量重编
  - ⚠️ **中途被缓存骗过一次**：`xtask clean` 不清 `artifacts/build/compiler/*/release/cache`，
    第一轮 (b) 拿到的是上一轮的 driver、量出「0 命中」。定向小用例才分辨出来 ——
    「全量零命中」永远要先怀疑是缓存或没接通
  - 顺带量到 **16 处 E0479 全是记忆化惰性初始化**（`if (cache != null) return cache;`），
    见 proposal Q1 末段

## 阶段 5: D —— 反向推导 + override
- [ ] 5.1 函数体 `return null;` 字面量但返回类型未标 → `UnmarkedNullReturn`
- [ ] 5.2 全仓跑，按提示补标；记录补了多少处（覆盖率指标）
- [ ] 5.3 override / 接口：返回可去 `?` 不可加；形参可加不可去 → `NullableOverrideMismatch`

## 阶段 6: E —— 砍 `?.` 与 `??`
> `?.` 与 `??` 都**不依赖** `simplify-ref-parameters`，也不依赖彼此。
> `?.` 成本≈0，可随时单独拆出；`??` 须在阶段 2（`Expect`）之后。
- [x] 6.1 `?.`：删脱糖 + AST 节点 + 绑定分派；删 `tests/control_flow/null_conditional.z42`
  （`??` 的覆盖在 `null_coalesce.z42` 里已有，不丢）
  - **保留 token、报 E0480 并按普通 `.` 继续解析**（同 E0471 对 `out`/`in` 的手法），
    比让 `?.` 碎成 `?` + `.` 的语法汤强得多
  - ⭐ 顺带修掉一个真 bug：旧脱糖把接收者**绑定两次** ⇒ `F()?.X` **调用 `F` 两次**。
    原注释自述「简单接收者双求值无副作用；goldens 均为局部接收者」——**已知而未修**
  - ⚠️ `delegates-events.md` 原本把 `?.Invoke()` 当作防空委托的**推荐写法**，已改为显式查空。
    生产代码零依赖（实测 `?.` 在 z42 源码里只有 3 处，全在删掉的那个用例里）
- [x] 6.2 `?.`：确认 android/ios appbuilder 里生成的 Kotlin/Swift 源串**不受影响**（那是字符串内容）
  - 实测：`?.` 的其余 14 处命中全是 JS / Swift / Kotlin（平台代码或生成的源码串），一处未动
- [x] 6.3 `??`：删 `OperatorEmitter._emitNullCoalesce` 发射 + `ExprTyper` 绑定分支；
  语法层保留 token、报 E0480、**完整吃掉右侧后只取左侧**（不这么做 `a ?? b ? c : d` 会脱轨）
- [x] 6.4 `??` 迁移 —— **实测形态与规划差得很远，结论更好**：
  - 全仓 100 处里 **70 处是同一个形态** `Environment.GetEnvironmentVariable("X") ?? ""`
    （生产 50 / 测试 20）
  - ⇒ **不机械改写成 70 个三行 if 块**（那会让代码变吵），而是加重载
    `GetEnvironmentVariable(name, fallback)`，调用点退回一行且**结果不带 `?`**
    ⇒ 没有空检查义务。这正是 proposal 里「『可能没有』的结果怎么表达」那条规约
  - 其余真实生产站点只有 **5 处**：`AppProperties.GetOrDefault` / `RuntimeConfig.GetOrDefault`
    （`return v ?? fallback` → 早返回守卫）、`DateTime.ParseIso8601`（拼消息）、
    `ArgParser`（`?? ""` 在 `if (envVal != null)` 里，**本来就是死代码**）、`launcher`（实参位，提局部）
  - ⚠️ design 写的「生产 69 处（compiler 47 / stdlib 22）」**与实测不符**——
    那个数把实现本身、注释、regex 串 `"colou??r"`、wasm appbuilder 里生成的 JS
    （`err.stack ?? err`）都算进去了。**后三类一处未动。**
- [x] 6.5 `??` 迁移 —— 测试：`tests/control_flow/null_coalesce.z42` 删除、
  `tests/operators/default_string.z42` 改显式写法、parser/typecheck/codegen 的行为断言改阴性断言
- [x] 6.6 `ObsoleteNullOperator` 诊断带迁移写法（含「换成收默认值的 API」这条建议）
- [x] 6.7 全仓 grep 清零（生产代码 `??` 仅剩实现自身与注释）

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
