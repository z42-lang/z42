# Design: z42 原生交互式 REPL

## Architecture

```
z42 repl [-c "expr"]
  │  launcher_cli.z42 路由 repl → z42i
  │  Z42_LIBS = libs/ + programs/z42c/ + programs/interactive/
  └── z42vm programs/interactive/z42.interactive.zpkg
        ├── ReplSession        读→分类→Eval→打印 循环
        ├── LineEditor         Std.Repl.ReadLine/ReadBlock（→ rustyline builtin）
        ├── MetaCommands       . 前缀元指令派发
        └── z42.scripting（libs/）
              Script.Eval(state, input)
                ├── InputClassifier   分类 + 括号平衡
                ├── Transcript        Growing Transcript + $ReplVars 昇格
                ├── PackageCompile.Compile   ← 静态依赖（programs/z42c/）
                ├── __load_bytecode_in_memory(mods)   ← 新 VM builtin
                ├── Method.Invoke($Eval_N)            ← 非泛型（0.3.12）
                └── ResultFormatter   结果打印
```

## Decisions

### D1: z42.scripting 调编译器 = 静态依赖 PackageCompile（User 已定）
**问题**：scripting 编译 session 源有两条路——(a) 静态依赖 `z42c.pipeline.PackageCompile`；(b) 反射注入 `ICompiler`（z42b 范式）。
**决定**：选 (a)。REPL 是 pre-1.0 host-only（scripting-charter 路径 2b），静态依赖最简；且**绕开** dynamic-component-registration 正在修的跨包接口 cast bug，使 REPL 不被其阻塞。组件化注入留给 z42b/未来。

### D2: z42.scripting 依赖编译器包的构建层级（Open Question — 需 User 裁决）
**问题**：`z42.scripting` 要 `using` `PackageCompile`（`z42c.pipeline`）+ `z42c.syntax`（parse session 源成 `CompilationUnit[]`），即一个 **stdlib 库依赖 compiler 包**——非常规分层（stdlib 通常不依赖 z42c）。
**选项**：
- **A（推荐）**：`z42.scripting` 作为「compiler-consuming 库」，声明 deps 到 z42c.pipeline/z42c.syntax，构建顺序排在 z42c 之后（build stdlib 的 scripting 步单独在 z42c 就绪后跑）。运行期 Z42_LIBS 已含 programs/z42c/，dep 解析可达。符合 scripting-charter 的 L3 分层定位。
- **B**：不作 stdlib 库，把 scripting 逻辑并入 `z42.interactive` 程序包（与 z42c 同为 toolchain 层，天然可依赖 z42c）。代价：用户代码无法 `import z42.scripting`（charter 期望它是可复用库）。
**倾向**：A（保 charter 定位），但构建顺序/`build stdlib` 是否支持「lib 依赖 z42c」需实测确认——**留 6.5 让 User 拍板 A/B**。

### D3: 内存加载 = 新 builtin `__load_bytecode_in_memory`（Open Question — 需 User 裁决）
**问题**：`PackageCompile` 产出内存 `Mods`（`ZbcFileZ[]`），VM 要执行 `$Eval_N` 必须先加载。现有 `__load_module` **吃磁盘路径且 test 专用**（返回 TestEntry、跑 static-init）。
**选项**：
- **A（推荐）**：新增 `__load_bytecode_in_memory(mods)`，复用 lazy loader 内核但输入内存字节、返回通用可调用句柄。干净，匹配 repl.md 设计的 `LoadBytecodeInMemory`，零磁盘 I/O。
- **B**：scripting 侧把 `ToPackedBytes` 写临时 zpkg → `__load_module`。省一个 builtin，但每 eval 落盘 + 复用 test 语义（不契合）。
**倾向**：A。

### D4: 状态模型 = Growing Transcript（MVP，repl.md 已定）
每轮输入追加进累积 session 源整体重编；`var` 昇格为 `$ReplVars` 静态字段。选它因语义正确、实现简单；O(n) 重编在数百行 session 可接受。增量方案 defer（`repl-future-incremental-compilation`）。

### D7: 每轮唯一命名空间 + 唯一包名（阶段 2 实施期发现，2026-07-23）
**问题（实施期根因）**：阶段 1 的 `__load_bytecode_in_memory` 复用 lazy-loader 的
`register_loaded_artifact`，它**按模块名 first-wins 幂等**（`if loaded_zpkgs.contains(mod_key)
return`）+ 函数表 first-wins。而 Growing Transcript **每轮重编同一个会话包再加载**：若每轮包名
恒为 `$repl`、命名空间恒为 `Repl`，则第 2 轮起 `mod_key` 重复 → 整轮被跳过，新 `$Eval_N` /
更新后的 `$ReplVars` **不注册** → REPL 从第二条输入起失效。
**决定**：每轮用**唯一命名空间 `Repl.R{N}` + 唯一包名 `repl_r{N}`**（N=evalCounter）。每轮源 =
`namespace Repl.R{N};` + 累积 usings + `static class $ReplVars {...}`（本轮全量快照）+ 累积
fn/class 声明 + `static <ret> $Eval_{N}()`。每轮自包含：函数 FQN（`Repl.R{N}.$Eval_{N}` /
`Repl.R{N}.$ReplVars.*`）跨轮天然不撞 → 加载即全新注册，static-init 跑本轮 `$ReplVars`
（重新求值全部 var 字面量/表达式）。
**权衡**：旧轮命名空间在 VM 里滞留（内存随轮数增长）——与 D4「O(n) 重编」同量级、可接受；
真正回收留 `repl-future-incremental-compilation`（load-context supersede）。**不改 spec 的
Growing Transcript 模型**，只是其落地的命名机制。副作用重放（`var x = sideEffect()` 每轮重跑）
是 Growing Transcript 固有语义，MVP 接受。

### D5: 行编辑器 = rustyline（User 已定）
`__repl_readline`/`__repl_readblock` 封装 rustyline：历史、Ctrl-A/E/K/U、Ctrl-D。`__repl_readblock` 在 z42 侧或 Rust 侧做括号平衡续行——**倾向 Rust 侧**（rustyline 的 `Validator` 天然支持多行未闭合续行），z42 侧 `InputClassifier` 仍独立做一次平衡判定用于分类。

### D6: REPL 宿主 = 复用 z42.interactive 脚手架（非新建）
z42i（`src/toolchain/interactive/`）2026-07-01 已 scaffold（Main 打印 "planned"），packages.toml 已登记 [component.interactive]、SDK 已 ship 占位二进制。本 change 填充它，**不新建 programs/repl/**。repl.md 里 `z42.repl`/`programs/repl` 说法 stale，本 change 校正为 `z42.interactive`/`programs/interactive`/z42i。

## Implementation Notes
- **$Eval_N 调用**：session 编成 exe 包，`$Eval_N()` 为顶层静态方法，经 `Method.Invoke`（非泛型）反射调用取返回 `object`；`ResultFormatter` 按运行期类型分派打印。
- **Boxing**：`$Eval_N` 返回 `object`，原始类型返回值经 0.3.11 boxing 装箱，`ResultFormatter` 拆箱/反射展示。
- **错误恢复**：`Script.Eval` 编译失败时 `EvalResult.Success=false` + `NextState=state`（原状态），会话不追加。
- **Z42_LIBS 顺序**：libs/ + programs/z42c/ + programs/interactive/（含编译器 5 zpkg + scripting + repl 自身）。
- **repl.md 校正清单**：`z42.repl`→`z42.interactive`；`programs/repl`→`programs/interactive`；「编译器 7 个 zpkg」→5；补内存加载 builtin + 静态依赖 PackageCompile 决策。

## Testing Strategy
- **单元测试（Rust）**：`repl_tests.rs` — 括号平衡边界、`__load_bytecode_in_memory` 加载+调用往返。
- **stdlib [Test]（z42.scripting）**：`Script.Eval` 表达式求值、变量持久、错误恢复、分类器边界（`tests/eval_expr` / `eval_var_persist` / `eval_error_recovery`）。
- **手动 smoke**：`z42 repl` 交互 + `z42 repl -c`。
- **GREEN gate**：`cargo build` + `xtask test stdlib` + `xtask test compiler`（自举不动点）+ 上述 smoke。

## Deferred / Future Work
沿用 repl.md 既有 Deferred 段（`repl-future-tab-completion` / `-incremental-compilation` / `-load-directive` / `-mobile` / `-debugger`），本 change 不新增，`.type`/`.members`/`.time`/`.counters`/`.trace` 随反射/diagnostics 就绪并入（repl.md 已标注）。
