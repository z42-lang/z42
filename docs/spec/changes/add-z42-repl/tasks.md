# Tasks: z42 原生交互式 REPL

> 状态：🟢 全阶段实现完成（求值引擎 + 宿主 + 路由，端到端可运行）| 分三 PR：阶段1(#19 已合) / infer-var-field-types(#24 已合) / 本 PR（引擎+宿主+路由）| 创建：2026-07-23
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
- [ ] 2.1 `z42.scripting.z42.toml`（deps 按 D2 最终裁决：A=依赖 z42c.pipeline/z42c.syntax）+ README
- [ ] 2.2 `Repl.z42`（`Std.Repl.ReadLine/ReadBlock` native 绑定）
- [ ] 2.3 `ScriptState.z42` / `EvalResult.z42`
- [ ] 2.4 `InputClassifier.z42`（6 类分类 + 括号平衡）
- [ ] 2.5 `Transcript.z42`（Growing Transcript + `$ReplVars` 昇格 + using/type 追加）
- [ ] 2.6 `Script.z42`（`Create` / `Eval`：分类→建源→PackageCompile→内存加载→Invoke→EvalResult；错误恢复）
- [ ] 2.7 `ResultFormatter.z42`（原始/对象反射/数组/null/RuntimeError）
- [ ] 2.8 `tests/eval_expr` `eval_var_persist` `eval_error_recovery` [Test]

## 阶段 3: z42.interactive REPL 宿主 + launcher（toolchain）
- [ ] 3.1 `interactive_main.z42` 改真入口：交互循环 + `-c "expr"` 单次求值
- [ ] 3.2 `ReplSession.z42`（读→分类→Eval→打印）
- [ ] 3.3 `LineEditor.z42`（封装 Std.Repl + 提示符/续行）
- [ ] 3.4 `MetaCommands.z42`（MVP 集：.help .exit .quit .reset .clear .history .save .vars .types .usings .using .mode .version）
- [ ] 3.5 `z42.interactive.z42.toml` 加依赖 z42.scripting + source 列表
- [ ] 3.6 `launcher_cli.z42` 注册 + 路由 `repl` → z42i（Z42_LIBS 三段）

## 阶段 4: 验证
- [ ] 4.1 `cargo build --release`（z42vm）无错
- [ ] 4.2 `xtask test stdlib`（含 z42.scripting）全绿
- [ ] 4.3 `xtask test compiler` 自举 gen1==gen2 逐字节不动（本 change 不碰 z42c 源）
- [ ] 4.4 `z42 repl` 手动 smoke：`1+2`、`var x=5;x*2`、编译错误保留、`.vars`/`.help`/`.exit`
- [ ] 4.5 `z42 repl -c "1+2"` → 3 后退出
- [ ] 4.6 spec scenarios 逐条覆盖确认

## 阶段 5: 文档同步（按阶段 9 触发矩阵）
- [ ] 5.1 `docs/design/toolchain/repl.md` 校正 stale（包名/路径/zpkg 数/内存加载/静态依赖决策）
- [ ] 5.2 `src/toolchain/interactive/README.md` 去 scaffold、改六段制
- [ ] 5.3 `src/libraries/z42.scripting/README.md` 六段制
- [ ] 5.4 `docs/roadmap.md` 0.4.0 REPL 状态
- [ ] 5.5 `docs/spec/changes/ACTIVE.md` 实现期登记三子系统 / 归档释放

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
