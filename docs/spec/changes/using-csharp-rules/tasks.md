# Tasks: `using` 按 C# 规则

设计见 [design.md](./design.md)。

## PR-1 外围命名空间（using-csharp-enclosing-ns）
- [x] 1.1 修复前红：`z42c.semantics/tests/typecheck/enclosing_ns/`（外围胜过 using / 无 using 可见 / 打破 using 间歧义 /
      自由函数同款；按段不按串前缀的反例）+ `z42c.pipeline/tests/pkgcompile` 的
      `test_enclosing_namespace_of_dependency_needs_no_using`（包激活 + E0436，修前红已验）
- [x] 1.2 `NsScope`（`Chain` / `IsEnclosing`）；`SymbolTable.ScopeChain`（`_setScope` 时算一次）
      ⚠️ 全局 ns（""）是每个文件**最外层**的外围（C#：文件级 using 挂在全局那层，先看全局成员再看 using）。第一版漏了，
      GREEN 的 golden 重生成抓到（`stdlib_generic_shadow` / `generic_stack`：无 namespace 文件的本地 `Stack` 与
      `using Std.Collections` 的 `Stack<T>` 被判歧义）；补 `test_global_namespace_declaration_wins_over_using` 守住。
- [x] 1.3 解析器 ①（`_resolveClass`、`ResolveTypeP` 接口 / 类两段）走外围链
- [x] 1.4 `IsBareNameAmbiguous` / `IsBareIfaceNameAmbiguous` / `IsScopeVisibleNs` / `_funcCandidates` /
      `TypeChecker._isVisibleNs` / `_enforceFileScope` / `_activeNamespaces` / 包激活（`IrDump.ActivationNsOf`）
- [x] 1.5 字节：`test fingerprint` 19 包逐字节一致；诊断与同名冲突时的解析答案会变 ⇒ 指纹条目
- [x] 1.6 文档：参考手册 namespaces.md 规则 5 + E0436 口径；semantics README
- [x] 1.7 （晚一个 nightly）driver `--emit-zbc` 包激活改用 `ActivationNsOf`（nightly main@77a6bfc 已含该符号）；
      golden `src/tests/basic/enclosing_ns_activation`（`namespace Std.Collections.Probe;` 不写 using 用 `Stack<T>`，修前 E0443）

## PR-2 立门（using-csharp-strict-gate）
- [x] 2.1 修复前红：`z42c.pipeline/tests/usinggate/`（同包跨 ns 的裸名 / 限定名 / 静态调用 / enum 常量 / 自由函数 /
      声明位四种 / delegate；依赖包类型只在字段类型里出现）+ 对照（有 using / 外围 / 集合字面量 / 元组）
- [x] 2.2 `NsUseRecorder` + `SymbolTable.UseRecorder`（只挂在 `Infer` 的本文件视图上，结束停用）；`ResolveTypeP` 拆外壳 /
      `_resolveTypePCore`，外壳按结果记 ns（别名目标暂停记录）；`DelegateNs`
- [x] 2.3 表达式位：静态成员读 / 静态调用 / ns 限定静态调用 / enum 常量 / 自由函数调用 / 函数引用（两处）
- [x] 2.4 声明位：`DeclTypeUses` 在 `Infer` 末尾补录
- [x] 2.5 合成类型 `NamedType.Synth`：集合字面量、元组、`_typeToTypeExpr`、Bencher、`typeof` 的 Type、methodof 的
      MethodInfo、AttributeSynth 的返回类型、ConstBlob
- [x] 2.6 `_enforceFileScope` 判据 = `UsedNs` ∪ `UsedDepNs`（cached 文件只有后者）
- [x] 2.7 仓库违规**拆成先行 PR（PR-2a）**——bench-pr 用 PR 的 driver 编 base 的编译器源码，门与补 using 同 PR 会把 base 编红
      （构建循环 + 自动补 using 脚本找出）：stdlib 3（z42.core ×2、z42.net 测试）、编译器 9（z42.build、z42c.semantics ×7、
      z42c.pipeline）、测试 2、multi-exe fixture 2（`free_func_cross_ns` 原测「不写 using 也能调」的旧规则，改补 using）、
      学习手册示例 1（organization/ns）
- [x] 2.8 文档：参考手册 namespaces.md（E0436 口径、「用到」的定义、合成类型例外）；学习手册 organization 章 + OUTLINE；README
- [ ] 2.9 （晚一个 nightly）`UsedNs` 持久化进 cache meta（driver 的 IncrementalDriver / CachedNsMeta）—— 此前 cached 文件
      只按 `UsedDepNs` 判：另一个文件删掉 `global using` 时，cached 文件的同包跨 ns 引用不会报 E0436
  - [x] 2.9a pipeline 侧（support）：`CacheMeta.UsedNs` + `usedns` 行（MetaVersion 8→9）、`CachedNsMeta.UsedNs`、
        `PackageCompile` 回填。用例 `usinggate` 的 cached 回填（关掉回填即红）+ `incremental` meta 往返
  - [x] 2.9b driver 侧（use；nightly main@d90720b 已含 2.9a）：`IncrementalDriver` 写 `m.UsedNs`、`Main` 构造
        `CachedNsMeta` 时带上（meta 读回的数组带空槽，按 `UsedNsCount` 截成精确长度）。
        ⚠️ 原先写的场景（「另一个文件删掉 global using 时 cached 文件不报 E0436」）经 driver **打不中**：global using
        变了增量规划整包重编（实测 `cached: 0/2`）。2.9b 真正的消费方是 3.6（全包判 global using 要看 cached 文件的用法）。
        e2e `_e2eUsedNsCacheChecks`：只改 f0 注释让 f1 命中缓存，global using 不误报（撤掉 driver 两处即红，实测）
- [ ] 2.10 （可选，C# 口径）E0436 只看 `UsedNs`：`UsedDepNs` 按接收者类型记实例调用，C# 不算「用到」；需 2.9 落地后再收

## PR-3 多余 using 告警（using-csharp-unused-warning）
- [x] 3.1 用例（`usinggate` 单元）：没用到 / 只在声明位用到（不报）/ prelude / 外围 / 同文件重复 / 已有 global using /
      有编译错误时不报
- [x] 3.2 `UsingLint`（`CuCompile` 代码生成之后，判据 = `UsedNs` ∪ `UsedDepNs`，与 E0436 同一份）；W0607 / W0608
      进登记表，发射点先用字面量（diag-literal-emitters.txt）
- [x] 3.3 `UsingDecl.Injected`（global using 注入副本不报）/ `CoveredByGlobal`（注入时因已存在而跳过 ⇒ W0608）
- [x] 3.4 清理仓库多余 using（构建循环 + 自动删除脚本）。删掉的只有两类：「文件里没用到」与 prelude；没有「外围 ns」
      那一类 ⇒ 不依赖 PR-1 进种子（上一版 z42c 照样能编）
- [x] 3.5 文档：参考手册 namespaces.md「多余的 using 会告警」+ 诊断表；error-codes.md
- [x] 3.6 `global using` 声明本身全包都没用到 ⇒ W0607（`UsingLint.CheckGlobalUsings`，挂在 `EnforceFileScopeAll` 末尾：
      缓存回填之后、全包文件齐了才判；包里有错误时不判）。顺带修 PR-3 的误报：`namespace A;` 的文件里写 `global using A;`
      曾按「本文件 ns」报 W0607 —— 外围判据对 global using 不适用。用例 `usinggate` 两条
- [x] 3.7 （晚一个 nightly）W0607 / W0608 发射点切回 `DiagnosticCodes.UnnecessaryUsing` / `DuplicateUsing`
