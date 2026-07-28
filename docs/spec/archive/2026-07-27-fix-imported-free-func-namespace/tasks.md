# Tasks: 导入自由函数按源命名空间限定（fix-imported-free-func-namespace）

> 状态：🟢 已完成 | 创建：2026-07-27 | 完成：2026-07-27 | 类型：fix（compiler codegen 根因修复）| 占用子系统：`compiler`
> 分支：worktree `fix-imported-free-func-ns`（User 授权隔离预抢，compiler 锁由 nested-types-followup 持有）

**变更说明：** z42c 发射跨包裸调用**导入的自由函数**时，误把它限定到**当前命名空间**而非其**源命名空间** → 跨包/跨 ns 裸调用 emit 出错误 FQN → 运行期 undefined。

**原因（根因）：** 类有 `ImportedClassNs`（[EmitContext.QualifyClass](../../../../src/compiler/z42c.semantics/src/EmitContext.z42)）跟踪导入类的源 ns；**自由函数没有对应机制**——[ImportedSymbolLoader.z42:156](../../../../src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42) seed 时源 ns 丢弃（owner 传 `""`），[ExprEmitter.z42:724](../../../../src/compiler/z42c.semantics/src/ExprEmitter.z42) 只能 `Qualify(当前ns)`。修法 = 给自由函数补一条与类对称的「源 ns 跟踪 + 限定」路径。

**动机：** REPL 每轮独立包 + 唯一命名空间 `Repl.R{N}`，跨轮调用上一轮定义的自由函数即触发此 bug（[add-repl-decls-multiline] 依赖本修复）。此外这是**通用缺陷**：任何 z42 跨包裸调导入自由函数都错，非 REPL 专属。

**字节不动点安全性（已勘定）：** 全 `src/compiler/` + `src/libraries/` 中 public 命名空间级自由函数 = **0**（194 个带缩进的均为类方法）。→ `ImportedSymbols.FunctionNamespaces` 对所有现存编译恒空 → `QualifyFreeFunc` 恒回落 `Qualify` → **z42c 自举 gen1==gen2 逐字节不变 + stdlib 零字节变化**。本修复纯粹解锁一个当前坏掉的新行为（首个消费者 = 新增 cross-zpkg 测试 + REPL）。

**文档影响：** `docs/design/compiler/self-hosting.md` 或 codegen 机制页记「自由函数跨包限定」与类对称（归档时按触发矩阵）；z42c.semantics README 功能索引（如涉及入口）。

## 进度概览
- [ ] 1. 核心修复（6 文件，镜像类的 ImportedClassNs 机制）
- [x] 2. 回归测试（cross-zpkg 裸调导入自由函数）
- [x] 3. 验证（自举字节不动点 + 关键 gate）+ 文档同步 + 归档

## 1. 核心修复
- [x] 1.1 `ImportedSymbolLoader.z42`：`ImportedSymbols` 加 `StrMap FunctionNamespaces`（ctor 初始化）；自由函数 seed 处 `FunctionNamespaces.Put(fz.Name, new StrBox(em.Namespace))`（first-wins 守卫内）
- [x] 1.2 `IrGen.z42`：加字段 `StrMap ImportedFuncNs`（镜像 `ImportedClassNs`）
- [x] 1.3 `EmitContext.z42`：加字段 `StrMap ImportedFuncNs`（ctor 置 null）+ 方法 `QualifyFreeFunc(name)`（命中 → 源 ns 限定 + `TrackDepNamespace`；否则回落 `Qualify`）
- [x] 1.4 `FunctionEmitter.z42`：3 处 `_ctx.ImportedClassNs = _gen.ImportedClassNs` 后各加 `_ctx.ImportedFuncNs = _gen.ImportedFuncNs`
- [x] 1.5 `IrDump.z42`：加 `_filterShadowedFuncs(imported, cu)`（镜像 `_filterShadowed`，扫顶层 `MethodDecl` 做 local-wins 剔除）；`_compileCu` + `BuildModuleD` 各设 `gen.ImportedFuncNs = _filterShadowedFuncs(imported.FunctionNamespaces, cu)`
- [x] 1.6 `ExprEmitter.z42`：free 调用分支改用 `QualifyFreeFunc`——重构为 if(local-lifted-fn)/elseif(static→QualifyClass)/else(free→QualifyFreeFunc)，TrackDepNamespace 副作用只对真·free 触发
  - 实施期修正：局部变量名 `fn` 撞 z42 保留关键字（E0202）→ 改 `fname`（`_filterShadowedFuncs` 内）

## 2. 回归测试
- [x] 2.1 `src/tests/cross-zpkg/free_func_cross_pkg/`：target（ns `Demo.FreeFuncBase`）定义 ns 级自由函数 `Square`；ext（`using` target）裸调 `Square`（ext→base 跨包 free call）；main 裸调 `Square` + `SquareViaExt` → expected `25` / `36`
- [x] 2.2 **旧 z42c 复现**：`undefined function Demo.FreeFuncApp.Square`（限定到调用方 ns）；**新 z42c**：build-tree 手动 build target/ext/main + run → `25` / `36` 正确

## 3. 验证 + 归档
- [x] 3.1 `cargo build --release`（z42vm）—— ✔（build compiler 内含，1m09s finished）
- [x] 3.2 `xtask test compiler` —— ✔ **z42c 自举不动点 5/5 gen1==gen2**（含 z42c.semantics）+ z42c e2e 全过
- [x] 3.3 `xtask build stdlib` —— ✔ **25/25 succeeded, 0 failed**（我的 z42c 编 stdlib 无回归）
- [x] 3.4 文档同步：`docs/design/compiler/compiler-architecture.md`「QualifyClassName」节后加「QualifyFreeFunc」子节
- [x] 3.5 归档（mv + ACTIVE.md 摘除本 change 的预抢 note；compiler 锁仍由 nested-types-followup 持有，本 change 是隔离预抢、不占该行）

## 备注
- **local 全量 `xtask test` 的 cross-zpkg stage 在 Z42_HOME 下走 SDK 旧 z42c**（toolchain 路径），故新用例 `free_func_cross_pkg` 在该路径会「复现 bug」——只能在 **build-tree 路径 / CI**（从源码建 z42c）正确通过。已用 build-tree z42c 手动端到端证过（25/36）。**full GREEN 以 CI 为权威**（隔离预抢惯例，与 self-host 不动点本地已过一致）。
- 首个消费者是 2.1 的新测试 + 后续 `add-repl-decls-multiline`（B）。B 依赖本修复进 REPL 运行期加载的 z42c。
- local-wins：`_filterShadowedFuncs` 从 `ImportedFuncNs` 剔除本地声明的同名自由函数（镜像类），防「本地 + 导入同名」误绑到导入那份。
- 字节不动点安全性勘定：全 compiler+stdlib 命名空间级自由函数被跨包裸调 = 0 → 修复对现存代码惰性（不动点 5/5 + stdlib 25/25 实证）。
