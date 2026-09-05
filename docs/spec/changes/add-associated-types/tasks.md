# Tasks: 泛型表达力 —— 跨包约束持久化 + `Self` + 关联类型

> 状态：🟡 进行中 | 创建：2026-09-05 | 范围裁决：2026-09-06

## 进度概览

- [ ] **PR-1** 跨包约束持久化 + flag 位 + 认知修正（零格式 bump）
- [ ] **PR-2** `Self` 类型（仅接口）
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

- [ ] 1B.1 `src/libraries/z42.ir/src/IrModule.z42` —— `IrConstraintDesc` 加 5 个承载位（`RequiresClass` / `RequiresStruct` / `BaseClass` / `RequiresCtor` / `RequiresEnum`）
- [ ] 1B.2 `src/compiler/z42c.semantics/src/ClassDescBuilder.z42:278-317` —— special 约束不再丢弃；base 与 interface 分开承载（今天混塞进同一 `Interfaces` 数组）
- [ ] 1B.3 `ClassDescBuilder._interfaceDesc:341-344` —— 接口 bundle 不再恒空，填入接口自身的 where（依赖 1C.1）
- [ ] 1B.4 `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42:280-298` —— 置全 bit0/1/2/4/5（今天只写 bit3）
- [ ] 1B.5 `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42:459-476` —— bit2 base 由「读而不存」改为存入承载位（注释已留接口）
- [ ] 1B.6 实测确认**零格式 bump**：regen fixture 后 `cargo test --test format_fixture_versions` 绿，zbc 仍 38 / zpkg 仍 43

### 1C 语义层（约束模型 + 键规则）

- [ ] 1C.1 `ConstraintChecker.Resolve:38` —— 条件由 `class || struct` 扩到含 `interface`（design D8）
- [ ] 1C.2 `SymbolTable.z42` —— 新增 `ConstraintKey(Z42ClassType)` 单一辅助（规则同 `Classes` 的条件 arity-mangle）
- [ ] 1C.3 `ConstraintChecker.z42:40 / :122` —— 写入与查询改调 `ConstraintKey`，消掉同名多 arity 的 last-wins 串味
- [ ] 1C.4 `GenericConstraint.z42` —— 新增「从导入元数据构造 `ConstraintSet`」的入口

### 1D 跨包搬运

- [ ] 1D.1 `src/libraries/z42.ir/src/ExportedTypes.z42` —— `ExportedClassZ` 加约束字段（**不进 ctor 签名、ctor 给默认值、构造后赋值**）
- [ ] 1D.2 `src/libraries/z42.ir/src/TsigReconcile.z42:508-523` —— `_rebuildClass` 读 `cd.TypeParamConstraints` 搬进 `ExportedClassZ`（今天一次都没读过）
- [ ] 1D.3 `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` —— seed 导入类型的约束进 `SymbolTable.ClassConstraints`（用 1C.2 的键）

### 1E 落地强度与 🔴 风险（design D4 / D7）

- [ ] 1E.1 新接通的跨包校验**先以 warning 落地**，不翻 error
- [ ] 1E.2 跑完整 `xtask test` + 自举，收集探针输出
- [ ] 1E.3 若出现 `Dictionary<int,int>` 一类基元误报 —— **不绕过、不加特例**，查 `_satisfiesInterface` 对基元实参的归一路径，与运行期 `generics.rs::constraint_satisfied_by` 对账
- [ ] 1E.4 零误报后，**同一 PR 内**翻成 error
- [ ] 1E.5 给 `src/tests/types/struct_generic_container.z42` 的 `struct P` / `struct Tagged` 补 `: IEquatable<>`（上一轮顺延来的处置）

### 1F 测试

- [ ] 1F.1 `constraint_tests.z42` —— 补本包正/负例（含接口 where、同名多 arity 不串味）
- [ ] 1F.2 `src/tests/cross-zpkg/generic_constraint_cross_pkg/` NEW —— 七类约束跨包各一条正例 + 负例
- [ ] 1F.3 `xtask test stdlib --mode jit` —— 运行期五个死分支首次被执行，须补跑 JIT 模式

### 1G 文档与归档

- [ ] 1G.1 `docs/book/src/language/generic-constraints.md` —— 已知限制 §1 由「不校验」改为「已校验」；补跨包链路机制（含 ASCII 链路图）
- [ ] 1G.2 `docs/roadmap.md` —— 关掉 `where-constraint-future-crosspkg` 与 `-runtime-flags`；登记 design 的三条新 Deferred
- [ ] 1G.3 归档动作随 PR 内落地（**禁止合并后单独 push 归档**）

---

## PR-2：`Self` 类型（仅接口）

- [ ] 2.1 `Z42Type.z42` —— `Z42InterfaceType` 加型参名槽（今天只有 `Z42ClassType` 有 `GenericParamNames`）
- [ ] 2.2 `SymbolTable.ResolveTypeP` —— 「当前所属类型是接口」时把 `NamedType("Self")` 解析成 `Z42GenericParamType("Self")`
- [ ] 2.3 `TypeEnv.z42` —— 透传「当前所属接口」上下文（已有 `ClassName` 字段与 :86-97 的改写先例可参照）
- [ ] 2.4 `MemberCollector._fillInterface` —— 建立 `Self` 绑定并传入成员解析
- [ ] 2.5 `ClassExtractor._extractInterface` —— 导出时 `Self` 编码为裸字符串（与型参 `T` 同款）
- [ ] 2.6 `ImportedSymbolLoader` —— 接口方法解析改用**带型参上下文**的 `_resolve` 四参版，型参集 = 接口 `TypeParams` ∪ `{"Self"}`（**顺带修既有缺口**：今天跨包接口方法里的 `T` 落到 `Z42ClassType.Builtin("T")` 兜底）
- [ ] 2.7 确认类里写 `Self` 落到 E0443，不新增错误码
- [ ] 2.8 `constraint_tests.z42` —— `Self` 正/负例（含 `where K : IEquatable` 省略实参、类里用 `Self` 报错）
- [ ] 2.9 `src/tests/cross-zpkg/self_type_cross_pkg/` NEW —— 跨包 `Self` 往返
- [ ] 2.10 `docs/book/src/language/generic-constraints.md` —— `Self` 语义与「仅接口」作用域
- [ ] 2.11 `docs/roadmap.md` —— 关掉 `where-constraint-future-type-arg-matching`
- [ ] 2.12 `./xtask test bootstrap` —— 新语法必跑（确认没在 z42c 源里越界使用）

---

## PR-3：关联类型

### 3A 地基：泛型接口实例化

- [ ] 3A.1 `Z42Type.z42` —— `Z42InstantiatedType.Def` 提升为可承载 `Z42ClassType` 或 `Z42InterfaceType`
- [ ] 3A.2 `SymbolTable.ResolveTypeP` —— 泛型接口引用解析成实例化接口类型
- [ ] 3A.3 确认既有裸名匹配路径（`_satisfiesInterface`）不因地基变化而回归

### 3B Parser

- [ ] 3B.1 `MemberParser._parseMemberBody:107-133` —— 在 `_parseType()` **之前**的拦截区加 `type Item;` 三 token 前瞻分支（上下文关键字，**不进 lexer**）
- [ ] 3B.2 `TypeParser._parseType:105-122` —— 类型实参位支持 `Name = Type` 命名绑定
- [ ] 3B.3 `TypeExpr.z42` / `Decl.z42` —— 承载绑定与关联类型声明节点
- [ ] 3B.4 回归确认 `type` 仍可作普通标识符

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
