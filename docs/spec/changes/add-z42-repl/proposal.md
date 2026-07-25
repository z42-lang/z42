# Proposal: z42 原生交互式 REPL（add-z42-repl）

> 状态：🔴 DRAFT（待 6.5 审批）| 创建：2026-07-23 | 类型：`vm`（新 native builtin）→ 完整流程
> 子系统：`runtime`（rustyline 行编辑 + 内存字节加载 builtin）‖ `stdlib`（新库 `z42.scripting`）‖ `toolchain`（填充 `z42.interactive` 脚手架 + launcher `repl` 路由）
> 设计 SoT：[docs/design/toolchain/repl.md](../../../design/toolchain/repl.md)（本 change 落地前需按实际 stale 处校正）
> 前置（均已满足）：非泛型 `Method.Invoke`（0.3.12 ✅）、Boxing（0.3.11 ✅）、进程内编译核心 `PackageCompile`（extract-compile-pipeline-api ✅ 2026-07-17）、`z42.interactive` 脚手架（2026-07-01 ✅）

## Why

z42 缺少交互式求值环境。REPL 是 0.3.x/0.4.0 招牌产品能力：输入 z42 代码→即时编译求值→打印结果→状态跨行持久。地基（自举编译器 zpkg + 进程内编译 API + 反射调用）已全部就绪，且 REPL 宿主 `z42.interactive`（z42i）脚手架已存在（当前仅打印 "planned"），本 change 把它填成真 REPL。

**关键前提复核**：REPL 的 `z42.scripting` 采用**静态依赖 `PackageCompile`**（决策 D1），不走「反射注入 ICompiler」路径，因此**不撞** [dynamic-component-registration](../dynamic-component-registration/) 正在修的跨包接口 cast bug——那不是本 change 的硬前置。

## What Changes

1. **VM（`runtime`）**
   - 引入 `rustyline`，新增 native builtin `__repl_readline(prompt)` / `__repl_readblock(prompt, cont)`（历史、行编辑 Ctrl-A/E/K/U、多行括号平衡检测、Ctrl-D 退出）。
   - 新增内存字节加载 builtin `__load_bytecode_in_memory(mods)`：把 `PackageCompile` 产出的内存模块字节加载进 live VM 并返回可调用句柄（区别于 test 专用、吃磁盘路径的 `__load_module`）。
2. **stdlib（`stdlib`）——新库 `z42.scripting`**
   - `Script.Create()` / `Script.Eval(state, input)`（scripting-charter Form B 状态承载）。
   - `InputClassifier`（表达式/变量声明/函数声明/类声明/using/纯语句 分类 + 括号平衡多行判定）。
   - Growing Transcript（`var x` → `$ReplVars` 静态字段提升；整体重编）+ 错误恢复（编译失败不追加、`NextState = prevState`）。
   - `ResultFormatter`（原始类型直印；对象走 `ToString()` 或反射 `TypeName { field: val }`；数组、null）。
   - `Std.Repl` native 绑定声明（`ReadLine`/`ReadBlock` 的 `[Native]` extern，供 z42i 与用户代码共用）。
3. **toolchain（`toolchain`）**
   - 填充 `src/toolchain/interactive/core/`：真 REPL 循环（`ReplSession` + `LineEditor` + `MetaCommands`）+ `-c "expr"` 单次求值。
   - MVP `.` 元指令集：`.help .exit .quit .reset .clear .history .save .vars .types .usings .using .mode .version`。
   - launcher `launcher_cli.z42` 注册并路由 `z42 repl` → z42i（Z42_LIBS = libs/ + programs/z42c/ + programs/interactive/）。
4. **docs**：校正 repl.md 的 stale 处（`z42.repl`→`z42.interactive`/z42i、`programs/repl`→`programs/interactive`、编译器 "7 zpkg"→5、内存加载 builtin、静态依赖 PackageCompile 决策），刷新 roadmap 0.4.0 REPL 状态。

## What This Does NOT Do（明确划走）

- **反射 `.type`/`.members` 指令**：随反射就绪并入，非 MVP（设计已标 [refl]）。
- **`.time`/`.counters`/`.trace` 诊断指令**：依赖 diagnostics.md，非 MVP（[diag]）。
- **`.load <file>` 指令**：defer（`repl-future-load-directive`）。
- **Tab 补全**：依赖 LSP，defer（`repl-future-tab-completion`）。
- **增量编译**：MVP 用 Growing Transcript（O(n) 重编，session 数百行内可接受）；增量方案 defer（`repl-future-incremental-compilation`）。
- **反射注入编译器 / ICompiler 组件化**：本 change 走静态依赖，组件化留 dynamic-component-registration。
- **mobile/WASM REPL**：host-only（scripting-charter 路径 2b），mobile defer。

## Scope（允许改动的文件）

### runtime（`runtime` 锁，现被 `lazy-per-function-jit` 占 → 实现排队）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/Cargo.toml` | MODIFY | 加 `rustyline` 依赖 |
| `src/runtime/src/corelib/repl.rs` | NEW | `__repl_readline` / `__repl_readblock`（rustyline） |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册新 builtin（`__repl_readline`/`__repl_readblock`/`__load_bytecode_in_memory`） |
| `src/runtime/src/vm_context.rs` | MODIFY | `load_module_from_bytes` helper（内存字节 → live VM） |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `__load_bytecode_in_memory` builtin（复用 lazy loader，返回句柄） |
| `src/runtime/src/corelib/repl_tests.rs` | NEW | 括号平衡 / 内存加载 Rust 单测 |

### stdlib（`stdlib` 锁，现被 converge-z42c-onto-z42-project 占 → 实现排队）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/toolchain/scripting/z42.scripting.z42.toml` | NEW | 库清单（依赖 z42.core + z42c 编译器包，见 design D2）|
| `src/toolchain/scripting/README.md` | NEW | 六段制 |
| `src/toolchain/scripting/src/Script.z42` | NEW | `Script.Create` / `Script.Eval` |
| `src/toolchain/scripting/src/ScriptState.z42` | NEW | 会话状态（sessionSource + evalCounter + vars/types/usings 台账）|
| `src/toolchain/scripting/src/EvalResult.z42` | NEW | `Success`/`Value`/`ErrorMessage`/`NextState` |
| `src/toolchain/scripting/src/InputClassifier.z42` | NEW | 分类 + 括号平衡 |
| `src/toolchain/scripting/src/Transcript.z42` | NEW | Growing Transcript + `$ReplVars` 昇格 |
| `src/toolchain/scripting/src/ResultFormatter.z42` | NEW | 结果打印 |
| `src/toolchain/scripting/src/Repl.z42` | NEW | `Std.Repl.ReadLine/ReadBlock` native 绑定 |
| `src/toolchain/scripting/tests/eval_expr/` | NEW | eval 表达式端到端 |
| `src/toolchain/scripting/tests/eval_var_persist/` | NEW | 变量跨行持久 |
| `src/toolchain/scripting/tests/eval_error_recovery/` | NEW | 编译失败保留会话 |

### toolchain（`toolchain` 锁，空闲）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 真 REPL 入口 + `-c` 单次求值 |
| `src/toolchain/interactive/core/ReplSession.z42` | NEW | 会话循环（读→分类→Eval→打印）|
| `src/toolchain/interactive/core/LineEditor.z42` | NEW | 封装 `Std.Repl` + 多行提示 |
| `src/toolchain/interactive/core/MetaCommands.z42` | NEW | `.` 元指令派发 |
| `src/toolchain/interactive/core/z42.interactive.z42.toml` | MODIFY | 加依赖 z42.scripting；补 source 列表 |
| `src/toolchain/interactive/README.md` | MODIFY | 去 scaffold 说明，改六段制 |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | 注册 + 路由 `repl` → z42i |

### docs
| 文件 | 变更 | 说明 |
|------|------|------|
| `docs/design/toolchain/repl.md` | MODIFY | 校正 stale（包名/路径/zpkg 数/内存加载/静态依赖决策）|
| `docs/roadmap.md` | MODIFY | 0.4.0 REPL 状态 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 实现阶段登记三子系统占用 |

**只读引用**：`src/compiler/z42c.pipeline/src/PackageCompile.z42`（编译核心 API）、`src/runtime/src/corelib/reflection.rs`（`__load_module` 参照）、`src/toolchain/builder/core/`（z42b 镜像宿主形态）、`scripts/packages.toml`（interactive 已登记，无需改）。

## Out of Scope
- 反射/诊断指令、`.load`、Tab 补全、增量编译、组件化注入、mobile REPL（见上）。
- 修改 `scripts/packages.toml`（interactive 组件已登记）。

## Open Questions（design.md 展开）
- [x] D2：`z42.scripting` 依赖编译器包（z42c.*），构建层级/顺序如何安排（stdlib lib 依赖 compiler pkg 是非常规分层）。
  **落定**：物理移出 `src/libraries/`（其 `members=["*"]` 会把子目录当基 stdlib 成员、用「仅 stdlib」的 `build --workspace` 编，撞 `E0401 undefined Lexer/TokenKind`），改置 `src/toolchain/scripting/`，由 `xtask_toolchain.z42:_buildScriptingLib` 以「stdlib + z42c 合并 Z42_LIBS」专步构建。zpkg 名仍 `z42.scripting`（import 按名解析，与源码位置无关）。
- [ ] D3：内存加载走新 builtin（推荐）vs 临时 zpkg 落盘复用 `__load_module`。
- [ ] MVP 结果打印对未重写 `ToString()` 的对象，反射展示深度（1 层字段 vs 递归）。

## GREEN 判据
- `cargo build --release`（z42vm）无错。
- `xtask test stdlib`（含 z42.scripting 新 `[Test]`）全绿。
- `xtask test compiler` 自举 gen1==gen2 逐字节不动（本 change 不碰 z42c 源）。
- `z42 repl` 手动 smoke：`1+2`→3、`var x=5; x*2`→10、编译错误保留会话、`.vars`/`.help`/`.exit`。
- `z42 repl -c "1+2"` 输出 3 后退出。
