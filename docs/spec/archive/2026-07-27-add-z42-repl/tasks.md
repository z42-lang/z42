# Tasks: z42 原生交互式 REPL

> 状态：🟢 已完成 | 完成：2026-07-27（归档扫描收口）| 分三 PR：阶段1(#19 已合) / infer-var-field-types(#24 已合) / 本 PR（引擎+宿主+路由）| 创建：2026-07-23
>
> **归档收口说明（2026-07-27）**：实际发货实现把阶段 2/3 的**多文件计划结构**（`InputClassifier`/`Transcript`/`ResultFormatter`/`ReplSession`/`LineEditor`/`MetaCommands`）**收敛**进更少文件（`Script.z42`+`Rewriter.z42`+`interactive_main.z42`），下方原细分 `[ ]` 按此重述为「已发货（合并形态）」或「显式延后」。**显式延后给 `add-repl-decls-multiline`（Change B）**：多行输入（`ReadBlock` 接线）+ 函数/类型**声明累积**（完整分类）。**延后为后续 follow-up**：更全元指令集（.reset/.clear/.history/.save/.types/.mode/.version）+ 富结果格式化（对象反射/数组）。REPL 的 MVP（表达式 / var carry-forward / using / 语句 / 错误恢复 / `.help .exit .quit .vars .usings` / `-c`）端到端已发货并验证。
> **隔离分支并行**（User 授权）：阶段1 在 `claude/z42-repl`（#19 已合）；var 字段修复在 `claude/infer-var-field-types`（#24 已合）；引擎+宿主+路由在 `claude/z42-repl-scripting`。GREEN 以 CI 为权威。
> 实现顺序：runtime（阶段 1 ✅ #19）→ compiler 前置 var 字段修复（#24）→ stdlib z42.scripting（阶段 2 ✅）→ toolchain z42.interactive + launcher（阶段 3 ✅）

## 进度概览
- [x] 阶段 1: VM builtin（#19 合并）— cargo build + 781 lib 单测全绿
- [x] 前置: infer-var-field-types（#24 合并）— carry-forward 跨轮 var 字段保型
- [x] 阶段 2: z42.scripting 求值引擎 — 表达式/var carry-forward/using-stdlib/语句/错误恢复（warm-z42c 回路端到端）
- [x] 阶段 3: z42.interactive REPL 宿主 + launcher `z42 repl` 路由 — 交互 + `-c` 模式实测
- [x] 阶段 4: 端到端验证（warm-z42c 回路；full GREEN 以 CI 为权威）
- [x] 阶段 5: 文档同步（repl.md 实现落地 + stale 校正）

## 阶段 1: VM builtin（runtime）✅
- [x] 1.1 `src/runtime/Cargo.toml` 加 rustyline 依赖（target-gated non-wasm）
- [x] 1.2 `src/runtime/src/corelib/repl.rs` 新建：`__repl_readline` + `__repl_readblock`（rustyline 历史/行编辑/Ctrl-D；readblock 括号平衡多行，忽略字符串/字符/注释内括号；wasm 回退裸 stdin）
- [x] 1.3 `src/runtime/src/vm_context.rs` 加 `load_module_bytes_into_vm`（内存字节 → live VM）
- [x] 1.4 `src/runtime/src/corelib/reflection.rs` 加 `__load_bytecode_in_memory(byte[])` builtin（复用 lazy loader；抽 `register_loaded_artifact` 公共体 + `load_module_from_bytes`）
- [x] 1.5 `src/runtime/src/corelib/mod.rs` 注册 3 个 builtin
- [x] 1.6 `src/runtime/src/corelib/repl_tests.rs` 9 个 bracket_depth 单测（内存加载往返留待阶段 4 端到端）

## 阶段 2: z42.scripting 库（stdlib）
- [x] 2.1 `z42.scripting.z42.toml`（deps=A：z42c.syntax/semantics/pipeline + z42.ir）+ README — 已发货（物理落 `src/toolchain/scripting/`，见 README「位置 D2」）
- [x] 2.2 `Repl.z42`（`Std.Repl.ReadLine/ReadBlock` native 绑定）— 已发货
- [x] 2.3 `ScriptState.z42` / `EvalResult.z42` — 已发货
- [x] 2.4 输入分类 + 括号平衡 —（合并形态）括号平衡在 Rust `bracket_depth`（阶段 1）；z42 侧分类合并进 `Script._classify`，MVP 覆盖 var 声明 / 表达式 / 语句 / using。**函数/类型声明完整分类 → 延后 `add-repl-decls-multiline`（Change B）**
- [x] 2.5 会话状态模型 —（合并形态）`Transcript`/`$ReplVars` 昇格合并进 `Script.z42`（每轮 `Repl.R{N}` + `Vars{N}` 静态字段）+ `Rewriter.z42`（裸引用改写）；perf 优化后（#46）为 CachedScan + 内存增量 carry-forward
- [x] 2.6 `Script.z42`（`Create` / `Eval`：分类→建源→PackageCompile→内存加载→Invoke→EvalResult；错误恢复）— 已发货
- [x] 2.7 结果格式化 —（合并形态）MVP 为 `interactive_main._fmt`（`"" + v` ToString，null 抑制）。**富格式化（对象反射/数组）→ follow-up**
- [x] 2.8 求值验证 —（合并形态）Rust `pkgcompile` 单测（`test_cached_scan_reused`/`test_extend_with_package_adds_namespace`，perf #46）+ warm-z42c 手动回归（表达式/carry-forward/重赋值/重声明/void/错误恢复）

## 阶段 3: z42.interactive REPL 宿主 + launcher（toolchain）
- [x] 3.1 `interactive_main.z42` 改真入口：交互循环 + `-c "expr"` 单次求值 — 已发货
- [x] 3.2 读→分类→Eval→打印 —（合并形态）内联进 `interactive_main.Main` 循环
- [x] 3.3 行编辑封装 —（合并形态）`interactive_main` 直调 `Std.Repl.ReadLine`。**多行续行（`ReadBlock` 接线）→ 延后 `add-repl-decls-multiline`（Change B）**
- [x] 3.4 元指令 —（合并形态）内联 MVP 集 `.help .exit .quit .vars .usings`。**更全集（.reset/.clear/.history/.save/.types/.mode/.version）→ follow-up**
- [x] 3.5 `z42.interactive.z42.toml` 加依赖 z42.scripting + source 列表 — 已发货
- [x] 3.6 `launcher_cli.z42` 注册 + 路由 `repl` → z42i（Z42_LIBS 三段）— 已发货

## 阶段 4: 验证
- [x] 4.1 `cargo build --release`（z42vm）无错 — 阶段 1 #19 全绿
- [x] 4.2 `xtask test stdlib`（含 z42.scripting）— GREEN 以 CI 为权威
- [x] 4.3 `xtask test compiler` 自举不动点（本 change 不碰 z42c 源）— 不回归
- [x] 4.4 `z42 repl` 手动 smoke：`1+2`、`var x=5;x*2`、编译错误保留、`.vars`/`.help`/`.exit` — 实测通过
- [x] 4.5 `z42 repl -c "1+2"` → 3 后退出 — 实测通过
- [x] 4.6 spec scenarios 逐条覆盖确认 — MVP 场景覆盖；多行/声明累积场景转 Change B

## 阶段 5: 文档同步（按阶段 9 触发矩阵）
- [x] 5.1 `docs/design/toolchain/repl.md` 校正 stale — 已更新（实现落地 + 状态模型）
- [x] 5.2 `src/toolchain/interactive/README.md` — 已更新
- [x] 5.3 `src/toolchain/scripting/README.md` 六段制 — 已更新
- [x] 5.4 `docs/roadmap.md` 0.4.0 REPL 状态 — 已更新
- [x] 5.5 `docs/spec/changes/ACTIVE.md` 归档释放 — 本次归档收口（toolchain 锁释放）

## 备注

### 里程碑收口（2026-07-23，User 裁决）
阶段 1（VM 地基）+ spec + D7 设计修正作为**已验证里程碑**收口；阶段 2/3 暂停，留待**有 warm z42c、能跑 z42i 的环境**边写边验（不盲写不可验证的 z42）。原因：z42.scripting 是「依赖编译器包的 stdlib 库」（D2），按 bootstrap-seed 轴④冷启动路径本地必验；且下列阶段 2 语义模型决策必须靠跑起来的 REPL 验证对错。

### 阶段 2 语义模型（部分已用 warm-z42c 验证回路实证，2026-07-23）
1. ~~**`var x = 5` 字段类型来源**~~ ✅ **已实证：z42 支持 `static var x = 5;` 字段类型推断**（编译通过）——`var`/显式类型都可直接提升为静态字段，**无需推断探针**，mutation 持久。原顾虑消除。
2. **静态字段引用需限定** ✅ **已实证：`E0401: undefined: x`**——静态方法内**不能**不加限定引用同类静态字段；`ReplVars.x` 限定形式编译通过。⇒ **用户输入里的会话变量引用必须改写成 `$ReplVars.<name>`**（token 级改写，可用 z42c.syntax 的 Lexer 识别标识符 token）。这是 Transcript 的核心工作量，spec/design 原未点明。
3. **副作用重放语义**：静态字段模型下仅 var 初始化器随 static-init 重放；纯语句 `$Stmt_N` 一次性、**不累积进 transcript**（否则 `Console.WriteLine` 每轮重打印）。定为**不累积**。
4. **D7 命名空间/包名唯一**（已定）：每轮 `Repl.R{N}` / `repl_r{N}`，绕开加载器 first-wins 幂等。

### 验证回路（已打通，2026-07-23）
worktree 的 z42vm（含 REPL builtin）+ 主工作树 `artifacts/build/` 的 warm z42c(5 包)+stdlib(25 包，含 z42.ir/project) 组装成单一 Z42_LIBS 目录，直接 `z42vm z42c.driver.zpkg --mode interp -- build <toml> --release --output-dir <out>` 即可编译验证。脚本存 scratchpad `zc.sh`/`zrun.sh`。**阶段 2 恢复时此回路可编译并端到端验证 z42.scripting**。

### 已确认可复用的 API（阶段 2 恢复时直接用）
- parse：`Z42.Semantics.IrDump.ParseAll(string[] srcs, string[] files, int count) -> CompilationUnit[]`
- hash：`Z42.IR.ZpkgBuilder.Sha256Hex(text)`
- compile：`Z42.Pipeline.PackageCompile.Compile(CompileInputs) -> CompileArtifacts`
- bytes：`Z42.Project.ZpkgWriter.WritePacked(art.Z, false).ToBytes() -> byte[]`
- 加载：`[Native("__load_bytecode_in_memory")] extern bool(byte[])`（阶段 1 已落）
- 调用：`[Native("__invoke_static")] extern object Invoke(string fqn)`（复用 z42.test/ModuleLoader 范式，返回函数结果）
- 行编辑：`[Native("__repl_readline")]` / `[Native("__repl_readblock")]`（阶段 1 已落）
- 依赖（z42.scripting.toml）：`z42c.syntax` + `z42c.semantics` + `z42c.pipeline` + `z42.ir`（+ core prelude）
- 集合/字符串：`Std.Collections.List<T>`（core prelude：`Count`/`this[i]`/`Add`/`ToArray`）、`String.Join/Substring/StartsWith/Split/...`

### 其他
- 静态依赖 PackageCompile → 不依赖 dynamic-component-registration 的接口 cast 修复。
- 隔离开发在 worktree `../z42-repl-wt` 分支 `claude/z42-repl`；主工作树的 lazy-per-function-jit jit WIP 未触碰。ACTIVE.md 的 REPL 独立分支登记随本分支走，合并回 main 时生效。
