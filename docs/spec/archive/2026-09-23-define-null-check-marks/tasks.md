# Tasks: 引用类型的空检查标记

> 状态：🟢 已完成 | 创建：2026-09-21 | 完成：2026-09-23
> 分支/worktree：`ref-null-model` @ `/Users/d.s.qiu/Documents/z42-lang/wt-refnull`（基于 origin/main 187200c61 #748）
> 类型：`lang` —— 完整流程（阶段 1–9）
> **依赖**：`enforce-value-type-non-null`（定下 `?` 只适用引用类型）；流分析设施（建议先 `add-definite-assignment`）
> **不依赖** `simplify-ref-parameters`

## 进度概览
- [x] 阶段 0: User 审批 + Q1 实测计划确认
- [x] 阶段 1: A —— 流分析引擎（**`add-definite-assignment` 已建好设施**，本变更只加第二套事实）
- [x] 阶段 2: B —— `Expect("理由")`（E0490）
- [x] 阶段 3: C —— 标记 → MaybeNull，义务点全开（形参 #752 / 返回值 #753 / 字段 #763 / **跨包 PR 9**）
- [x] 阶段 4: Q1 实测（字段窄化 (a) vs (b)）—— **取 (a)**
- [x] 阶段 5: D —— ~~反向推导（不做）~~ + override / 接口一致性（E0489）
- [x] 阶段 6: E —— 砍 `?.` 与 `??` + 全仓迁移（#754 / #755 / #756）
- [x] 阶段 7: 自举 + GREEN（每刀各自过 CI；末刀 PR 9 另见下）
- [x] 阶段 8: 文档同步 + 归档

### PR 切分（实施时定的）

原计划是「引擎 → `Expect` → 开闸」。实际上 `add-definite-assignment` 已经把引擎建好了
（结构化数据流 + 正常结束分析 + join 规则），所以改按**义务点的来源**切，一次开一个口子：

| PR | 范围 | 状态 |
|---|---|---|
| **1** | 标 `?` 的**形参** + 义务点 + 窄化（E0478） | ✅ #752 已合 |
| **2** | 标 `?` 的**返回值** + `PossibleNullReturn`（E0479） | ✅ #753 已合 |
| 2b | —— 合并进下方 PR 7 |
| **3** | 砍 `?.`（E0480） | ✅ #754 已合（先于字段那批——它只有 3 处、且带一个真 bug） |
| **5** | **字段 / 属性（E0484）+ Q1 实测定稿** | ✅ #763 已合 |
| **6** | ~~反向推导（实测后不做）~~ + override / 接口一致性（E0489） | 本 PR |
| **7** | `Expect("理由")`（E0490） | 本 PR —— ⚠️ 「emitter 合成 `new` + `throw` 无先例」**实测确为误判**：照抄 #760 的 `ThrowTerm` 手法，一个新 opcode 都没加 |
| **4** | 砍 `??`（100 处） | ✅ #755 + #756 已合 —— 与 `?.` 同码 E0480 |
| **8** | override / 接口一致性（E0489） | ✅ #782 已合 |
| **9** | **跨包携带标记**（`$Nullable` / `$RetNullable` 旁路通道）+ 3 条跨包用例 + 文档 + 归档 | 本 PR |

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

## 阶段 2: B —— `Expect("理由")`✅
> 原写「必须在阶段 3 开闸之前落地」——实际排到了最后，理由见上方 PR 切分表。
- [x] 2.1 intrinsic 绑定 —— 挂在 `MemberResolver._bindInstanceMemberCall` **顶部**
  （所有收者种类——类 / 接口 / 基元 / 懒加载 stub / Unknown——都汇聚到这一个漏斗）
  - ⚠️ **规范没预见的撞名**：`Expect` 已经是真实成员 —— `TomlParser.Expect(char, string)`，
    12 个调用点。定下**真成员优先、intrinsic 兵底**：intrinsic 抢赢会让用户已有的
    `Expect` **静默改变行为**（最坏的一类回归）；真成员赢最多是「这个类上用不了逃生口」，
    用户看得见、改得动。实测：353 个 golden **一个没动**
  - 值类型收者也不介入（复用 `TypeChecker._isNonNullableValueType`，**不另写一份判据**）
- [x] 2.2 参数校验 → **E0490**：必须给 / 必须是字符串字面量 / **且非空串**
  （`Expect("")` 等于把它还原成 C# 的 `!`）
- [x] 2.3 发射 —— `BrCondTerm` + `ThrowTerm`，**照抄 #760 的手法**，全用现有指令、
  不新增 opcode、不动 wire 格式、VM 一行不改
  - ⚠️ 消息**只放理由、不嵌源位置**：`Span.File` 是构建机路径，嵌进去会烤进 zbc、
    破坏字节不动点（#760 踩过）。位置不会丢 —— `Throw` 运行期已做 resolve_line +
    populate_stack_trace，抛出点自动在栈回溯里
  - ctor 键**从符号表现查**（primary ctor 是裸名、非-primary 才是全签名 mangle），同 #760
- [x] 2.4 结果事实置 NotNull；**不下钻 `Target`**（那正是要放行的那个值）
  - ⚠️ 确定赋值（E0407）**不受影响**：`_reads` 照样递归 `Target`。两件事不同源
- [x] 2.5 e2e 运行期用例（`src/tests/null_checks/expect_escape_hatch.z42`）
  - ⚠️ **两条路径的结果都必须被使用**，否则 DCE 消掉整条指令、用例成摆设
    （`make-hard-cast-fail-properly` 实测踩过）
  - ⚠️ 做了**阴性对照**（改错期望消息 → 确认变红），否则「1 passed」证明不了它走到了那条路

## 阶段 3: C —— 标记 → MaybeNull
- [x] 3.1 `SymbolTable.z42:590` 引用类型分支：仍返回 inner 类型（类型身份不变），沿途记标记位
  - 实施时改成在 `SymbolCollector._methodSymbol` 直接读 AST（`md.Params[i].Type is NullableType`），
    比在擦除点串标记位简单，且**不动类型身份**——`ParamIsNullable` / `RetIsNullable` 与
    `ParamIsRef` 一样是 `Z42FuncType` 的旁路数组，不进 `Name()` / `Dump()`。
- [x] 3.2 `MethodSymbol` 每形参一位 + 返回值一位；`FieldSymbol` 一位
  - 形参位 + 返回值位已加（`Z42FuncType.ParamIsNullable` / `RetIsNullable` / `IsNullableParam(i)`），
    **三者均已接通**；字段位 = `FieldSymbol.IsNullable`（本 PR）。
  - 返回值位到调用点靠 `BoundCall.RetIsNullable`（post-construction，经 `BoundCall.Marked`
    在 **20 个手里有已解析签名的构造点**写入，其余构造点不动）。
    ⚠️ 考虑过让 FlowAnalyzer 自己按 `(OwnerClass, RegKey)` 反查符号表（只改一处），
    **否决**：那等于再实现一遍重载解析，查错就是**误报**（D3 零容忍）；
    显式接线漏点只会漏报（D4 允许），方向安全。
- [x] 3.3 跨包标记（**PR 9**）—— 做法与规范写的**完全不同**，前提是假的
  - 🔴 **规范的前提「`TsigTypeName` / `StubEmitter` 已双向拼写」是错的**：实测 zpkg 里
    **一个 `?` 都没有**。真因是 `FunctionEmitter._sigTypeName:229` 写 SIGS 时**显式剥 `?`**，
    而那不是疏忽 —— **SIGS 的类型拼写同时是派发键**（`Find$1$string`），把 `?` 拼进去
    会改键、打烂派发与全部 golden。⇒ **拼写这条路根本走不通，只能走旁路。**
  - 落地 = 照 `record-ref-in-signature` 的 `$ByRef`/`$RefSig` 手法开旁路 attr-ref 通道：
    形参挂 **`$Nullable`**（逐形参）、返回值挂**方法级** **`$RetNullable`**（返回值没有形参槽）。
    骑既有通道 ⇒ **无 zbc/zpkg 格式 bump**。`CompilerFingerprint` 11 → 12。
  - ⭐ **不配完备性标记**（与 `$ByRef` 的关键差别）：`?` 的语义是「请编译器强制检查」，
    **缺席 = 不强制**（不是「保证非空」）⇒ 旧包读不出哨兵时落成 false **恰好正确**，
    不存在 `ref` 那种「不知道」与「全都不是」混同会误报的问题。
  - **覆盖面**：类方法 + 自由函数的形参与返回值。实测全仓恰好 **3 个**真标记跨过边界
    （`Process.Which` / `ProcessHandle.TryWait` / `IPAddress.TryParse`），**零误报**。
  - 🔴 **仍不携带的三类（刻意，漏报方向）**：
    ① **接口成员** —— 在 zbc 里走 **TYPE 方法块**（只有名/返回/形参类型/static），
       **根本没有 attr-ref 通道**，要补得扩格式（minor bump）⇒ 独立 change；
    ② **字段** —— 拼写走 `SurfaceTypeName(ft)`（`?` 已擦除），改它会动 TSIG 文本、
       牵扯继承字段展开与 struct 布局判别；
    ③ **extern 桩** —— `_emitNativeStub` 不写 `Attrs`/`ParamAttrs`。
    > 🔄 **2026-09-25 已关闭**：见 `docs/spec/changes/carry-null-marks-through-native-stubs/`。
    > 重新摸底发现这条不是「漏报一种形态」而是「**作者已经 opt in、机制假装没看见**」：
    > stdlib 里已有 **9 处 extern 标了 `?`**（`Environment.GetEnvironmentVariable` 等）。
    > 且它是下文「人工标注 stdlib」的**前置**—— 最佳候选（`Type.GetElementType` /
    > `GetGenericArguments` / `WeakHandle.Upgrade` / `PropertyInfo.GetValue`）大多是 extern。
  - 顺带修掉一个**用户可见的毛病**：诊断里印的是 `BoundCall.MethodName` = **注册键**，
    跨包导入符号带 `$arity$types` 后缀 ⇒ 用户看到 `` `Find$1$string` ``（源码里根本没写过）。
    加 `FlowAnalyzer._displayName` 剥后缀。**这条在跨包打通前照不出来**：本包非重载方法的
    键恰好就是裸名，一直蒙对
- [x] 3.4 义务点全开（解引用七类 + `return` 传播）→ **E0478** / **E0479**
  - 解引用：成员访问 / 下标 / 方法调用接收者 / `foreach` 集合 / `throw` 操作数
    （限**裸名**与**调用结果**）
  - `return` 传播：**E0479**，可空值不得从未标 `?` 的返回类型漏出去（**字段也算来源**，本 PR 接通）
  - 未覆盖：跨包的**字段 / 接口成员 / extern 桩**（见 3.3 的三类缺口）
- [x] 3.5 标 `?` 的字段直接解引用 → **E0484** `NullableFieldNotSnapshotted`（消息给快照写法）
  - `FieldSymbol.IsNullable` 旁路位（与 `Z42FuncType.ParamIsNullable` 同构，不进类型身份），
    `MemberCollector` 直接读 AST（`fd.Type is NullableType`）；**属性同样置位**——D4 的第一条
    理由（「会被读两次」）说的正是属性
  - 标记**从接收者的类型上查**（`m.Target.Type()` → `Z42ClassType.Fields`），于是 `this.F` 与
    `other.F` 走同一条路，不必把 enclosing 类当参数串下来
  - **快照**（`var v = this.F;`）把义务转移到局部 → 复用形参那半的窄化机器；这条不是优化，
    是字段标记的**唯一逃生口**
  - `return this.F;` 漏给未标 `?` 的返回类型仍由 E0479 接住
- [x] 3.6 跨包用例（`src/tests/cross-zpkg/`，3 条，全绿）
  - `nullable_marks_cross_pkg` —— 正例：标了 `?` 的跨包返回值按规矩窄化 ⇒ 编过跑通；
    **同包对照**未标 `?` 的 API 直接解引用**也**编过（钉「缺席 = 不强制」这个支点）
  - `nullable_marks_cross_pkg_unchecked` —— 阴性：跨包 `?` 返回值直接解引用 ⇒ **E0478**
  - `nullable_marks_cross_pkg_override` —— 阴性：override 跨包基类方法时**去掉**形参的 `?`
    ⇒ **E0489**（走的是另一个消费端：继承一致性，不是调用点）
  - ⭐ **方向是挑过的**：一开始写的是接口 + 「契约没标、实现加 `?`」那一侧，**修前就能过** ——
    导入侧标记位缺省 false，而「契约方没标」恰好也是 false，**蒙对了，一条也分辨不出**。
    真正有判别力的只有「**契约方标了、实现方去掉**」。（后来又因接口无 attr 通道改走基类。）

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

## 阶段 5: D —— ~~反向推导~~ + override
- [x] 5.1 ~~函数体 `return null;` 字面量但返回类型未标 → `UnmarkedNullReturn`~~
  —— **❌ 不做**（User 裁决 2026-09-23）。摸底：探针跑全仓 **464 处命中 / 174 个方法**，
  形态全部正当（守卫 252 / 裸 `return null;` 127 / `ReadAt` 哨兵 42 / 虚方法基类桩 37），
  **无一处是错的**。且它与「缺席 = 不强制」的支点冲突，并使「去 `?` 永远安全」失效。
  理由写回 design §D8 + proposal + `docs/reference/.../types.md`
  - ⚠️ 探针第一轮拿到「零命中」是**缓存骗的**（`cached: 115/115`）——与阶段 4 同一个坑，
    `xtask clean` 不清 `artifacts/build/*/*/release/cache`。判据是日志里必须全是 `cached: 0/N`
- [x] 5.2 ~~全仓跑，按提示补标~~ —— 随 5.1 取消
- [x] 5.3 override / 接口：返回可去 `?` 不可加；形参可加不可去 → **E0489**
  - 两条路各挂现成钩子：override 半在 `_passSealedEnforce`（已备好 `localSym` + `near`）、
    接口半在 `_checkOneIfaceMethod`（已备好 `cm` + `ims`），共用 `_checkNullableDirection`
  - **全仓真实命中 0**（现存 `?` 标记仅 43 处）⇒ 零迁移。但**「零命中」不能证明接通**——
    接通性由 8 条定向用例担保，其中 **4 条阴性**（该报必报）

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
- [x] 7.1 按 `bootstrap-seed.md` 走冷种子（编译器自身改了 47 处 `??`）
- [x] 7.2 `xtask build` + `xtask test all` 全绿；`cargo test -p z42 --lib`（**debug，不能 `--release`**）
  - 🔴🔴 **PR 9 在这一步栽了很久，教训值钱**：改动使**两代**才生效（第一代产的包才带哨兵，
    第二代才读得到），于是我拿**混代的 `artifacts/build/`** 反复量，得出过两个**完全错误**的结论
    ——先判「跨包标记没接通」，又判「main 从冷缓存编不过、8 条既有误报」。
    两次都是**对照组本身被污染**：树是 `origin/main` 没错，但 `artifacts/build/` 里的库与
    **暂存编译器**还是我自己上一轮 3.3 的产物。
  - ⭐ **判据（记死）**：验 pristine **必须 `rm -rf artifacts/build`**，只核 `artifacts/.z42`
    （SDK 种子）**不够** —— 真正在编译的是 `artifacts/build/compiler/` 那份。
    真做干净之后：纯净 main 冷构建**全绿**，我的改动连跑**两代**也**全绿**。
- [x] 7.3 golden 核对（`xtask test all` 覆盖）
- [x] 7.4 确认无 zbc/zpkg 格式 bump —— **理由与规划时写的不一样**：`?` **根本不在**签名文本里
  （`_sigTypeName` 剥掉了，因为那串是派发键）。真正的承载是**既有 attr-ref 通道**上的
  `$Nullable` / `$RetNullable` 两个哨兵 ⇒ 无格式 bump，但 `CompilerFingerprint` 11 → 12
  （标了 `?` 的方法 attr 块多了哨兵 ⇒ zpkg 字节变；且**跨包调用点源文件哈希不变而诊断变**，
  不 bump 会命中旧条目、把新诊断整个吞掉）
- [x] 7.5 推 PR 过 CI

## 阶段 8: 文档 + 归档
- [x] 8.1 `docs/reference/src/language/types.md` —— 空检查规则 / 义务点清单 / 窄化清单 / 字段快照 / `Expect` / 跨包边界
- [x] 8.2 **命名把关**：全文用「空检查」，**不得出现「空安全」**；显式写明不健全与会漏的形态
- [x] 8.3 版本语义表（返回加 `?` 破坏调用方 / 形参加 `?` 破坏实现方 / 去 `?` 永远安全）—— types.md §「加 `?` 和去 `?` 各会破坏谁」，并写明它与「D8 反向推导」不能同时为真
- [x] 8.4 `docs/roadmap.md` —— 可空线的真实进度（🔴 未排期 → ✅ 0.6.x，并挂出三类跨包缺口）
- [x] 8.5 归档

---

## 明确不做
- **悲观档**（所有引用类型强制检查，不看标记）—— 实测全仓 49275 个解引用点 / 974 处现存检查，
  悲观档需新增 8000~19000 处守卫（代码量 +8%~19%），而历史证据不支持（18 个 null 提交里绝大多数是 GC/运行期的）
- 运行期 `NullReferenceException` 位置信息改善 —— 正交且更便宜，独立 change
