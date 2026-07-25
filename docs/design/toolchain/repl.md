# z42 REPL

> 状态：🟢 已实现（0.4.0；求值引擎 z42.scripting + 宿主 z42.interactive + launcher `z42 repl`）
>
> 相关：[scripting-charter.md](../compiler/scripting-charter.md) · [launcher.md](../runtime/launcher.md) · [self-hosting.md](../compiler/self-hosting.md)

## 定位

`z42 repl` 是 z42 原生交互式求值环境——输入 z42 代码、即时求值、打印结果、状态跨行持久。

设计准则：
- **REPL 本身是 z42 程序**（`z42.interactive.zpkg`），运行在 VM 上，完整 dogfood
- **z42c 不参与 eval/run**：编译器 zpkg 被 REPL 当库加载，不是入口
- **`z42 repl` 命令驱动**：由 launcher 路由；`z42`（无参数）显示 help，不自动进 REPL

## 触发方式

```bash
z42 repl              # 进入 REPL
z42 repl -c "1 + 2"   # 单次求值，输出结果后退出（类 python -c）
```

## 实现落地（0.4.0，add-z42-repl）

实测可运行：`z42 repl` 交互循环 + `z42 repl -c "expr"` 单次求值。相对原设计的落地要点：

- **求值编排**：`Std.Scripting.Script.Eval(state, input)` —— 分类（using / var 声明 / 表达式/语句）
  → 每轮 build 唯一命名空间 `Repl.R{N}` 源 → `PackageCompile.Compile` → `ZpkgWriterZ.ToBytes`
  → `__load_bytecode_in_memory` 内存加载 → `__invoke_static("Repl.R{N}.Eval{N}")` 取装箱结果。
- **状态模型（D7 + D8）**：会话变量提升为 `Vars{N}` 类静态字段；每轮 carry 前轮全部变量
  （`public static var v = Vars{prev}.v;`）+ 新变量；用户裸引用经 `Rewriter`（Lexer 基）改写为
  `Vars{N}.v`。**每轮 zpkg 落盘于会话临时目录并纳入 LibsDirs**——in-memory 轮次对 PackageCompile
  磁盘 DepScan 不可见，落盘使后续轮编译期可 carry-forward 引用前轮 Vars 类。
- **依赖 [`infer-var-field-types`](../../spec/archive/)（#24）**：`var` 字段跨 zpkg 保型，carry-forward
  的跨轮 `var` 变量算术/拼接才成立（原为 E0402）。
- **错误恢复**：编译失败 `EvalResult.Success=false`、会话不推进。
- **调用是自由函数**：`Eval{N}` emit 为自由函数（非类方法），经 `__invoke_static` 按 FQN 调；
  入口/eval 均自由函数（实证类方法作 entry 不解析）。

follow-up（未接）：多行输入（`__repl_readblock` builtin 已在，宿主未接）、`ResultFormatter`
对象反射展示（当前 `"" + v` ToString）、`.reset`/`.save`/`.history` 等元指令、fn/class 顶层声明累积。

## 架构

```
z42 repl
  │
  └── launcher.zpkg 路由 repl 命令
        │  Z42_LIBS = libs/ + programs/z42c/ + programs/interactive/
        └── z42vm programs/interactive/z42.interactive.zpkg
              ├── LineEditor      — __readline native builtin（Rust rustyline）
              ├── InputClassifier — 区分表达式 / 声明 / 语句
              ├── ReplSession     — 会话状态（growing transcript）
              └── z42.scripting   — Script.Eval() → compile + load + invoke
```

关键约定：
- REPL 通过 `z42.scripting` 调用已加载的编译器 zpkg（`programs/z42c/`），不直接依赖 `z42c` 命令
- 行编辑器由 Rust 侧实现（rustyline），通过 native builtin `__readline` 暴露给 z42 程序

## 状态模型：Growing Transcript

会话维护一个累积的"会话源文件"，每轮输入追加后整体重编译：

**变量声明 → 提升为 `$ReplVars` 静态字段**

```z42
// 用户输入: var x = 5
static class $ReplVars {
    static int x = 5;
}
static int $Eval_1() { return $ReplVars.x; }

// 用户输入: var y = x * 2  （$ReplVars 扩展）
static class $ReplVars {
    static int x = 5;
    static int y = $ReplVars.x * 2;
}
static int $Eval_2() { return $ReplVars.y; }

// 用户输入表达式: x + y
static int $Eval_3() { return $ReplVars.x + $ReplVars.y; }
```

**错误恢复**：编译失败时不追加本次输入，`NextState = prevState`（$ReplVars 保持上一轮状态），打印错误后继续等待输入。

**选型理由**：MVP 选 growing transcript（语义正确、实现简单、session 历史通常不超过几百行）。增量模块方案性能更好但跨模块状态共享复杂，defer（见下文）。

## 输入分类

| 类型 | 特征 | 处理 |
|------|------|------|
| 表达式 | 非声明、非控制流语句 | wrap → `$Eval_N()` → 打印返回值 |
| 变量声明 | `var x = ...` / `T x = ...` | 提升为 `$ReplVars` 静态字段 |
| 函数声明 | `fn name(...) { ... }` | 追加到顶层声明区 |
| 类声明 | `class Foo { ... }` | 追加到顶层声明区 |
| using | `using Std.IO;` | 追加到 using 列表 |
| 纯语句 | 赋值、有副作用调用 | wrap → `$Stmt_N()` → 执行不打印 |

**多行检测**：未闭合的 `{` / `(` / `[` → 显示 `...` 提示符继续读取，直到括号平衡。

## z42.scripting API

REPL 的编译 + 执行层，实现 scripting-charter Form B（状态承载）。位置：`libs/z42.scripting.zpkg`（stdlib，用户代码也可 import）。

```z42
namespace Std.Scripting {

    class ScriptState {
        string _sessionSource;
        int _evalCounter;
    }

    class EvalResult {
        bool Success;
        object Value;           // 表达式结果；语句/声明为 null
        string ErrorMessage;
        ScriptState NextState;  // 成功时为新状态；失败时 = 上一状态（不破坏会话）
    }

    class Script {
        static ScriptState Create() { ... }
        static EvalResult Eval(ScriptState state, string input) { ... }
    }
}
```

`Script.Eval` 内部流程：
1. `InputClassifier` 分类 input
2. 构造新 sessionSource（growing transcript 追加）
3. 调用已加载的 `z42c.pipeline` zpkg 编译
4. 调用 VM native API 加载内存模块（`LoadBytecodeInMemory`）
5. 通过 `Method.Invoke`（非泛型，0.3.12 已落地）调用 `$Eval_N()`
6. 返回 `EvalResult`

## 结果打印

| 值类型 | 输出 |
|--------|------|
| `void` / 纯语句 | 无输出 |
| 变量声明 | 打印赋值结果（同表达式） |
| 原始类型（int / f64 / bool / string）| 直接打印 |
| 对象 | `ToString()`；未重写则反射展示 `TypeName { field: val, ... }` |
| `null` | `null` |
| 数组 | `[elem1, elem2, ...]` |
| 运行时异常 | `RuntimeError: <message>` + 保留会话 |

## REPL 内置指令（`.` 前缀，不编译）

**约定**：以 `.` 开头的整行 = 元指令（meta，不进编译/transcript）；其余 = z42 代码。
未知 `.xxx` → `unknown command '.xxx'; try .help` 并保留会话。指令大小写不敏感、可带参。
标注：**[MVP]** = 0.3.15 首发；**[diag]** = 依赖 [diagnostics.md](../runtime/diagnostics.md)（事件/计数/时间）；
**[refl]** = 依赖反射；**[defer]** = 见下 Deferred。

### 会话控制
| 指令 | 功能 | |
|------|------|---|
| `.help [cmd]` | 无参列全部指令分组；带参看该指令详情 | [MVP] |
| `.exit` / `.quit` / Ctrl-D | 退出 REPL | [MVP] |
| `.reset` | 清空会话（transcript + `$ReplVars` 归零，回到空白 session）| [MVP] |
| `.clear` | 清屏（**不**清会话状态）| [MVP] |

### 历史 / 转录
| 指令 | 功能 | |
|------|------|---|
| `.history [n]` | 显示最近 `n` 条 eval（默认全部，带行号）| [MVP] |
| `.save <file.z42>` | 把当前会话 transcript 导出为可独立编译的 `.z42` | [MVP] |
| `.load <file.z42>` | 把文件内容按行喂入会话 | [defer] `repl-future-load-directive` |

### 作用域内省
| 指令 | 功能 | |
|------|------|---|
| `.vars` | 列会话变量：`name : Type = value` | [MVP] |
| `.types` | 列会话内声明的类型 | [MVP] |
| `.usings` | 列当前生效的 `using` | [MVP] |
| `.using <ns>` | 给会话追加一个 `using <ns>;` | [MVP] |
| `.type <expr>` | 显示表达式的**静态类型**（typecheck，不求值；类 GHCi `:type`）| [refl] |
| `.members <Type>` | 反射列出类型成员（字段/方法/属性）| [refl] |

### 执行 / 诊断
| 指令 | 功能 | |
|------|------|---|
| `.mode [interp\|jit]` | 无参显示当前执行模式；带参切换（`ExecMode`，JIT 平台才有 jit）| [MVP] |
| `.time <expr>` | 求值并报告**编译 + 执行耗时**（per-eval span）| [diag] |
| `.counters` | 打印运行时计数器快照（编译次数/异常数/分配等）| [diag] |
| `.trace [on\|off\|<cat>]` | 开关事件跟踪（编译/GC/类型加载…按 category）| [diag] |

### 元信息
| 指令 | 功能 | |
|------|------|---|
| `.version` | z42 运行时 + 编译器 zpkg 版本 | [MVP] |

### `.help` 输出样例
```
z42 REPL — 输入 z42 代码即时求值；. 前缀为元指令。
  会话:   .help [cmd]  .exit/.quit  .reset  .clear
  历史:   .history [n]  .save <f.z42>
  内省:   .vars  .types  .usings  .using <ns>  .type <expr>  .members <Type>
  执行:   .mode [interp|jit]  .time <expr>  .counters  .trace [on|off|<cat>]
  元:     .version
  (.type/.members 需反射；.time/.counters/.trace 需 diagnostics)
```

> **MVP 指令集**（0.3.15 首发）：`.help .exit .quit .reset .clear .history .save
> .vars .types .usings .using .mode .version`。`.type`/`.members` 随反射就绪并入；
> `.time`/`.counters`/`.trace` 随 [diagnostics.md](../runtime/diagnostics.md) 落地并入；
> `.load` 见 Deferred。

## 行编辑器

Rust 侧 `rustyline` 实现，通过 native builtin 暴露给 REPL 程序：

```z42
// z42.interactive 内部调用
string line  = Std.Repl.ReadLine(">>> ");
string block = Std.Repl.ReadBlock(">>> ", "... ");  // 多行（括号平衡检测）
```

功能：历史记录（上下键）、行编辑（Ctrl-A/E/K/U）、Ctrl-D 退出。Tab 补全 deferred（依赖 LSP）。

## 包位置与 Z42_LIBS

| 包 | 位置 | 说明 |
|----|------|------|
| `z42.scripting.zpkg` | `libs/` | stdlib，用户也可 import |
| `z42.interactive.zpkg` | `programs/interactive/` | REPL 主程序（exe zpkg）|

`z42 repl` 运行时 Z42_LIBS：

```
libs/ + programs/z42c/ + programs/interactive/
```

`programs/z42c/` 含编译器 5 个 zpkg（IR 收敛后），是 `z42.scripting` 运行期动态加载的依赖。

## 前置依赖

| 依赖 | 落地版本 |
|------|---------|
| 自举编译器 7 个 zpkg（byte-identical CI gate）| 0.3.10 |
| Boxing 机制 | 0.3.11 |
| 非泛型 Method.Invoke | 0.3.12 |
| programs/ 目录布局 + z42 apphost 化 | 0.3.15 spec 前置（launcher 布局修订）|

## Deferred / Future Work

### repl-future-tab-completion

- **来源**：0.3.15 设计讨论
- **触发原因**：Tab 补全候选列表需要 LSP server 提供语义信息
- **前置依赖**：0.5.x LSP v1（`z42-lsp`）
- **触发条件**：LSP 落地后
- **当前 workaround**：无补全，依赖历史记录导航

### repl-future-incremental-compilation

- **来源**：0.3.15 设计讨论（Growing Transcript 性能权衡）
- **触发原因**：Growing Transcript 是 O(n) 重编译；小 session 可接受，大 session 慢
- **前置依赖**：增量模块加载 + 跨模块静态状态共享 VM 能力 —— 即 [load-context.md](../runtime/load-context.md)（每轮输入 = 一个加载上下文，重定义 = 新版 supersede 旧版 + 旧版无引用时回收 `whyRetained`）+ [componentized-runtime.md](../runtime/componentized-runtime.md)（运行时编译器作为可加载组件）。该增量方案 = "每行 = 一个 context" 模型，其使能基建已在 2026-06-21 运行时设计弧中落定（DESIGN）。
- **触发条件**：session 规模成为实际性能瓶颈时（benchmark 驱动）
- **当前 workaround**：Growing Transcript（session 历史通常不超过几百行）

### repl-future-load-directive

- **来源**：0.3.15 设计讨论
- **触发原因**：`.load file.z42` 指令 ROI 低，MVP 不做
- **触发条件**：用户呼声出现
- **当前 workaround**：手动 copy-paste 或 `z42 repl -c "$(cat file.z42)"`

### repl-future-mobile

- **来源**：scripting-charter C6
- **触发原因**：编译器 zpkg 进 mobile 分发依赖 1.1.x；iOS/WASM W^X 限制
- **前置依赖**：scripting-charter C5 + Q15（WASM GC）
- **触发条件**：1.1.x+ mobile scripting 落地时

### repl-future-debugger

- **来源**：0.3.15 设计讨论
- **触发原因**：调试集成需要 DAP server + VM 单步支持
- **前置依赖**：0.8.x DAP debugger
- **触发条件**：DAP 落地后
