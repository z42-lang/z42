# Tasks: 泛型方法端到端（方法级 type_args，M1）

> 状态：🟡 进行中 | 创建：2026-08-21
> 类型：lang/ir/vm（完整流程）| 分支：add-generic-methods | worktree：../z42-genmethods

## 进度概览
- [x] 阶段 1: 前端（parser + AST + 绑定）
- [x] 阶段 2: IR + 格式（指令 + 编解码 + 版本 bump zbc1.36/zpkg0.41）
- [x] 阶段 3: 运行期（interp ✅ + jit：含泛型的函数走 interp-fallback，jit_unsupported_reason 拦截）
- [x] 阶段 4: 测试 + 文档（6 golden 全过 + book/design/roadmap/README）
- [~] 阶段 5: 验证 —— 本地 GREEN（temp-降版号绕自举墙）：e2e interp 256/0 + jit 252/0、compiler 24 单元 + 5/5 自举字节不动点、cargo --lib 除 host_tests(需 0.41 build 产物,CI 两代自举重生)外全过、feature e2e 全过；**格式-bump 全量 GREEN 以 CI 两代自举为准**（bootstrap-seed.md）

> **格式-bump 收尾说明**：本地 cargo vm(0.41) 无法读 0.40 seed 工具链、且 `xtask build` 硬编 cargo vm → 本地全量重建被两代自举墙挡（macOS 墙）。committed fixture（6 zbc + 3 zpkg + indexed 伴随+内容哈希）因**非泛型逐字节不变**已 hand-patch 版本头字节到 0.41（无 BLID 校验，安全）；sym-only-sidecar 冻结在 35（无测试读取）。build-产物依赖的 host_tests / zbc_compat golden 本地陈旧 → CI 两代自举重建后转绿。

## 阶段 1: 前端
- [ ] 1.1 `Ast.z42`：`CallExpr` 加 `TypeExpr[] TypeArgs` + `TypeArgCount`；`Dump()` 带 `<...>`
- [ ] 1.2 `ExprParser.z42`：调用点 `名<类型列表>(` 解析 + `<` 有限前瞻歧义消解（回退零副作用）
- [ ] 1.3 `Bound.z42`：`BoundCall` 加解析后方法 type_args（具体 FQ 名数组）
- [ ] 1.4 `TypeChecker.z42`：绑定方法 type_args → 泛型方法 decl；arity 校验（诊断码）；`where` 约束校验（复用 `ConstraintChecker`）
- [ ] 1.5 `TypeChecker.z42` / `ExprEmitter.z42`：方法体内 `typeof(T)`/`new T()`/`default(T)` 判别方法级 vs 类级形参（方法级优先），解析 `param_index`

## 阶段 2: IR + 格式（按 pipeline）
- [ ] 2.1 `IrInstr.z42`：`Call`/`VCall` 加 `MethodTypeArgs: string[]`；新增 `MethodTypeArgInsn{dst, paramIndex}` + `MethodDefaultInsn{dst, paramIndex}`
- [ ] 2.2 `ExprEmitter.z42`：call 发 `MethodTypeArgs`；`typeof(T)`→`MethodTypeArgInsn`；`new T()`→`MethodTypeArgInsn`+activator；`default(T)`→`MethodDefaultInsn`
- [ ] 2.3 `ZbcWriter.z42` / `ZbcReader.z42`：编解码 Call 新字段 + 两条新指令
- [ ] 2.4 `ZbcVersion.z42` + `ZpkgWriter.z42`：`Minor` bump（version-bumping.md 全 checklist 逐项）
- [ ] 2.5 golden zbc fixture regen + strict-pin 断言更新

## 阶段 3: 运行期
- [ ] 3.1 `interp/mod.rs`：`Frame` 加 `method_type_args: Box<[String]>`（默认空）
- [ ] 3.2 `metadata/zbc_reader.rs`：解码 Call 新字段 + 新指令 → `Instruction` variant
- [ ] 3.3 bytecode `Instruction` enum：加 `MethodTypeArg` / `MethodDefault` variant（`bytecode.rs`）
- [ ] 3.4 `interp/exec_call.rs`：建帧填 `frame.method_type_args`（含 static/instance 调用路径）
- [ ] 3.5 `interp/exec_address.rs`（或新 `exec_typearg.rs`）：`MethodTypeArgInsn`→`make_type_from_name(frame 槽)`；`MethodDefaultInsn`→`default_value_for(frame 槽)`；OOB/空 graceful
- [ ] 3.6 `interp/exec_instr.rs`：新指令 match 分派（无 `_` 兜底）
- [ ] 3.7 jit `frame.rs` + `translate.rs`：镜像 frame 槽 + Call 传递 + 新指令 helper（interp 全绿后）
- [ ] 3.8 `cargo test --lib`（runtime 单测）

## 阶段 4: 测试 + 文档
- [ ] 4.1 `src/tests/e2e/generic-methods/`：typeof(T) 真句柄/基元/一致性、new T()、default(T) ref→null/val→零值、多参 `<K,V>`
- [ ] 4.2 `src/tests/e2e/generic-methods/`：arity 错诊断、`where` 约束违背诊断
- [ ] 4.3 `src/tests/`：`<` 歧义 fixture（`a<b>c` 比较不误判）
- [ ] 4.4 `docs/book/src/lang/generics.md`：方法级泛型章节（frame 携带 + 与类级对称 + mermaid）
- [ ] 4.5 `src/libraries/z42.ir/README.md` + `src/runtime/src/interp/README.md`：功能索引 / Frame 说明
- [ ] 4.6 `docs/roadmap.md`：标注 M1 进度 + Deferred Backlog Index 加 3 条（reflective-invoke / type-inference / classlevel-typeof）

## 阶段 5: 验证
- [ ] 5.1 `cargo build --release`（z42vm）无错
- [ ] 5.2 `xtask test e2e` + `--dir cross-zpkg` 全绿
- [ ] 5.3 `xtask test stdlib` + `xtask test compiler`（z42c 自举，不使用泛型方法 → gen 稳）
- [ ] 5.4 `xtask test bootstrap`：上一 nightly z42c 仍能编当前源（support 先行守住，无越界）
- [ ] 5.5 `xtask test vscode-syntax`
- [ ] 5.6 spec scenarios 逐条覆盖确认
- [ ] 5.7 push 后盯 CI：格式 bump 冷路径两代自举 / test-vm-jit 绿（本地不可验部分以 CI 为准）

## 备注
- Fork A（frame 槽）为本质方案；Fork B（codegen 隐藏参数）已否决（临时、反射 invoke 用不上）。
- support 先行：本变更零 z42c/stdlib 泛型方法使用；`Deserialize<T>`（M2）晚一 nightly。
- 格式 bump 不踩 format-bump 同周期删 C# 种子的残留窗口（bootstrap-seed.md）。
