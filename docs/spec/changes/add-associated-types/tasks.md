# Tasks: 泛型表达力 —— 跨包约束持久化 + `Self` + 关联类型

> 状态：🟡 进行中 | 创建：2026-09-05 | 范围裁决：2026-09-06

## 进度概览

- [x] **PR-1** 跨包约束持久化 + flag 位 + 认知修正（零格式 bump）✅ 已合并 **#499**
- [x] **PR-2** `Self` 类型（仅接口）✅ 实现完成（GREEN 全绿 + JIT 双模式 + bootstrap 边界门），待开 PR
- [ ] **PR-3** 关联类型（含泛型接口实例化地基，双格式 bump）

> 三个 PR 顺序落地，每个独立 GREEN、独立合并。PR-3 依赖 PR-1 建好的跨包通道。

---

## PR-1：跨包约束持久化

### 1A 认知与门面修正（**独立 commit，必须先落**）

- [x] 1A.1 `ImportedSymbolLoader.z42:91-98` —— 保留「struct-ness 编码在 `HasBase`」的事实描述，删除「不许加 `ExportedClassZ` 字段 ← bootstrap 越界」的过时论证（依据：design D1 三条证据）
- [x] 1A.2 `docs/design/compiler/self-hosting.md:219` —— 删已失效的 warm-skip 描述（与同文件 :235-245 自相矛盾）
- [x] 1A.3 `.claude/rules/bootstrap-seed.md:151` —— 旧函数名 `_ensureBootstrapZ42Ir` → `_ensureBootstrapSelfDepLibs`；轴④判据补「**已有包的新 API** 由预建自动破环，无需等 nightly」
- [x] 1A.4 `scripts/build/xtask_compiler.z42` —— **无需改动**：`_ensureBootstrapSelfDepLibs` 的 `stdlibFlat` 参数本就同时充当 Z42_LIBS 与 `--output-dir`，已天然参数化
- [x] 1A.5 `scripts/build/xtask_bootstrap_check.z42` —— A 路径用隔离 runlibs 调同一预建函数；**注释写清改后仍守哪三轴、不再守哪一轴**（design D2 表）
- [x] 1A.6 跑 `./xtask test bootstrap` 确认改后仍绿（nightly + repo 两路径全 ✓，REAL_EXIT=0）。**真正的证明在 1D 之后复跑**——加完 `ExportedClassZ` 字段仍绿 = 假阳性确已消除

### 1B IR 与格式层（写端 → 读端）

- [x] 1B.1 `src/libraries/z42.ir/src/IrModule.z42` —— `IrConstraintDesc` 加 5 个承载位（`RequiresClass` / `RequiresStruct` / `BaseClass` / `RequiresCtor` / `RequiresEnum`）
- [x] 1B.2 `ClassDescBuilder` —— **改为复用 `ConstraintChecker` 已算好的 `ConstraintSet`**（`IrGen._symbols` 可达），而非从 AST 重推分类。比原计划更根治：writer 与 checker 共用同一判定，special 丢弃与 base/iface 混同两个缺陷一并消失
- [x] 1B.3 `ClassDescBuilder._interfaceDesc:341-344` —— 接口 bundle 不再恒空，填入接口自身的 where（依赖 1C.1）
- [x] 1B.4 `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42:280-298` —— 置全 bit0/1/2/4/5（今天只写 bit3）
- [x] 1B.5 `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42:459-476` —— bit2 base 由「读而不存」改为存入承载位（注释已留接口）
- [x] 1B.6 实测确认**零格式 bump**：regen fixture 后 `cargo test --test format_fixture_versions` 绿，zbc 仍 38 / zpkg 仍 43

### 1C 语义层（约束模型 + 键规则）

- [x] 1C.1 `ConstraintChecker.Resolve:38` —— 条件由 `class || struct` 扩到含 `interface`（design D8）
- [x] 1C.2 `SymbolTable.z42` —— 新增 `ConstraintKey(Z42ClassType)` 单一辅助（规则同 `Classes` 的条件 arity-mangle）
- [x] 1C.3 `ConstraintChecker.z42:40 / :122` —— 写入与查询改调 `ConstraintKey`，消掉同名多 arity 的 last-wins 串味
- [x] 1C.4 入口落在 `ImportedSymbolLoader._constraintSetOf`（离使用点最近），`GenericConstraint.z42` 无需改动

### 1D 跨包搬运

- [x] 1D.1 `src/libraries/z42.ir/src/ExportedTypes.z42` —— `ExportedClassZ` 加约束字段（**不进 ctor 签名、ctor 给默认值、构造后赋值**）
- [x] 1D.2 `src/libraries/z42.ir/src/TsigReconcile.z42:508-523` —— `_rebuildClass` 读 `cd.TypeParamConstraints` 搬进 `ExportedClassZ`（今天一次都没读过）
- [x] 1D.3 `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` —— seed 导入类型的约束进 `SymbolTable.ClassConstraints`（用 1C.2 的键）

### 1E 落地强度与 🔴 风险（design D4 / D7）

- [x] 1E.1 ~~先以 warning 落地~~ → **改为直接 error**：实测 driver `Main.z42:345` 的 `if (art.ErrorCount > 0)` 门控全部诊断呈现，warning 在 CLI 上根本不打印 ⇒ 探针是空操作。见 design D4
- [x] 1E.2 **阳性对照**：跨包违反 fixture 确实报出 `E0402 … does not satisfy constraint \`IShow\` on \`Box\``（Span 正确）⇒ 通道确已打通，「零违反」与「通道没通」可区分。error 模式下 `build compiler` + `build stdlib` 均 0 违反
- [x] 1E.3 **未触发**：error 模式下 `build compiler` + `build stdlib` + 完整 GREEN 均零违反，🔴「基元 wrapper 归一偏差」风险未兑现
- [x] 1E.4 直接以 error 落地（warning 探针在本项目不可见，见 design D4）
- [x] 1E.5 `struct P` / `struct Tagged` 补 `: IEquatable<>` + `Equals`/`GetHashCode`。⚠️ 注释里写明**覆盖点偏移**：原测「不实现 IEquatable 的 blob struct 靠装箱默认相等当键」，现走用户 Equals；P3a 的装箱/拆箱本意仍覆盖

### 1F 测试

- [x] 1F.1 `constraint_tests.z42` 27 → **33** 条：接口 where 三条声明期诊断 + 一条不误报 + 同名多 arity 双向。**已实证是真门**——临时把 `ConstraintKey` 退回恒裸名后，两条 arity 用例分别以「凭空误报 E0402」和「漏报」变红
- [x] 1F.2 `src/tests/cross-zpkg/generic_constraint_cross_pkg/` NEW —— target/ext/main 三包，六类约束各一条正例 + **三包链路**（约束在 A、类型在 B、实例化在 C）。负例不放这里（harness 比对 stdout，装不下编译期负例）
- [x] 1F.3 JIT 模式补跑：`test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit` 均 0

### 1G 文档与归档

- [x] 1G.1 `docs/book/src/language/generic-constraints.md` —— 已知限制 §1 由「不校验」改为「已校验」；补跨包链路机制（含 ASCII 链路图）
- [x] 1G.2 `docs/roadmap.md` —— 关掉 `where-constraint-future-crosspkg` + `-runtime-flags`（两条同一链路，一并兑现）；§编号顺移；新登记 4 条 Deferred（loader 接口启发式 / 跨包 `new T()` / 跨包 enum ToString / driver 隐藏 warning）
- [x] 1G.3 ~~归档随本 PR~~ → **本 change 拆三个 PR，归档只在最后一个（PR-3）里做**：`changes/` → `archive/` 是整个 change 完成时的动作，PR-1/PR-2 各自只带自己的文档同步。铁律「归档与代码同 PR」仍然满足——归档与 PR-3 同 PR

---

## PR-2：`Self` 类型（仅接口）

> **实施结论：半径比计划还小——12 项里 4 项被证明「无需改动」，真正的代码改动只有 2 处。**
> 关键简化：`ResolveTypeP` **本来就**有「名字命中 paramNames → `Z42GenericParamType`」分支
> （`SymbolTable.z42:214-218`），所以 `Self` 只要在解析接口成员时被**并进 paramNames** 即可，
> 不需要在 `ResolveTypeP` 里特判、不需要 `TypeEnv` 透传接口上下文、不需要给
> `Z42InterfaceType` 加槽。design D5「复用既有型参机制」原来可以走得更彻底。

- [x] 2.1 `Z42Type.z42` `Z42InterfaceType` 加型参名槽 —— **无需改动**：本包侧型参名直接来自
      `ClassDecl.TypeParams`（`_fillInterface` 手上就有），跨包侧 `ExportedInterfaceZ` 早已携
      `TypeParams`/`TpCount` 且 `TsigReconcile._rebuildInterface:382-383` 已从 `cd.TypeParams` 填。
      两条路都不经过 `Z42InterfaceType`，加槽是多余状态
- [x] 2.2 `SymbolTable.ResolveTypeP` 特判 `Self` —— **无需改动**（改走 2.4 的 paramNames 注入，见上）
- [x] 2.3 `TypeEnv.z42` 透传接口上下文 —— **无需改动**：接口无方法体（`IrGenAuxEmitter` 只发
      `_interfaceDesc` 描述符，不发函数），`Self` 只出现在**签名位置**，而签名解析全部发生在
      `_fillInterface` 内部，上下文天然在手
- [x] 2.4 `MemberCollector._fillInterface` —— **唯一本包改动点**：`tp = _withSelf(TypeParams.Names, Count)`、
      `tpc = Count + 1`。新增 `_withSelf` 辅助（拷贝新数组，**不就地追加**——原 `TypeParams.Names`
      被 ClassExtractor / IrGen 按 Count 复用，就地改会串味）
- [x] 2.5 `ClassExtractor._extractInterface` 导出 `Self` 为裸字符串 —— **无需改动**：
      `_hybridTypeName` → `_resolvedTypeName` 末尾 `PrimModel.SurfaceName(t.Name())` 对非基元名原样返回
      ⇒ `Z42GenericParamType("Self")` 自动导出成 `"Self"`，与型参 `T` 完全同款
- [x] 2.6 `ImportedSymbolLoader` 接口方法解析改四参版，型参集 = 接口 `TypeParams` ∪ `{"Self"}`
      （**顺带修既有缺口**：跨包接口方法里的 `T` 此前落 `Z42ClassType.Builtin("T")` 垃圾类型）
- [x] 2.7 类里写 `Self` 落 E0443、不新增错误码 —— 已实证（`Self x = null;` / `new Self()` 两个位置）
- [x] 2.8 `constraint_tests.z42` 33 → **38** 条：`Self` 5 条。⚠️ **首轮 7 条全是空测试，已推翻重写**——
      见下「实施期发现」①；真门是两条 `DumpBody` 用例（`:<unknown>` ↔ `:Self` 两态实测），
      作用域守卫合并成一条多断言用例（签名位 + 表达式位共 5 行）
- [x] 2.9 `src/tests/cross-zpkg/self_type_cross_pkg/` NEW —— target 声明带 `Self` 的接口（含泛型接口
      `IBox<T>` 同时守 `T`）、ext 经接口静态类型跨包调用、main 具体类调用。**已实证是真门**：
      把 2.6 退回单参版后该用例以 `FAIL self_type_cross_pkg (ext build)` 变红
- [x] 2.10 `docs/book/src/language/generic-constraints.md` —— 新增 `Self` 章节（语义 / 仅接口作用域 /
      实现模型与其三条边界）；已知限制 §1 补注「`Self` 是绕开不是消除」
- [x] 2.11 ~~关掉 `where-constraint-future-type-arg-matching`~~ → **校正为不关**：`Self` 让**新写法**
      不必写类型实参，但带实参的接口仍全部按裸名匹配，且 stdlib 现有声明这一轮**不改写**
      （bootstrap-seed 轴① ）⇒ 关掉它就是把「绕开」写成「已解决」，正是本条线在清的那种文档腐坏。
      改为：该条目补注说明 + 新登记两条 Deferred（`self-return-type-substitution` /
      `semanticdump-skips-collector-diags`）
- [x] 2.12 `./xtask test bootstrap` —— 绿（nightly z42c 编通当前源，无越界）。`Self` 无 lexer/parser
      改动，`TypeParser._parseType` 今天就把它解析成 `NamedType("Self")` ⇒ 结构上零越界风险，仍按规矩跑

### 实施期发现（PR-2）

1. 🔴 **`SemanticDump` 看不见 collector 相位诊断 ⇒ 一整类单测是空测试**。
   `SemanticDump.FirstErrorCode` 给 TypeChecker 另建 DiagnosticBag、从不合并 `SymbolCollector.Diags`
   （SemanticDump.z42:154-157），而**方法返回类型 / 形参类型 / 字段类型**位置的未定义类型 E0443
   恰恰由 collector 的 `_chkTypeRef` 发出 ⇒ 这些位置写任何未定义类型，FirstErrorCode 都返回 `""`。
   实测对照：`class C { Undef f(){…} }` / `void f(Undef x)` / `public Undef x;` 全 `""`，
   只有 `Undef x = null;` 与 `new Undef()` 报 E0443。
   **本 PR 首轮写的 7 条 `Self` 用例把支持改动整个退回后仍然 7/7 全绿** —— 全是空测试。
   改用 `DumpBody` 断言**解析结果**（无支持 `:<unknown>` / 有支持 `:Self`）才成真门。
   **→ 已修**，独立 change `fix-semanticdump-collector-diags`（PR-2 合并后紧接着落，
   User 追问「要不要先修」后先量了爆炸半径）：
   `SemanticDump._model` 合并 `sc.Diags`，6 个入口收敛到单一口径（原来 6 份复制粘贴的 preamble
   正是「其中一份忘了合并」能长期不被发现的土壤）。**实测爆炸半径 = 0**：584 条既有 compiler
   用例零失败 —— 与 `emit-zbc-no-error-gate` 不同，本洞只影响 `SemanticDump` 这一个测试 harness
   与人工 `--dump-bound`，**不经过 golden 语料**，故不必等那条线排队。
   新门 `typecheck_tests.z42::test_undefined_type_in_declaration_signature_positions_reported`
   已实证是真门（注掉 `MergeFrom` 即红）。roadmap 那条 Deferred 随之撤销登记。
2. **`Self` 返回位不做替换**：`IClone c; var x = c.Copy();` 里 `x` 的类型是型参 `Self` 本身，
   不替换成接收者静态类型。具体类上调用不受影响。已登记 Deferred `self-return-type-substitution`。

---

## PR-3：关联类型

### 3A 地基：泛型接口实例化

- [x] 3A.1 ~~`Z42InstantiatedType.Def` 提升为可承载 `Z42ClassType` 或 `Z42InterfaceType`~~ →
      **改为新增子类 `Z42InstantiatedInterfaceType : Z42InterfaceType`**（design D6 订正）：
      原方案要审 49 处 `.Def` + 44 处 `is Z42InstantiatedType`，子类方案 **0 处必改**。
      配套：`Z42InterfaceType` 去 `sealed` + 加 `TypeParamNames/TypeParamCount`；
      `Name()` **仍返回裸名**（带实参拼写会漂移 TSIG 字节，另给 `NameWithArgs()`）
- [x] 3A.2 `SymbolTable.ResolveTypeP` —— 泛型接口引用 `IFoo<int>` 解析成实例化接口类型
      （此前 `Interfaces.Find` 命中即返回裸接口、`nt.Args` 整条丢弃）。导入侧
      `ImportedSymbolLoader` 同步填型参名，与本包 `StubCollector` 对称
- [x] 3A.3 既有裸名匹配路径不回归 —— **完整 GREEN 全绿 + 自举字节不动点保持**（3/3 gen1==gen2），
      这是子类 + 裸名 `Name()` 两个选择共同保证的

### 3B Parser

- [x] 3B.1 `MemberParser._parseMemberBody` 拦截区加 `type Item;` / `type Item = X;` 三 token 前瞻
      （上下文关键字，**不进 lexer**）。⚠️ 已实测并**刻意接受**一处代价：类型名恰好叫 `type` 的
      字段声明（`type x;`）会被吃掉——全仓 `src/`+`examples/` 零命中、且无任何名为 `type` 的类型；
      换来 `type` 不进 lexer（进 lexer 会废掉所有把 `type` 当变量/参数/属性名的源码）
- [x] 3B.2 `TypeParser` 类型实参位支持 `Name = Type` 命名绑定，**且仅在 where 约束位开**
      （`_allowAssocBindings` 门控，`_parseConstraint` 进出成对开关）。普通类型位
      `List<Item = int>` 仍是语法错误——`=` 在类型位一旦全局放开就再收不回来
- [x] 3B.3 新增 `AssocBindingType : TypeExpr`（`Item = int`，长在 Args 槽里 ⇒ `NamedType` 一行不改）
      + `AssocTypeDecl : Decl`（`type Item;` / `type Item = int;`）
- [x] 3B.4 `decl.z42` +5 条：3 条正例 + 「普通类型位不得接受绑定」+ 「`type` 仍是普通标识符」。
      **3 条正例已实证是真门**（把两处支持改成 `if (false)` 后精确变红，两条守卫按预期保持绿）

### 3C 语义

- [ ] 3C.1 `MemberCollector` —— 收集接口的关联类型声明
- [ ] 3C.2 `Z42InterfaceType` —— 关联类型槽
- [ ] 3C.3 `GenericConstraint.ConstraintBundle` —— 承载关联类型绑定
- [ ] 3C.4 `ConstraintChecker` —— 绑定解析与校验；实现方未绑定报诊断

### 3D 格式（**双 bump**）

- [ ] 3D.1 `IrModule.IrConstraintDesc` —— 承载绑定
- [ ] 3D.2 `ZbcWriter` —— bit7 = `has_assoc_bindings` + `count u8` + `(name_idx, type_idx) × n`
- [ ] 3D.3 `ZbcReader` + Rust `type_reader.rs` + `ZpkgReader._skipConstraintBundle` —— **三方 reader 同步**（memory 教训：改 producer 必核 3 个 reader）
- [ ] 3D.4 `ExportedInterfaceZ` + `TsigReconcile` —— 关联类型名单跨包重建
- [ ] 3D.5 zbc minor 38→39 + zpkg minor 43→44，按 `version-bumping.md` checklist 同步 `versions.rs` / changelog / 10 个 committed fixture regen
- [ ] 3D.6 `cargo test --test format_fixture_versions` 绿

### 3E 测试与文档

- [ ] 3E.1 `constraint_tests.z42` —— 关联类型正/负例
- [ ] 3E.2 `src/tests/cross-zpkg/assoc_type_cross_pkg/` NEW
- [ ] 3E.3 `docs/book/src/language/generic-constraints.md` + `docs/design/language/generics.md`（L3-G3a 改为已实现）
- [ ] 3E.4 `docs/roadmap.md` —— 关掉 L3-G3a
- [ ] 3E.5 `./xtask test bootstrap` + 完整 GREEN

---

## 备注

### 实施期发现的 Scope 外缺口（**未修**，按规矩不顺手改）

1. **运行期加载校验只认 FQ、接口靠启发式豁免** —— `src/runtime/src/metadata/loader/constraints.rs`
   的 `check_one` 查 `module.type_registry`（键为 FQ），查不到就 `bail!` 让**整个模块加载失败**。
   接口约束一直没暴露这点，是因为它有一条「`I` + 大写开头就放行」的启发式（注释自陈 registry
   只装类、接口 soft-allow）。⇒ 一个**不以 `I` 开头的接口**用作约束，今天就会让模块加载炸。
   本 change 只按既有约定让 base 写 FQ 绕开，没动这条启发式。
2. **跨包泛型 `new T()` 不工作** —— `CtorBox<T> where T : new()` 的 `Make()` 里 `new T()` 报
   `class DemoCTarget.T not found in module registry`（把型参名当类名找）。
3. **跨包 enum 的 `ToString()` 返回序号** —— `Color.Green` 打印成 `1` 而非 `Green`。

2/3 与约束校验无关，只是 cross-zpkg 用例顺带撞上；用例已收窄为「构造即断言」避开它们。

- **support ≠ use**：本 change 全程**不改写真实源码使用新语法**（`INumber` / `Dictionary` /
  `Protocols` 保持旧写法）。use 改写等下一个 nightly 发布后另开 change（bootstrap-seed 轴① 铁律）。
- **Scope 扩张已计入**：design D8 发现的「接口 where 从不被 Resolve」已纳入 PR-1（1B.3 / 1C.1），
  proposal Scope 表已含相应文件。实施中若再发现 Scope 外文件 → **立即停下回阶段 3**。
- **已知踩坑**（上一轮实测）：手拼 zbc TYPE 段 fixture 必须读到尾（对象全字段布局块恒存在、非
  gated）；`grep -c "\[Test\]"` 会多数一条，核对用例数用 `grep -o "^void test_"`；后台任务的
  exit code 不可信，从 log 读 `REAL_EXIT=`。
- **恢复环境**：worktree `../z42-assoctypes`，分支 `add-associated-types`。**未供种** —— 开工前
  按 overlay 配方从同 sha 兄弟 worktree 拷 `artifacts/build/{libraries,compiler,toolchain}` +
  `artifacts/xtask`（**别拷 `artifacts/build/runtime/`**），再 `cargo build --release --bin z42vm`，
  每条命令带 `Z42_PORTABLE_VM=$PWD/artifacts/build/runtime/release/z42vm`。
