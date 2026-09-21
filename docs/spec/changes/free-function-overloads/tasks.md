# free-function-overloads — 实施任务

按依赖序。每步后 build z42c + 跑相关测试。

## 阶段 1（support 先行）

- [x] **T1 键规则**：`MemberCollector._passMembers` 自由函数登记处加**包级 per-ns** 声明序 tracker
      （`SymbolCollector._freeFnLocalSeen`），算 `RegKey`（primary 裸 / 非-primary `MangleKey`），
      写 `md.RegKey` + `MethodSymbol.RegKey`。
- [x] **T2 候选集**：`SymbolTable` 加 `FuncOverloadsByFqn`（基名 FQN → `MethodSymList`）+
      `GetFuncCandidates(ns,name)`；`FunctionsByFqn` 键改为 RegKey-限定 FQN；`GetFuncIn` 仍命中 primary。
- [x] **T3 调用点决议**：`MemberResolver` 裸名 + `ns.f` 两路改走候选集 + `OverloadBinder._resolveFreeOverload`
      （抽出 `_resolveOverloadCands` 与类方法共用）；`BoundCall.MethodName = 选中.RegKey`；no-match/歧义诊断。
- [x] **T4 重复检测**：删 `DeclBinder._checkDuplicateFreeFunctions`；收敛到 `MemberCollector` 的包级全签名
      tracker（`_freeFnSigSeen`），仅同签名报 E0408，且**不注册重复份**（免调用点级联 E0425）。
- [x] **T5 发射**：`IrGenAuxEmitter.EmitFreeFunctions` + `DeclBinder._bindFreeFunc` body 键 `md.Name`→`md.RegKey`。
- [x] **T6 跨包**：`FuncImplExtractor._extractFunc` 按 RegKey 精确取 + 按 RegKey 导出；
      `ImportedSymbolLoader` 基名 = RegKey 剥首个 `$`、RegKey 存符号；`SymbolCollector` 合并按基名分组。
- [x] **T7 取引用 v1 限制**：`ExprTyper` 方法组指向多重载自由函数 → E0425（无歧义放行）。
- [x] **T8 入口实测**：`build all` 自建 + e2e Main 程序跑通（D7 确认，`Main` 天然 primary 裸键）。

## 验收

- [x] **G2** 字节对账：`test compiler` 自建不动点 3/3 gen1==gen2 逐字节相同（零 bump 硬证据）。
- [x] **G3** 正例：`src/tests/basic/free_function_overloads`（arity + 类型重载）interp + jit 双绿。
- [x] **G4** 负例：同签名重复报 E0408（指双方，单条诊断）；重载 no-match/歧义有诊断；单测三条。
- [x] **G1** 编译器相关阶段全绿：`test compiler`（不动点+单测）/ `test e2e`（全量 654+60+3，interp+jit）/
      `test e2e --dir cross-zpkg`（含新 fixture）/ `test examples basics/functions` / `test stdlib` / `test lines` /
      `test docs`。（`test runtime` = Rust 单测，与本编译器改动无关且易挂死，未跑；`test fingerprint` 需 CI 的
      `--base`，本地不可单跑，其守护的字节稳定由不动点 3/3 已证。）
- [ ] **G5** 冷启动自建 `compile-toolchain`（CI 验；本地全量 `build all` 自建已过）。

## 文档

- [x] **B1** 教程 [functions.md] 「自由函数也能重载」+ 小结 + 示例 overload.z42/dup.z42/run.console。
- [x] **B2** reference（functions/syntax/parameter-modifiers）+ internals `source-compile.md` 机制页 + TypeChecker/builder 注释同步。
- [x] **B3** doc-check：用户可见 → learn+reference✓；接手可懂 → internals 机制页✓；目录/入口未变 → 无 README 改动。`test docs` 绿。
