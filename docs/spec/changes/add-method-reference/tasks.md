# Tasks: `methodof` 方法引用表达式（add-method-reference）

> 状态：🔵 **IMPL —— User 已确认（2026-09-11）** | 创建：2026-09-06 | 见 [proposal.md](proposal.md)
>
> 本变更属 **lang 类**，按 [CLAUDE.md](../../../.claude/CLAUDE.md) 走「DRAFT → User 确认 → IMPL →
> GREEN → COMMIT」。① 组是**阻塞项**：其中任一条证伪，设计需返工，不得直接进 ② 组。

## ① 前置验证（阻塞设计成立性，写实现代码前必须做完）

- [x] 1.1 **【最高优先】`[Foo(typeof(Bar))]` 端到端是否工作** —— ✅ **2026-09-06 验通过**：
      attribute 实参位置与普通代码的 `typeof` **逐条行为一致**（本地类 / 自引用 / 数组 /
      构造泛型 / 字段·方法上的 attribute 全部一致），合成工厂路径**零偏差**，
      担心的作用域问题**没有发生**。「零元数据改动」支点成立，设计不返工。
      附带挖出一个 pre-existing 缺陷（`--emit-zbc` 吞诊断 + `<unknown>` 哨兵泄漏进 IR），
      已拆**独立前置变更** `fix-emit-zbc-swallows-diagnostics`，本提案 rebase 到其上。
      注意：**不涉及依赖声明**——标准库无需声明依赖是既定设计且工作正常，该怀疑已排除。
      详见 proposal「验证项 0 的实测结果」。原始担忧记录如下：
      —— 这是整个「零元数据改动」的支点。已确认的事实：`AttributeSynth._synthFactory`
      (`AttributeSynth.z42:129-142`) 把 `at.Args` 原始 `Expr[]` 原样塞进 `ObjNewExpr`，
      全仓库**无任何 attribute 实参常量性校验**。但现有 attribute 实参**全是字面量**
      （字符串/数字/bool + 命名实参），330 个 `typeof` 用例**没有一个在 attribute 实参位置**。
      → `methodof` 会是第一个在该位置放非平凡表达式的特性。
      **最可能的坑 = 作用域**：工厂函数被合成为**顶层 static 自由函数**
      （`_synthFactory` 传 `new Param[0], 0, true, body`），而 attribute 可能写在类内部并引用
      该类可见的类型（嵌套类型 / private 类型 / 该 CU 的 usings）。
      **typeof 不通 ⇒ methodof 更不通，须先修工厂路径，本提案范围随之扩大。**
- [x] 1.2 **`typeof` 的 emit runtime 路径** —— ✅ **2026-09-11 查清，结论比预想省事**：
      发射侧不需要新 IR 指令。`TypeOpEmitter._emitBox`（同文件 :100-112）已证明范式——
      `BuiltinInstr(dst, "__box_struct", args, n)` **复用既有 Builtin opcode，零新 opcode、零格式 bump**。
      ⇒ `methodof` 走 `ConstStr(qualified)` + `BuiltinInstr(dst, "__methodof", [strReg], 1)`。
      builtin 注册在 `src/runtime/src/corelib/builtin_table_ext.rs` 的 `PART2` **表尾追加**
      （BuiltinId = 表下标、会烤进 zbc，**只可表尾追加**，见 builtin_table.rs:1-8）。
      runtime 侧 `build_method_info(ctx, simple, qualified, is_virtual)`
      （`reflection/methods.rs:206`）直接吃 qualified 名，**不新造类型**。
      **⭐ 最大发现：qualified 名不用自己拼签名编码。** `MethodSymbol.RegKey`
      （`Symbol.z42:17`）本就是「注册键 = Methods 映射键 = IR 名 = BoundCall 目标名」，
      形态 `Name` / `Name$arity` / `Name$arity$typesig`（方案A 全签名 mangle，
      stabilize-instance-dispatch-keys）——**重载身份是既有机制**。发射名 =
      `QualifyClass(_classShortName(ct)) + "." + ms.RegKey`，与 `EmitContext.ResolveSealedTarget`
      （:280）和 `TestIndexBuilder`（:61-63）**逐字节同款**。实测佐证：`Demo.Main.Main$0`
      是本轮实跑通的 VM 入口名。
- [x] 1.2b **🔴 新发现的实现约束（proposal 未记）：imported 类的继承方法必须查 `Deps` 校验。**
      TSIG 把**继承**方法展平进每个派生类的 `Methods`（`ImportedSymbolLoader._fillClass:266-293`）
      ⇒ `ct.Methods` 命中**不等于**本类声明 ⇒ `QualifyClass(派生类)+"."+RegKey` 可能指向
      **一个从未发射的函数** → 运行期 `undefined function`。`ResolveSealedTarget` 已用
      `Deps.Statics.ContainsKey(fq)`（`_depHasFunction`，EmitContext.z42:264）解决同一问题并
      沿基链上溯找真正声明类。**`methodof` 必须照做**，否则 `methodof(Derived.继承来的方法)`
      会静默发出坏名字——正是本特性要根治的那类静默失效。
- [x] 1.3 **`SurfaceTypeName` 对泛型参数 + 用户输入侧 alias 归一** —— ✅ **2026-09-11 查清，
      两侧都已现成，`TypeNameResolver` 无需改动**（可从 Scope 表移除 MODIFY 标记）：
      · **泛型参数 `T`**：`_resolvedTypeName` 末尾落到 `PrimModel.SurfaceName(n)`，而
        `SurfaceName`（`PrimModel.z42:141-145`）对非内建名 `Code(...) < 0` → **原样返回**
        ⇒ `T` 拼回 `T`，幂等。
      · **用户输入侧归一**：`TypeNameResolver.TsigTypeName(TypeExpr)`（:16-33）**已经**做了
        `_canonName` 别名归一（`byte→u8` / `sbyte→i8` / `short→i16` / `ushort→u16` /
        `uint→u32` / `ulong→u64`），且数组/泛型实参递归同款。
      ⇒ 匹配算法两侧落在**同一套字母表**：用户 `TypeExpr` 走 `TsigTypeName`，
        候选签名 `Z42Type` 走 `SurfaceTypeName`，逐位字符串比对即可。
        「明明写对却报不存在」的坑**不会发生**。
- [x] 1.4 **驻留缓存放哪层** —— ✅ **2026-09-11 查清，结论是 v1 不做，且这是「对称」而非「偷懒」**：
      实测 **`typeof` 今天自己就零缓存**——`exec_instr.rs:234-241` 每次执行都
      `make_constructed_type` **重新分配一个新 `Std.Type`**。本轮实跑确认
      `typeof(Box) == typeof(Box)` → **`false`**。
      ⇒ 给 `methodof` 单独加驻留缓存会造成两个后果：① `methodof(X.M) == methodof(X.M)`
      变 `true` 而 `typeof(T) == typeof(T)` 仍 `false`，**两个号称对称的特性行为不对称**；
      ② 悄悄给 `MethodInfo` 引入了对象身份语义，而这该是一次**显式的语义决策**。
      **裁决：v1 与 `typeof` 严格对称、不缓存。** 反射对象驻留是独立优化项，
      要做就 `typeof` / `methodof` 一起做、并同时定清对象身份语义。
      （原 tasks 4.3「驻留缓存」随之删除；反射侧零缓存的事实记录保留：
      `invoke.rs:169-200` 每次查 HashMap、`FieldIC`/`VCallIC` 在 `corelib/` 下零命中。）

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
- [x] 4.3 ~~驻留缓存~~ —— **按 1.4 结论取消**：与 `typeof` 对称即不缓存。反射对象驻留
      （含 `Invoke` 句柄化快路径）留作独立优化项，需与对象身份语义一并裁决。

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

## ⑦ span 地基（User 2026-09-06 裁决；2026-09-11 补记进 tasks）

> **为什么在本 PR 内**：proposal「IDE 支持的诚实边界」节记录了 User 的裁决——本提案顺带铺
> 「AST 名字级 span」与「符号声明位置」两块共用地基，**且必须在本 PR 内接一个可断言的消费方**，
> 否则就是「铺了没人走的路、无法验证铺对没有」。这条裁决此前**只写进了 proposal、没进 tasks**
> （2026-09-11 开工前核对时发现的提案内部不一致，已补齐）。
>
> 消费方选定为 **`methodof` 自己的诊断下划线**：⑤ 组要求「列出全部可用重载」，若下划线指在整个
> `methodof(...)` 表达式上而不是出问题的成员名上，诊断质量直接打折 —— 地基与特性是同一件事。

- [ ] 7.1 `MemberExpr` 记录**成员名自身的 span**（现状：`ExprParser.z42:23` 直接复用 target 的
      span，成员名的字节区间根本没被记录）。新增字段而非改写既有 `Span`，避免动既有诊断位置。
- [ ] 7.2 `MethodSymbol` / `FieldSymbol` 记录**声明位置 span**（现状：`Symbol.z42:9-56` 连声明
      位置都不存）。本地符号由 `SymbolCollector` 填；**imported 符号无 span**（跨包元数据不带
      位置信息）→ 显式留空并注释说明，不假装有。
- [ ] 7.3 **接上可断言的消费方**：把成员解析类诊断的下划线区间从「整个表达式」收窄到「成员名
      本身」，含 `methodof` 的 ⑤ 组诊断。
- [ ] 7.4 **门**：golden 诊断位置断言守住 7.3（位置回退即红）。**必须先跑退回对照**——
      把收窄改回去、确认 golden 真的变红，否则就是又一个「从不失败的门」
      （见 memory `audit-silent-gates-program`）。
- [ ] 7.5 **不做**：LSP 本体、引用索引、find-references / rename —— 依赖 roadmap 0.5.7 的
      `z42-lsp` 里程碑，独立立项。本组只铺地基 + 兑现一个消费方。

## ⑧ 文档

- [ ] 8.1 `docs/book/src/language/methodof.md`：语法 / 与 `typeof` 对称 / 重载消歧 /
      指不了的方法 / **`&` 留给非托管函数指针的分工与理由**
- [ ] 8.2 `docs/book/src/SUMMARY.md` 挂目录
- [ ] 8.3 归档（`changes/` → `archive/`）**随本 PR 一起提交**，不得合并后单独直推 main

## ⑨ 自举纪律（必须遵守）

- [ ] 9.1 本变更只落「z42c **支持** `methodof`」；z42c 自身源码 / stdlib / xtask
      **一律不得使用** `methodof`——按 [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)
      的 support 先行、use 晚一个 nightly 纪律。
- [ ] 9.2 落地后跑 `xtask test bootstrap` 确认无语法越界（上一版 nightly 的 z42c 仍能编当前源）

## GREEN 门

- [ ] G1 `xtask test` 全绿（interp）
- [ ] G2 `xtask test stdlib --mode jit` 全绿
- [ ] G3 自举字节不动点 gen1 == gen2
- [ ] G4 `xtask test lines` 全绿（超限文件只能缩不能涨）
- [ ] G5 `xtask test bootstrap` 无越界
- [ ] G6 ⑦ 组的 golden 诊断位置门**跑过退回对照**（改回旧位置必须变红）
