# Tasks: `methodof` 方法引用表达式（add-method-reference）

> 状态：🟡 **DRAFT —— 待 User 确认后才进 IMPL** | 创建：2026-09-06 | 见 [proposal.md](proposal.md)
>
> 本变更属 **lang 类**，按 [CLAUDE.md](../../../.claude/CLAUDE.md) 走「DRAFT → User 确认 → IMPL →
> GREEN → COMMIT」。① 组是**阻塞项**：其中任一条证伪，设计需返工，不得直接进 ② 组。

## ① 前置验证（阻塞设计成立性，写实现代码前必须做完）

- [x] 1.1 **【最高优先】`[Foo(typeof(Bar))]` 端到端是否工作** —— ✅ **2026-09-06 验通过**：
      attribute 实参位置与普通代码的 `typeof` **逐条行为一致**（本地类 / 自引用 / 数组 /
      构造泛型 / 字段·方法上的 attribute 全部一致），合成工厂路径**零偏差**，
      担心的作用域问题**没有发生**。「零元数据改动」支点成立，设计不返工。
      附带发现一个 pre-existing 的跨 zpkg 空 `Type` 不对称（详见 proposal「验证项 0 的实测结果」），
      不在本提案 Scope，但 6.4 的跨包用例须显式覆盖两种依赖声明情形。
      原始担忧记录如下：
      —— 这是整个「零元数据改动」的支点。已确认的事实：`AttributeSynth._synthFactory`
      (`AttributeSynth.z42:129-142`) 把 `at.Args` 原始 `Expr[]` 原样塞进 `ObjNewExpr`，
      全仓库**无任何 attribute 实参常量性校验**。但现有 attribute 实参**全是字面量**
      （字符串/数字/bool + 命名实参），330 个 `typeof` 用例**没有一个在 attribute 实参位置**。
      → `methodof` 会是第一个在该位置放非平凡表达式的特性。
      **最可能的坑 = 作用域**：工厂函数被合成为**顶层 static 自由函数**
      （`_synthFactory` 传 `new Param[0], 0, true, body`），而 attribute 可能写在类内部并引用
      该类可见的类型（嵌套类型 / private 类型 / 该 CU 的 usings）。
      **typeof 不通 ⇒ methodof 更不通，须先修工厂路径，本提案范围随之扩大。**
- [ ] 1.2 `typeof` 的 emit 具体走哪条 runtime 路径（`TypeOpEmitter.z42:61-64` 往下）
      —— `methodof` 要照抄；决定新 builtin 的形状与注册位置。
- [ ] 1.3 `TypeNameResolver.SurfaceTypeName` 对**泛型参数 `T`** 能否正确拼回 `T`
      —— 参数类型匹配依赖它。已知它输出 TSIG 规范名（`byte[]` → `u8[]`），
      泛型参数走 `t.Name()` 兜底路径（`TypeNameResolver.z42:86-89`），未验。
      同时确认用户输入侧的 alias 归一（用户写 `byte[]`，渲染出 `u8[]`）能否复用
      `PrimModel.SurfaceName` 的映射——**不做归一会出现「明明写对了却报不存在」**。
- [ ] 1.4 模块级驻留缓存放在哪一层
      —— 反射侧现在**零缓存**：`invoke.rs:169-200` 每次 `module.func_index.get(qualified)`
      查 HashMap，`FieldIC`/`VCallIC` 在 `corelib/` 下零命中，无现成 IC 可复用。
      参照 `bytecode.rs:530 resolved: OnceLock<ResolvedTokens>` + `tokens.rs:26 UNRESOLVED` 哨兵。

## ② 语法层

- [ ] 2.1 `Lexer.z42` 注册 `methodof`（照 `typeof` 同款 `_kw`）
- [ ] 2.2 `Ast.z42` 新增 `MethodOfExpr { OwnerType: TypeExpr, Member: string, ParamTypes: TypeExpr[], ParamCount: int, HasParamList: bool }`
- [ ] 2.3 `ExprParser.z42` 的 `methodof(` 分支：**括号内按签名语法解析**，不走表达式解析
      —— 这是本设计的技术关键：表达式文法下 `Logger.Log(List<int>)` 里的 `List<int>` 与 `<`
      比较运算真歧义；封闭括号内 `<` 恒为类型实参，歧义消失。
- [ ] 2.4 `HasParamList` 与「空参数列表」要能区分：`methodof(X.M)`（不给列表，仅当无重载时合法）
      vs `methodof(X.M())`（显式零参重载）——两者语义不同，AST 必须分得开。
      省略规则的完整语义（候选集含继承链+跨包、≥2 必报错、新增重载会让已有省略写法变红）
      见 proposal「参数签名可省略（无重载时）」节

## ③ 语义层：解析与重载匹配

- [ ] 3.1 `BoundExprOp.z42` 新增 `BoundMethodOf`，带 `TargetName`
      —— 照抄 `BoundTypeof.TargetName`(`BoundExprOp.z42:184-188`) 保 FQN 的手法。
      **不是新机制**：`typeof` 已经为「结构化后 FQN 蒸发」打过同一个补丁。
- [ ] 3.2 `TypeOpTyper.z42`：解析 owner 类型 → 按 `Member` 收候选（含继承链、imported）
- [ ] 3.3 重载匹配：`HasParamList` 时用 `SurfaceTypeName` 规范化后逐位比对；
      **按名字全收 ≠ 本特性语义**——零匹配与多匹配都必须报错，不得静默选一个
- [ ] 3.4 属性访问器：`methodof(Logger.get_Level)` / `set_Level` 直接按方法名走（v1 不引入
      `getter:` 标签形式）
- [ ] 3.5 **明确拒绝**四类指不了的方法（见 proposal「指不了的方法」节）：泛型基类替换后同签名 /
      运算符与转换（无名字）/ 显式接口实现 / 私有成员。诊断必须说「无法指代」而非含糊报找不到。

## ④ 发射 + runtime

- [ ] 4.1 `TypeOpEmitter.z42`：`BoundMethodOf` → 产出 qualified 名 → 调新 builtin
      —— **重载解析全部在编译期完成**，IR 里只剩一个 qualified 名 → 一个 STRS 池索引，
      运行期零签名匹配痕迹。体积与一次普通 `Call` 等同。
- [ ] 4.2 runtime 新 builtin `__methodof(qualified)` → `MethodInfo`
      —— 复用 `methods.rs:206 build_method_info`，**不新造类型**
- [ ] 4.3 驻留缓存（按 1.4 的结论落地）；顺带给反射 `Invoke` 一条句柄化快路径

## ⑤ 诊断（码待分配，E04xx 段）

- [ ] 5.1 目标方法不存在 → Error，**必须列出该名字下全部可用重载**
- [ ] 5.2 参数类型列表匹配到多个 → Error，列出全部候选 + 说明为何分不开
- [ ] 5.3 未给参数列表但存在重载 → Error，提示补参数类型列表
- [ ] 5.4 目标是运算符/转换/显式接口实现 → Error，明确「无法指代」
- [ ] 5.5 **逃生口检查**：确认上述诊断都给出了可操作的下一步。
      Swift 的 `@derivative(of:)` 没留逃生口，用户撞上只能读到 ambiguous、无解
      （其编译器测试 `derivative_attr_type_checking.swift` 里没有任何类型标注用法）。

## ⑥ 测试

- [ ] 6.1 `src/tests/reflection/methodof_basic.z42`：无重载 / 参数列表消歧 / 属性访问器 /
      静态方法 / `Invoke` 往返
- [ ] 6.2 `src/tests/attributes/methodof_in_attribute.z42`：attribute 实参放 `methodof(...)`，
      `GetCustomAttributes` 读回并 `Invoke`（**这是本特性的主用例**）
- [ ] 6.3 负例：不存在的方法 / 歧义重载 / 运算符 → 断言诊断码与「列出候选」的文案
- [ ] 6.4 跨 zpkg：在 A 包 `methodof` 一个 B 包的方法，确认 FQN 保全
- [ ] 6.5 **jit 双验**：`xtask test stdlib --mode jit`
      —— 本地 `xtask test` 只跑 interp，新增反射路径必须补跑

## ⑦ 文档

- [ ] 7.1 `docs/book/src/language/methodof.md`：语法 / 与 `typeof` 对称 / 重载消歧 /
      指不了的方法 / **`&` 留给非托管函数指针的分工与理由**
- [ ] 7.2 `docs/book/src/SUMMARY.md` 挂目录
- [ ] 7.3 归档（`changes/` → `archive/`）**随本 PR 一起提交**，不得合并后单独直推 main

## ⑧ 自举纪律（必须遵守）

- [ ] 8.1 本变更只落「z42c **支持** `methodof`」；z42c 自身源码 / stdlib / xtask
      **一律不得使用** `methodof`——按 [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)
      的 support 先行、use 晚一个 nightly 纪律。
- [ ] 8.2 落地后跑 `xtask test bootstrap` 确认无语法越界（上一版 nightly 的 z42c 仍能编当前源）

## GREEN 门

- [ ] G1 `xtask test` 全绿（interp）
- [ ] G2 `xtask test stdlib --mode jit` 全绿
- [ ] G3 自举字节不动点 gen1 == gen2
- [ ] G4 `xtask test lines` 全绿（超限文件只能缩不能涨）
- [ ] G5 `xtask test bootstrap` 无越界
