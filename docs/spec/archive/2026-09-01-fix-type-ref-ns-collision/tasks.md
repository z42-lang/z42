# Tasks: fix-type-ref-ns-collision

> 状态：🟢 已完成 | 创建：2026-08-31 | 完成：2026-09-01（PR #353）| 类型：fix

**变更说明：** 修复 z42c 对**同短名跨 namespace 类型**的解析/发射缺陷——`new A.Foo`（`A.Foo`/`B.Foo` 同短名不同 ns）
被编译成 `new B.Foo`（撞 first/last-wins 赢家），致对象身份、`is`/`as`、`GetType().FullName` 全反。

**原因：** z42c `SymbolTable.Classes` 按**裸名**建键、同短名跨 ns 碰撞 first/last-wins；`ResolveTypeP` 的
限定名路径把 `A.Foo` 剥成 `Foo` 再查裸名 map，无视 `A.` 限定符 → 解析到赢家（B.Foo）。且 `ObjNew` 发射端
`CallEmitter` 用 `QualifyClass(短名)` 经 `ImportedClassNs` 再限定，同样撞短名赢家。
（是 [[fix-crosspkg-static-ns-collision]] 同类 bug 在**类型引用**上的遗漏——那次只修了静态调用。）

**根因修复（不打补丁）：** 让类型带上声明 namespace + 按 FQN 精确解析 + 发射端用已解析类型的 FQN。

## 设计（ns-aware，镜像 GetStaticScoped）
- `Z42ClassType` 加 `Namespace` 字段 + `Fqn()`（`ns.IrName()`；ns 空→裸名）。
- `SymbolTable` 加 `ClassesByFqn`（FQN 键→类型），与裸名 `Classes` 并存：保留每一份同短名类。
- 本地类注册（`StubCollector._putClassStub`）：从 `cu.Namespace` 设 `Namespace` + 登记 `ClassesByFqn`。
- `ResolveTypeP` 限定名路径：优先按 FQN 命中 `ClassesByFqn`（`A.Foo`→ns==A 的那份），不再剥短名撞赢家。
- `CallEmitter` obj_new：已解析类型 `Namespace!=""` → 直接发 `Fqn()`，绕开 `QualifyClass` 短名歧义。
  （is/as 本就发 AST 源码原始名 → 已正确，不改；imported 类**本 change 不设 ns**、走原路径，无回归。）

## 文档影响
- `docs/book/`：编译器类型解析机制页补「同短名跨 ns 按 FQN 解析」一节（归档前）。
- `.claude/rules/common-pitfalls.md` §1：短名 first-wins 已有条目，本 fix 是其在类型引用维度的又一实例，补一句关联。

## Scope（本 change 允许改动）
- `src/compiler/z42c.semantics/src/Z42Type.z42` — MODIFY：Z42ClassType +Namespace +Fqn()
- `src/compiler/z42c.semantics/src/SymbolTable.z42` — MODIFY：+ClassesByFqn + 限定名 FQN 解析
- `src/compiler/z42c.semantics/src/StubCollector.z42` — MODIFY：本地类设 ns + 登记 ClassesByFqn
- `src/compiler/z42c.semantics/src/CallEmitter.z42` — MODIFY：obj_new 用 resolved 类型 Fqn
- `src/tests/multi-exe/ns_same_short_name/` — NEW：回归 fixture（2 probe exe）
- `docs/book/src/...`（编译器类型解析页）— MODIFY：机制补节
- `docs/spec/changes/fix-type-ref-ns-collision/` — NEW：本容器

## 任务
- [x] 1.1 复现确认（interp+jit `new A.Foo`→B.Foo）+ 根因定位（ObjNew class_name="B.Foo"，SymbolTable 短键）
- [x] 1.2 Z42Type：Namespace 字段 + Fqn()
- [x] 1.3 SymbolTable：ClassesByFqn + 限定名 FQN 解析
- [x] 1.4 StubCollector：本地类 ns + 登记 ClassesByFqn
- [x] 1.5 CallEmitter：obj_new 用 resolved 类型 Fqn
- [x] 1.6 回归 fixture（multi-exe/ns_same_short_name）+ 本地验证（driver 编译+运行匹配）
- [x] 1.7 文档同步（source-compile.md 机制节 + common-pitfalls §1 关联注）
- [x] 1.8 本地 GREEN：**除 1 个 pre-existing 失败外全绿**（见备注）
- [ ] 1.9 归档 + PR（待 User 确认 pre-existing 失败处置）

## GREEN 状态（本地，2026-08-31）
- ✅ cargo build (release z42vm) / e2e goldens / e2e cross-zpkg / **e2e multi-exe（含本 change 回归 `ns_same_short_name` ✓）** / stdlib [Test]（全绿）
- ⚠️→✅ manifest-targets `compile-then-test`：`xtask test` 那一跑报 `kind=exe but no Main()`（kind=lib 被当 exe），**已查明为 flaky/transient stale-artifact，非代码缺陷、非本 change**：
  - **根因定证**：settle 后**手动**跑同一命令 `z42vm --mode interp z42.builder.zpkg -- test <toml>` → **PASS（2 passed, 0 failed, rc=0）**。故失败是运行瞬时态，不是稳定失败。
  - **机制**：整条 source 链（`ManifestLoader._parseProject` 读 kind="lib" → `ProjectManifest.Project.Kind` → builder `_orchestrate` 设 `ctx.Project=manifest.Project` → `Pipeline.Compile` 传 `ctx.Project.Kind` → `Z42cCompiler` 读 `req.Kind`）在 origin/main **完全正确**；`.z42/libs/z42.build.zpkg` 确含 kind 逻辑。`xtask test` 跑时 req.Kind 到达为 "" 只因当时加载的 z42.builder/z42.build 处于 stale/mid-rebuild 态（本机 8+ 并发会话争抢共享构建产物 + 专门的 `z42-compilethentest` worktree 另一会话在动）。
  - **与本 change 正交**：`git stash` 掉本 change 4 文件后干净重建 z42c+stdlib 跑 targets **同样失败**（c=0/s=0/t=1）→ 证明与本 change 无关；且失败在 project-kind 解析、非类型解析，所有走类型解析的 stage 全绿。
  - **结论**：不阻塞本 change；CI（无并发争抢）为 GREEN 权威。

## 备注（Deferred）
- **imported 跨包同短名类型**（`using` 两个包各有 `Foo`）本 change **未覆盖**——只给本地类设 ns 与
  ClassesByFqn。半径与风险更大（imported 加载 + 合并路径），且静态调用侧同类问题已由
  fix-crosspkg-static-ns-collision 单独处理。列 Deferred，触发再评估。
- **非限定同短名歧义**（`using A; using B;` 后裸写 `Foo`）当前仍 first/last-wins 静默选一；C# 语义应
  报歧义错误。属独立诊断改进，Deferred。
