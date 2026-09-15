# Tasks: resolve-free-functions-by-namespace

> 状态：🟢 已完成（User 2026-09-15 确认 DRAFT）｜ 创建：2026-09-15 ｜ 叠在 #667（`add-call-arity-diagnostics`）之上

- [x] 1.1 普查：`MemberCollector` 挂同名重复注册探针，全仓 19 处——同 ns 跨文件 0、跨 ns 合法同名（multi-exe `Main`、z42c.semantics 四个单测文件的辅助函数）⇒ 否掉「只报跨文件重名」、改按 ns 解析
- [x] 1.2 最小复现坐实 B-2（成败看文件名）/ B-3（`using Beta` 也运行期 undefined）
- [x] 2.1 `SymbolTable.FunctionsByFqn` + `FuncNsAll`（新分部 `SymbolTable.Functions.z42`）：`AddFunc`（local-wins）/ `GetFuncIn` / `ResolveFuncNs`；`AmbiguousFuncNameMsg` 改用同一候选集 `_funcCandidates`
- [x] 2.2 登记：`MemberCollector` 按 `cu.Namespace`（同 FQN 跨文件 → E0408 带另一处位置）；`ImportedSymbolLoader` 按模块 ns；`SymbolCollector._mergeImports` 按 FQN 并入
- [x] 2.3 解析：`MemberResolver` 自由调用 / `ns.func()`、`ExprTyper` 方法组；`FuncImplExtractor` 按声明 ns 精确取
- [x] 2.4 发射：`BoundCall.FreeNs/FreeImported`、`BoundFuncRef.FuncNs/FuncImported` → `CallEmitter` / `ExprEmitter` 直接发 `QualOf(ns, name)`；删除 `QualifyFreeFunc` / `ImportedFuncNs` / `_filterShadowedFuncs` / `ImportedSymbols.Functions`·`FunctionNamespaces`
- [x] 3.1 单测 `typecheck/free_func_ns`（7 阳性 + 3 对照，发射断言读 IR dump）；e2e `multi-exe/free_func_cross_ns`（两个 exe：using / 方法组 / 全局 ns / 不写 using）
- [x] 3.2 退回对照：源码退回 #667 状态、保留单测 → 7 阳性 FAIL、3 条按设计 PASS（文件序恰好能过的那一序 / 既有 E0456 / 对照）
- [x] 4.1 文档：book `compiler/source-compile.md` 新节「本包自由函数按命名空间解析」（含解析伪代码）；四个单测文件「平坦命名空间」绕行注释加历史注
- [x] 5.1 冷种子 GREEN（build ×2 → `xtask test` 全绿、不动点 3/3）+ JIT cross-zpkg 54/54 + 全量 `cargo test` 1378；**字节对账**：stdlib 50 个产物对 #667 构建逐字节相同
