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
- [ ] 1.7 （晚一个 nightly）driver `--emit-zbc` 包激活改用 `ActivationNsOf`

## PR-2 立门
- [ ] 待 PR-1 合入后展开

## PR-3 多余 using 告警
- [ ] 待 PR-2 合入后展开
