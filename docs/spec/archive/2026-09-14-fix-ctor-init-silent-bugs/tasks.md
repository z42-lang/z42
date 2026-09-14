# Tasks: fix-ctor-init-silent-bugs
**变更说明：** 修两个「构造器不跑、字段停在默认值、零诊断」的编译器静默 bug：① 静态 ctor 混进实例 ctor 的重载选择；② 零实参的 `: this()` / `: base()` 整条初始化子句被丢。
**原因：** #643 写测试时撞出，当前 nightly（`6e2a1aa14`）复现；两者都不报错、结果悄悄错。
**文档影响：** `docs/book/src/language/`（构造器初始化子句 / 静态 ctor 不参与实例构造）；`docs/design/language/language-overview.md` §6 实现细节补一句；#643 测试里的绕行写法改回直接写法。

## 实测（修前，nightly z42c + z42vm，interp/jit 同）

| # | 源 | 期望 | 实际 |
|---|----|------|------|
| ① | `class C { C(){…} static C(){} }` `new C()` | 实例 ctor 执行 | 不执行（`obj_new C.C$0`，函数实名 `C.C`） |
| ①' | `class C { static C(){} C(int x){…} }` `new C()` | E0426 | 零诊断、零初始化 |
| ② | `O2(int x) : this() {…}` | 先跑 `O2()` | `O2()` 不跑（连带字段初始化器也不跑：委托 ctor 本就跳过注入） |
| ②' | `D0(int x) : base() {…}` | 跑 `B0()` | `B0()` 不跑 |

## 根因

- ①：静态 ctor 走「静态非虚方法恒全签名 mangle」注册成 `C$0`；`OverloadBinder._ctorKey(ct, 0)` 先查 `C$0` 命中它，
  且按 arity 回扫 `OverloadsOf` 时也不排除 static。`ConstructTyper` 的同类型重载决议候选与 E0426 arity 校验同样把静态 ctor 算进去（①'）。
- ②：`DeclBinder._bindMethodBody` 的门是 `md.BaseArgCount > 0` ——把「写了初始化子句」错当「实参数 > 0」。AST 没有「存在」位。

## 任务

- [x] 1.1 `Z42ClassType.InstanceCtors()` 单一入口（`OverloadsOf(Name())` 过滤 `IsStatic`）；`_ctorKey` 快路径与回扫、
      `ConstructTyper` 的类型决议候选与 E0426 校验、**`ConstraintChecker._hasNoArgCtor`（`where T : new()`，实施中发现的第四处）**统一走它
- [x] 1.2 `MethodDecl` 加 `HasCtorInit`（**不进构造函数签名**，构造后由 parser 置位——轴 ④ 种子 ABI 约束）；（`BenchmarkDesugar._demote` 只处理非 ctor 的 bench 方法，不需透传）
- [x] 1.3 `DeclBinder` 门改 `md.IsCtor && md.HasCtorInit`；目标 ctor 在目标类上**不存在**时不发调用
      （零实参 `: base()` 指向无显式 ctor 的基类 = 今天的行为，保持；否则会发出对不存在函数的调用）
- [x] 1.4 回归：`src/tests/classes/ctor_init_clauses.z42`（②②'，含字段初始化器 + 委托链顺序）、`static_ctor_with_instance_ctor.z42`（①）；
      typecheck 单测 ①'（E0426）+ `new()` 约束两条（静态 ctor 不计 / 仅静态 ctor 满足）；cross-zpkg `ctor_init_cross_pkg`（跨包 `: base()` + 跨包 `new`）；
      运行期用例在修前 nightly 上逐条确认红（含 1.5 改回直接写法的两个）
- [x] 1.5 #643 的绕行写法（`property_initializers.z42` 的 `: this(x, 0)` 等）改回直接写法
- [x] 1.6 文档同步（新 book 页 `language/constructors.md` + static-constructors 交叉引用 + language-overview §6.3 顺序订正 + cross-zpkg README）；两后端手验；全量 GREEN（main `06aa65701`，`GREEN_EXIT=0`）

## 不在本 change（登记）

- ③ 跨包静态方法调用不触发 cctor —— vm 类，另立 DRAFT。
- 实测新发现：基类**只有字段初始化器、无显式 ctor**，派生类写了显式 ctor ⇒ 基类字段初始化器永远不跑
  （`class B { int b = 7; } class D : B { D(int x){} }` ⇒ `new D(3).b == 0`），用户也无 `: base()` 可写来补救。
  与 language-overview.md §6「不自动调用 base ctor」的模型相关，属语义决策，需 User 裁决后另立。
