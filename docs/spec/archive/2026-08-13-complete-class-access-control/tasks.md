# Tasks: 补全类级访问控制

> 状态：🟢 已完成 | 创建：2026-08-13 | 完成：2026-08-13
> 一个 PR / 4 commit；commit 顺序 ③→④→②→①（尊重依赖）；均无格式 bump。

## 进度概览
- [x] 阶段 A（commit 1 = ③）：顶层 private/protected 拒绝（E0442）
- [x] 阶段 B（commit 2 = ④）：接口类型可见性 + CheckTypeRef 接口分支
- [x] 阶段 C（commit 3 = ②）：不一致可访问性（E0441）
- [x] 阶段 D（commit 4 = ①）：类可见性反射面（6 谓词）
- [x] 阶段 E：文档同步 + 归档

## 阶段 A：③ 顶层拒绝（parser，commit 1）
- [x] A.1 `DiagnosticCodes.z42` 加 `TopLevelAccessModifier = "E0442"`
- [x] A.2 `Parser.ParseCompilationUnit`（210-224）分派点：顶层 decl 的 mods 含 private/protected → `_diags.Error(E0442, ...)`
- [x] A.3 `access_control_tests.z42` 加 E0442 用例：顶层 private class / protected interface / protected enum / protected 函数 → 报；internal/public/默认/嵌套 private → 不报
- [x] A.4 局部验证：`xtask test compiler`（编译器单测）+ 全量 `xtask test`
- [x] A.5 commit `feat(compiler): 顶层声明拒绝 private/protected（E0442）`

## 阶段 B：④ 接口可见性（compiler，commit 2）
- [x] B.1 `Z42Type.z42`：`Z42InterfaceType` 加 `public string Visibility;` + ctor 默认 `"public"`
- [x] B.2 `SymbolCollector._passInterfaces`（~483）：`ift.Visibility = IrGenFacts.classVis(c.Mods, nested)`
- [x] B.3 `ClassDescBuilder._interfaceDesc`（~296）：`cd.Visibility = IrGenFacts.classVisCode(c.Mods, nested)`
- [x] B.4 `ExportedTypes.z42`：`ExportedInterfaceZ` 加 `Visibility` 字段（ctor 默认 public）
- [x] B.5 `TsigReconcile._rebuildInterface`（~323）：`eiz.Visibility = _visStr(cd.Visibility)`
- [x] B.6 `ImportedSymbolLoader` 接口路径：`nift.Visibility = il.Visibility; nift.IsImported = true`
- [x] B.7 `AccessChecker.CheckTypeRef`：加 `Z42InterfaceType` 分支（读 `.Visibility`，同 private/protected/internal 逻辑；`_denyType` 支持 "interface" 措辞）
- [x] B.8 `src/tests/cross-zpkg/interface_internal_access/`：包 A internal 接口 + 包 B 引用 → E0404（手工验证型或 expected_output）
- [x] B.9 验证：`xtask test`（重点 cross-zpkg + 自举 gen1==gen2 byte-identical；含 internal 接口的 zpkg 字节值变但格式不变）
- [x] B.10 commit `feat(compiler): 接口类型可见性建模 + 跨包 internal 接口引用强制`

## 阶段 C：② 不一致可访问性（compiler，commit 3）
- [x] C.1 `DiagnosticCodes.z42` 加 `InconsistentAccessibility = "E0441"`
- [x] C.2 `AccessChecker.z42`：加 `_visRank(vis)` + `_exposedVis(Z42Type)` + `static CheckExposure(declVis, exposed, ctx, symbols, diags, sp)`
- [x] C.3 `DeclBinder._bindClass`：对每个类遍历 base+ifaces（类 vis）+ 每个 FieldSymbol（字段 vis vs 字段类型）+ 每个 MethodSymbol（成员 vis vs ret+params）调 `CheckExposure`，emit 走 `this._tc._diags`
- [x] C.4 `access_control_tests.z42` 加 E0441 用例：public 方法返 internal / public 字段 internal / public 类继承 internal 基 / internal 方法返 private 嵌套 → 报；一致 + private 成员暴露 → 不报
- [x] C.5 验证：`xtask test`（编译器自举中若 stdlib 自身触发 E0441 需评估——见备注）
- [x] C.6 commit `feat(compiler): 不一致可访问性诊断（E0441，public 签名暴露低可见性类型）`

## 阶段 D：① 类可见性反射（runtime + stdlib，commit 4）
- [x] D.1 `bytecode.rs`：`ClassDesc` 加 `visibility: u8`
- [x] D.2 `types.rs`：`TypeDesc` 加 `visibility: u8`
- [x] D.3 `zbc_reader.rs`（525）：`_class_visibility` 改存入 `ClassDesc.visibility`
- [x] D.4 `loader.rs`：`ClassDesc.visibility` → `TypeDesc.visibility` 线程
- [x] D.5 `reflection.rs`：6 个 `builtin_type_is_*`（public/not_public/nested_public/nested_private/nested_family/nested_assembly），读 `td.visibility` + 名内 `+` 判嵌套
- [x] D.6 `corelib/mod.rs`：注册 6 builtin
- [x] D.7 `Type.z42`：6 个 `[Native]` extern bool 属性（C# 命名 + 注释）
- [x] D.8 `reflection_tests.rs`：`type_visibility_decode_*` Rust 单测
- [x] D.9 `src/tests/types/type_visibility.z42`：golden（顶层 public/internal + 嵌套四级 → 6 谓词断言）
- [x] D.10 `cargo test --lib`（Rust 单测）+ `xtask test`（含 e2e golden）
- [x] D.11 commit `feat(runtime): 类可见性反射面（Type.IsPublic 族，对齐 C#）`

## 阶段 E：文档 + 归档
- [x] E.1 `docs/design/language/access-control.md`：Phase 2 状态更新（四项补齐，去掉「未做：类级访问强制」段的 Deferred 表述细化）
- [x] E.2 `docs/book/src/compiler/access-control.md`：机制页补反射面 / 不一致可访问性 rank / 顶层拒绝 / 接口可见性
- [x] E.3 `docs/roadmap.md`：N/A——roadmap 未索引 access-control 的 access-future-* Deferred（仅 book 页有），无需改
- [x] E.4 `src/libraries/z42.core/README.md`：N/A——README 用通用 Type.z42 行（个别反射谓词如 IsSealed 亦不列，属 book 层，两层分工）
- [x] E.5 `.claude/rules/version-bumping.md` 第27-32行「当前值」表刷新（1/33、0/38；顺带清理漂移，小卫生项）
- [x] E.6 归档：move 到 `docs/spec/archive/2026-08-13-complete-class-access-control/`
- [x] E.7 PR：push 分支 + `gh pr create`（body 三段 + 页脚）；合并前 rebase origin/main + 重跑 GREEN

## 备注
- **④ 调试教训（跨包 fixture 手工验证陷阱）**：验证跨包接口 E0404 时，`Z42_LIBS` 必须指向**新鲜重建**的
  stdlib（`artifacts/build/libraries/dist/release/`），**不能**用 `.z42/libs`（种子=旧 z42.ir，缺
  `ExportedInterfaceZ.Visibility` 字段 → 运行时字段读为 null → CheckTypeRef 匹配不上 "public" → 落入
  internal 分支把 public 接口也误拒）。driver 运行时从 Z42_LIBS 加载 z42.ir，须与编译期一致。
- **stdlib 自触发 E0441 风险（重点盯 C.5）**：加 E0441 后，stdlib 现有源码若存在「public 成员暴露 internal 类型」会在自举时报错 → 属真实不一致，需就地修（改类型或调可见性）或评估是否规则过严。这是 ② 的主要未知量，实施时先 grep 高危模式，编译器自举全绿为准。
- **环境**：worktree `z42-acl3` 基于 origin/main（f9928607，含 #184，zbc 1.33/zpkg 0.38）。需 0.38 warm 种子（无 bump → 一次供种后 warm 全程可验，不走两代自举）。
- **无格式 bump**：四项 zbc/zpkg 格式字节不变；自举 byte-identical 不动点应保持（接口 vis 值变属产物内容）。
