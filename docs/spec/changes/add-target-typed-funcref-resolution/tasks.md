# Tasks：自由函数取引用的 target-typed 消解（add-target-typed-funcref-resolution）

> 状态：🟢 **IMPL + GREEN 完成（2026-09-22，待落地）** | 创建：2026-09-22 | 见 [proposal](proposal.md) / [design](design.md) / [spec](specs/funcref-resolution/spec.md)
>
> 本变更属 **lang 类**，走「DRAFT → User 确认 → IMPL → GREEN → COMMIT」。① 组前置验证全部通过、无返工。

## ① 前置验证（全部通过 ✅）

- [x] 1.1 **LoadFn @非-primary RegKey 运行期可解析** —— ✅ **代码级 + 运行期双证**：`load_fn` 存
      `FuncRef(name)`，`call_indirect`（`exec_call.rs:383`）用 `module.func_index.get(fname)` 解析——与
      free-call **同一张表**。运行期手验（scratch `frtest.z42`，四位含实参位）interp + jit 均输出 `1 2 1 2`，
      非-primary `@App.Kind$1$long` 正确调用。零 VM 改动支点成立。
- [x] 1.2 **跨包非-primary 取引用** —— ✅ 新 fixture `src/tests/cross-zpkg/funcref_cross_pkg`（target 导出
      重载 `Kind(int)`/`Kind(long)`，main target-typed 取引用）`PASS`（cross-zpkg 65/0）；输出 `1 2`。
- [x] 1.3 **存量字节稳定** —— ✅ 不动点 **3/3 gen1==gen2 逐字节**（primary RegKey==FuncName，既有发射零变）。

## ② 实现（全部完成 ✅）

- [x] 2.1 `BoundFuncRef` 增 `RegKey` 字段 + 构造器参（`BoundExpr.z42`）；发射改 `QualOf(FuncNs, RegKey)`（`ExprEmitter.z42`）。
- [x] 2.2 `_bindFuncRefTargeted` + `BindWithTarget` 加 `IdentExpr + Z42FuncType` 分支。**因 ExprTyper.z42 超 886 行硬限，
      拆 partial `ExprTyper.Funcref.z42`**（`_bindFuncRefTargeted` / `_funcCandSigs` / `IsOverloadedFuncRefArg`）。
- [x] 2.3 变量声明接线（`StmtBinder._bindVarDecl`）：`declared is Z42FuncType && v.Init is IdentExpr` → `BindWithTarget`。
- [x] 2.4 return —— **无需改动**：`_bindReturn` 已走 `BindWithTarget(r.Value, ret, env)`，新分支自动生效。
- [x] 2.5 实参位：`IsOverloadedFuncRefArg` 接入延迟点（`MemberResolver._bindCall`）与 arg-shape（`OverloadBinder._typeOfArgExpr`）。
- [x] 2.6 诊断 **E0477**（原拟 E0475，扫描发现 E0475/E0476 已被 #741/TypeParser 占用 → 改 E0477）；E0425 消息更新。
- [x] 2.7 E0425 无目标路径保留（`_bindIdent` 多重载）。

## ③ 测试（GREEN 全绿 ✅）

- [x] 3.1 typecheck 单测 `funcref_target_typed_tests.z42`（8 例：S1/S3/S4/S5/S6/return/field + long 非-primary dump 断言）。
- [x] 3.2 运行期 interp+jit 手验 `1 2 1 2`；S8 cross-zpkg fixture PASS。
- [x] 3.3 E0477 自带 fixture（S4）+ E0425 对照（S5），断言精确条数。
- [x] 3.4 不动点 3/3 gen1==gen2。
- [x] 3.5 `test compiler`（24 单元）+ `test lines` ✓ + `stdlib` interp/jit 340/340 + `e2e` 662/0 + cross-zpkg 65/0。
- [ ] 3.6 全量 CI GREEN（含 compile-toolchain × 平台，冷启动能编——本地测不到，靠 PR CI 兜底）。

## ④ 文档同步（完成 ✅）

- [x] 4.1 `docs/reference/src/language/delegates-events.md` 新增 §2.5「重载自由函数取引用：按目标委托消解」+ §2.2 表补链接。
- [x] 4.2 `docs/reference/src/appendix/error-codes.md` 补 E0477 行；E0425 行补无目标 funcref 触发面。
- [x] 4.3 `docs/internals/src/runtime/delegates-events.md` §2.2 补「typer 选键、emitter 发键」机制 + 精确匹配理由 + 实例方法组后续项。
- [ ] 4.4 更新 `free-function-overloads-program` 记忆：D5 遗留闭合；记录实例方法组拆分为独立 change。（落地后）

## ⑤ 拆分出去的独立 change（本变更不做，仅登记）

- [ ] 5.1 **实例/静态方法组 `obj.M` / `T.M` 取引用的 target-typed 消解**——触及 thunk VCall 定向非-primary
      同-arity 重载（大概率 VM vtable）+ 现状静默选 primary 的行为决策。单独立项 DRAFT。
