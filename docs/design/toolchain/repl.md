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
z42 repl -h           # 显示帮助（参数 + 元指令列表；不进 REPL）
z42 repl --mode interp -c "1+2"   # 指定执行模式（默认 jit；也可 Z42_MODE / [runtime].mode）
z42 repl --config <file>          # 指定 runtime config（设 Z42_CONFIG）
```

> **`-h`/`--help` 由 launcher 直接处理**（add-repl-help）：`repl` 在 launcher 里是特判命令，绕过
> 生成式 help 路由——若不拦截，`-h` 会被原样透传给 z42i（不识别 → 照常启动 REPL，看不到帮助）。
> 故 `_forwardRepl` 入口显式拦 `-h`/`--help` 打印帮助（跳过 `-c` 的值，`z42 repl -c "-h"` 仍求值该串）。
> 帮助里的选项须与 `_forwardRepl` 的 launcher 级 flag（`--mode`/`--config`）+ z42i 自身参数（`-c`）同步。

## 实现落地（0.4.0，add-z42-repl）

实测可运行：`z42 repl` 交互循环 + `z42 repl -c "expr"` 单次求值。相对原设计的落地要点：

- **求值编排**：`Std.Scripting.Script.Eval(state, input)` —— 分类（using / var 声明 / **顶层
  函数/类型声明** / 表达式/语句；分类器 `Classifier.z42` token 级判别）→ 每轮 build 唯一命名空间
  `Repl.R{N}` 源 → `PackageCompile.Compile` → `ZpkgWriterZ.ToBytes` → `__load_bytecode_in_memory`
  内存加载 → `__invoke_static("Repl.R{N}.Eval{N}")` 取装箱结果（声明轮无 `Eval{N}`、不 Invoke）。
  - **尾随分号归一（fix-repl-trailing-semicolon）**：表达式/语句轮 build 源前剥掉输入末尾单个 `;`。
    否则表达式包裹 `return (X;)` 破坏括号（E0202）、语句回退 `X;; return null;` 撞 `;;` 空语句
    （parser 拒）→ 两路皆炸、该轮副作用丢失。剥尾随后 `expr;`/`stmt;` 与不带 `;` 等价；多语句
    `a();b();` 的中间 `;` 保留、仅尾随剥掉 → 回退路径 `a();b(); return null;` 自然合法。
- **声明累积（add-repl-decls-multiline）**：顶层函数/类型声明**原样**作 `Repl.R{N}` 命名空间成员编译
  （不裹壳类、不改写函数体），`ExtendWithPackage` 并入 CachedScan + `LoadBytes` 进 VM，记 `Repl.R{N}`
  到 `ScriptState.DeclNamespaces`；**每后续轮** prelude 追加 `using Repl.R{N};`——自由函数经
  `fix-imported-free-func-namespace`、类型（含实例方法）经 `ImportedClassNs` + **增量导入 world-extension**
  （见下）、enum 经 **TsigReconcile enum 导出**（见下）跨包裸调/裸用解析。重定义同名 → ERROR
  （`DeclNames` 查重，不 supersede）。声明体**不**捕获会话变量（Deferred `repl-future-decl-capture-vars`）。
  - **缺省可见性补 `public`（fix-repl-default-type-visibility）**：类型默认可见性是 `internal`，但 REPL 每轮是**独立
    package**（`Repl.R{N}`）——裸 `class Foo { public int X; }` 会双重踩雷：① 同轮 `internal` 类含 `public` 成员 →
    `E0441` 不一致可访问性；② 下一轮跨 package 引用 → `E0404` 跨包 internal 访问被拒。故 `Classifier` 记录声明是否
    **显式**写了可见性（`ParsedInput.HasVisibility`），`_evalDecl` 对**未显式写可见性的类型声明**自动前缀 `public`
    （显式写 `internal`/`public` 的尊重用户；自由函数不需要——跨包 internal 自由函数 REPL 本就可用）。这样裸
    `class`/`struct` 跨轮可用，符合 REPL「一个连续会话」的直觉。
- **增量导入的两处 compiler 修复（同 change）**：REPL 靠 `DepScan.ExtendWithPackage` 增量并入声明包，
  此前不完整重建类型元数据 → ① 类**实例方法** `no method`（world 不含增量包 → `TsigReconcile._rebuildClass`
  定位不到类自身、读不到 SIGS 方法）：`ExtendWithPackage` 现在 Rebuild 前把本包并入 `scan.Wp`；
  ② **enum 类型** `undefined`（`TsigReconcile._rebuildModule` 恒排除本地 enum）：现从 TYPE 段成员块重建
  `ExportedEnumZ` 导出（亦修**一般跨包 enum 导入**）。均无格式 bump。
- **默认 using（隐式导入；add-completion-query-api）**：`Script.Create` 给新会话种一组默认
  `Usings`（`Std.IO` / `Std.Collections` / `Std.Text` / `Std.Math`），对齐 C# `ImplicitUsings` /
  Kotlin·Scala REPL 默认 import——`Console` / `List` / `StringBuilder` / `Math` 开箱即用，无需手动
  `using`。保守集（避跨命名空间同名类型歧义）；其余按需 `using`，`.usings` 元指令列出全部（含默认）。
  **不加重首轮 scan**：默认 using 只是把命名空间放进解析作用域，包仍在符号被真正引用时才进世界
  （现为全量 scan，惰性化后更明显——见 `repl-future-persist-static-scan` 邻域的惰性方向）。
- **状态模型（D7 + D8；2026-07-26 perf ⑤ 精简）**：会话变量提升为 `Vars{N}` 类静态字段。
  **只有 var 声明轮**发新的 `Vars{N}` 类（carry 前轮全部变量 `public static var v = Vars{prev}.v;`
  + 新变量）；**非声明轮**（表达式 / 语句 / 赋值）直接引用现有 `Vars{VarsRound}` 类——赋值就地改其
  静态字段（值在 VM 静态存储中持久，后续轮可见），无需发新类。用户裸引用经 `Rewriter`（Lexer 基）
  改写为对应 `Vars` 类的成员访问。
- **carry-forward 机制（2026-07-26 perf ②/⑤ 从磁盘改为内存增量）**：曾**每轮 zpkg 落盘**于会话临时目录
  并纳入 LibsDirs（使后续轮 DepScan 能引用前轮 Vars 类），但那让每轮重扫整个世界（~3.5s）。现改为：
  首轮建一次静态 `DepScanResult` 缓存于 `ScriptState.CachedScan`；每个 var 声明轮把刚编出的 `Vars{N}`
  包经 `DepScan.ExtendWithPackage` **增量并入缓存 scan**（不落盘、不重扫）；后续轮经
  `CompileInputs.CachedScan` 复用。详见 [self-hosting.md] 邻近的 compile-pipeline 说明与
  `bench/repl/BASELINE.md`。
  - **内存包的驻留识别（fix-repl-inmemory-dep-warn）**：声明轮的包（`repl_r{N}`）只在内存加载、
    从不落盘，但后续轮引用其类型时编出的依赖仍按规范文件名 `repl_r{N}.zpkg` 记录。VM lazy loader
    加载任一包时会把 `<包名>.zpkg` 记进 `loaded_zpkgs`（驻留集），使依赖方的依赖解析循环在驻留集
    命中即短路——否则会对从不存在的 `repl_r{N}.zpkg` 发起磁盘查找并刷 `WARN: cannot read dep
    zpkg meta`（引用其实经进程内已加载模块正常解析）。见 `metadata/loader.rs` 的
    `LoadedArtifact.package_name` 与 `lazy_loader.rs` 的 `register_loaded_artifact`。
- **依赖 [`infer-var-field-types`](../../spec/archive/)（#24）**：`var` 字段跨 zpkg 保型，carry-forward
  的跨轮 `var` 变量算术/拼接才成立（原为 E0402）。
- **错误恢复**：编译失败 `EvalResult.Success=false`、会话不推进。
- **错误行号回映（add-repl-ns-completion-err-map）**：REPL 把用户输入包进生成源（prelude + wrapper），
  诊断原报生成源坐标 `repl_rN.z42(生成行,列): …`（对用户无意义）。`Script._remapDiag` 把每条诊断的行回映
  为**用户输入行 = 生成行 − prelude换行数**（表达式轮/声明轮统一），丢内部文件名 + 列号（Rewriter 改写
  会移列、不可靠）。单行输入 → 只留 `<code>: <msg>`（如 `E0401: undefined: x`）；多行块 → `第 N 行: <msg>`。
- **调用是自由函数**：`Eval{N}` emit 为自由函数（非类方法），经 `__invoke_static` 按 FQN 调；
  入口/eval 均自由函数（实证类方法作 entry 不解析）。
- **多行输入（add-repl-parser-completeness；早期 add-repl-decls-multiline）**：宿主 `interactive_main`
  **逐行**读、累积到 `buf`，由 **parser 权威**的 `Std.Scripting.Completeness.IsIncomplete` 判「写完没」——
  没写完（缺 `}` / 操作数 / 未闭合 `(){}[]` 等）续读，写完才整块交 `Script.Eval`（多行 fn/class 整体到
  分类器）。元指令单行即时返回；EOF→null 退出不变。完整性机制详见 book 页
  [`repl-input-completeness.md`](../../book/src/toolchain/repl-input-completeness.md)。
- **续行自动缩进（sink-repl-indent-to-script；早期 add-repl-multiline-completion）**：续读一行前，**脚本层**
  用既有 Lexer 数 `buf` 未闭合括号层数，算 `层数 × 4 空格`（`Std.Scripting.Completeness.ContinuationIndent`），
  交 `Repl.ReadLine(prompt, initial)` 的 `initial` 预填（经 rustyline `readline_with_initial`，光标落缩进后
  可编辑）→ 写 fn/class 体像编辑器而非顶格。缩进是编辑便利、对 parser 纯空白无语义影响；以闭括号 `}` 起头
  的行多缩一级，用户 backspace 一次（auto-dedent-on-`}` 为 follow-up）。非交互路径（管道/no-tty，
  `plain_readline`）忽略预填——输入自带文本。**native 侧不再留括号状态机**（早期 `bracket_depth` 已删）。

元指令落地状态（`interactive_main`）：已接 `.help .exit .quit .reset .clear .vars .types .usings
.using <ns> .type <expr> .version`（`.type` 见 add-repl-type-metacommand：**运行期类型**——`Script.Eval`
求值 `(<expr>).GetType().Name`，类 Python `type()`；求值一次、副作用照常；`.version` 打印 zbc/zpkg
**格式**版本——z42vm 运行时版本串未经 builtin 暴露给 z42，留 follow-up）。仍未接：`.history` / `.save`
（需宿主存 transcript）、`.mode`（需 `ExecMode` 接口）；`.time`/`.counters`/`.trace`
随 diagnostics 并入（见下表标注）。

follow-up（未接）：`ResultFormatter` 对象反射展示（当前 `"" + v` ToString）、`.history`/`.save`/`.mode`
等余下元指令。（多行输入 + fn/class/enum 顶层声明累积、`.reset`/`.clear`/`.using`/`.types`/`.version`
已落地。）

## 架构

```
z42 repl
  │
  └── launcher.zpkg 路由 repl 命令
        │  Z42_LIBS = libs/ + programs/z42c/ + programs/interactive/
        └── z42vm programs/interactive/z42.interactive.zpkg
              ├── z42.repl        — Std.Repl.Repl 行编辑（rustyline）+ ReplEditing 键位（tty 交互层，tier1）
              ├── z42.scripting   — Script.Eval() → compile + load + invoke（跨平台 eval-core）
              │                     + Classifier（分类）/ Completeness（完整性）/ Completer（补全）
              └── ReplSession     — 会话状态（growing transcript）
```

关键约定：
- **包分层（split-z42-repl）**：`z42.scripting`(`Std.Scripting`) = 跨平台编译+执行内核（playground/wasm 亦用）；
  `z42.repl`(`Std.Repl`) = tier1 终端行编辑 / 键位。z42.repl 单向依赖 z42.scripting（Completeness）；
  反射查询 `Engine.MemberNames`（补全用）归 scripting 引擎、不归 repl（消 scripting↔repl 环）。
- REPL 通过 `z42.scripting` 调用已加载的编译器 zpkg（`programs/z42c/`），不直接依赖 `z42c` 命令
- 行编辑器由 Rust 侧实现（rustyline），通过 native builtin `__repl_readline` 暴露给 z42 程序（`Std.Repl.Repl`）

## 启动预热（后台线程，add-repl-prewarm 2026-07-29）

**问题**：首次 `Eval` 要一次性构建依赖世界（`DepScan.ScanDirsLazy` 扫全 stdlib+编译器 zpkg 世界
+ 加载默认 using 包，实测 ~1.9–3.4s），与用户输入**零相关**，却懒到用户敲完第一行回车后才在
`Script.Eval` 里同步跑 → 首次 eval「回车后干等」。

**方案**：启动即把这份 input-independent 的工作 spawn 到后台线程，与用户在提示符打字并行。

```
interactive_main.Main()  [主线程]                worker  [Std.Threading.Thread]
  s = Script.Create()          ── spawn ──▶   Script.Prewarm(s):
  s.PrewarmThread = Thread.Start(…)              scan = DepScan.ScanDirsLazy(…)   ~1.4s
  Repl.SetCompleter(…)                           for u in Usings: EnsurePackageLoaded(scan,u)
  loop:                                          state.CachedScan = scan   ◀── 末尾原子发布
    line = Repl.ReadLine(">>> ", "")  ← 主线程阻塞在原生 readline（GC-safe park，见下）
    r = Script.Eval(s, line)
        └─ _ensureWarm(s): PrewarmThread.Join()  ← 首次消费前汇合（已完成则瞬回）
```

- **handoff 无锁**：`Prewarm` 全程操作**本地** `DepScanResult`，仅**末尾**一次 `state.CachedScan = scan`
  原子发布（64 位指针写）。并发读该字段的补全器只见 `null`（既有 null 分支返回空）或完全成品，绝不见
  半构造 / 正被 `EnsurePackageLoaded` 变异的中间态。故无需锁、无需 gate flag。
- **汇合与兜底**：`Eval` 顶部 `_ensureWarm` —— 有 worker 则 `Join`（已完成瞬回；打字极快则阻塞到完成，
  退化为同步、不更差）；worker 异常 / 无 worker（`.reset` 重建 s / `-c` 单次求值）→ inline `Prewarm` 兜底。
- **等价性**：预热构建的 `CachedScan` 与旧「首轮 Eval 内同步构建」语义等价，求值结果 / 错误 / 补全候选不变。

### GC-safe park（关键前置，corelib/repl.rs + gc/safepoint.rs）

z42 是单线程协作式 GC：停顿收集器要等所有其它线程主动停到 safepoint（命中 `check_safepoint`），而线程
只在**执行字节码**时命中。主线程阻塞在原生 rustyline `readline` 里**永不命中** → 若后台 worker 分配触发
GC，收集器会死等主线程 park（要到用户回车才解）→ 预热在首次 GC 卡死。

解法（等价 JVM `_thread_in_native` / Go `entersyscall`）：主线程进入阻塞原生读取前把自己登记为
「已 park」（`parked_count += 1`），离开时按 STW 相位等到安全再解除（`NativeParkGuard`）。其 z42 根在原生
调用期间冻结、可被收集器安全扫描。Tab 补全回调在 readline 内重入 VM，对称地临时 un-park（`NativeUnparkGuard`：
`exit → 跑 z42 → enter`）作正常 mutator 参与 safepoint。复用既有 `parked_count` + `gc_phase_cv`，无新同步原语。

> 引入：change `add-repl-prewarm`（`docs/spec/archive/…-add-repl-prewarm`）。冷启动/无间隙仍付一次构建，
> 彻底消除见 Deferred `repl-future-persist-static-scan`（磁盘持久化，正交可叠加）。

## 惰性类型世界（lazy-type-world，2026-07-30）

**问题**：预热/首次 eval 的主导成本是 `TsigReconcile.BuildWorld`——它一次性把全部 ~34 个 stdlib+编译器包
的**完整 TYPE/SIGS 元数据**解析进 `Wp`，**无论输入引用了什么**。这是 **O(标准库总量)**：库越大首轮越慢。

**方案**：`Wp` 改为 `LazyReconWorld` **按包懒填**。`TsigReconcile.Rebuild` 重建类的基类链时，遇导入祖先
FQ → 取其命名空间（最后一个 `.` 之前）→ 经 NSPC 建的路由表定位到**声明该 ns 的所有包** → 只解析那几个
（`EnsureFq`）。递归覆盖传递闭包；扫描跳过未填充（null）条目。

```
首次 eval "1+2"（不引用外部）→ 解析 prelude + 默认 using 闭包（实测 14/34 包），非全部 34
加载一个包 → 只解析它 + 基类链祖先闭包（spike 实测 max=3、avg=1）
标准库变多 → 首轮解析量不变（O(引用闭包)，不随总量增长）——根治「库变多更卡」
```

- **正确性**：路由定位与「扫遍全量 world」等价（spike 230/230 基类链 0 不匹配）；每包解析确定性 → 懒填与
  eager 全量产出**逐字节相同**。自举字节不动点 **5/5 gen1==gen2**（gen1=旧 eager 编译器、gen2=新惰性
  编译器，二者字节相同即证 eager==lazy）+ cross-zpkg 8/0 + stdlib 279/23。
- **零格式 bump、零 VM 改动**。`build/test` 全量编译走 `ScanDirs`（同惰性 world，Rebuild 全部包 → 填满）。
- **种子兼容**：改 `Rebuild` 签名踩 bootstrap 轴④（z42c 运行期自依赖 z42.ir）——保留旧 4-arg `Rebuild`
  + `BuildWorld` 作 eager 包装重载给种子，跨一版种子后可删。

> 引入：change `lazy-type-world`（`docs/spec/archive/…-lazy-type-world`）。残余优化（不预加载默认 using +
> 后台符号名字索引、延后 `Open`/STRS）见该 change 的 design Deferred。

## 惰性 prelude + ns 索引（repl-scan-nsindex-cache，2026-08-01）

**动机（对照 Python）**：`z42 repl -c "1+1"` = 1.7s，Python `1+1` = 0.01s，z42 VM 裸启动 = 0.00s。差距全在
「每次求值都跑整个 AOT 编译器 + **eager reconcile 整个 prelude+usings 类型世界**」——`1+1` 只需内建 `int`+
算子，却拖动整个 stdlib（lazy-type-world 已把类型世界惰性化，但**首次 eval 仍 eager reconcile prelude(z42.core)
+ 预载 4 个默认 using**，其 base 链级联把整个 stdlib 都开+读了）。Python 快在**解释 + 运行期惰性名字解析**，
对 `1+1` 根本不干这些活。

**T1 惰性 prelude（核心）**：`ScanDirsLazy` **不再 eager reconcile prelude**、`Script.Prewarm` **不再预载默认
usings**——worker 只建**骨架**（nsMap 路由 + 惰性 world）→ 秒发布。`1+1` 类纯表达式**零包加载**：
- **1.72s → 0.40s（4.3×，秒回车也不卡）**。
- 首次引用 Std 符号时 `Script._compileSrc` 既有的 **E0401 回退**按需加载 prelude+usings（默认 using
  `Std.Collections` 由 z42.core 声明 → 加载它即把 prelude 一并 reconcile；`object`/算术为编译器内建，`1+1`
  连 prelude 都不碰）。
- **不后台 reconcile**：本 VM 的 GC 安全点协作会让计算密集的**后台线程阻塞主线程 eval**（主线程 `1+1`
  编译分配 → 触发 GC → 死等后台 reconcile 到安全点 ~3s），故放弃「后台预热完整世界」（旧 prewarm 能藏住
  reconcile 仅因 readline 是 native-park 释放 GC；计算不 park）。**权衡**：首次**符号** eval（Console/List）
  走前台 E0401 ≈ 1.7s（不再被间隙隐藏）——**已由 T2 按符号 reconcile 承接落地**（见下节 T2/#98/#101：
  首次 `Console` 1.7s→**0.33s**、`Math` 再 -31%）。

**B ns 索引持久化（辅）**：`ScanDirsLazy` 把「每 zpkg → 命名空间列表」落盘缓存 `.z42-nsindex`（按 libs
指纹 `basename:size:mtime` key）；命中则从缓存建 nsMap + `LazyReconWorld` 路由（`LazyFromPairs`），**不再
open-all 全部 ~26 个包**，只按需 `Open` 引用闭包（`LazyReconWorld.EnsureIdx` 惰性 open + `EnsurePackageLoaded`
惰性 open）。使骨架 scan 从 ~0.4s（open-all）→ ~0.05s（命中），**是 Windows 的对症解**（消除 20+ 次被
Defender 逐个扫的文件打开）。指纹变 → 索引自动重建；不可写 libs → 回退 open-all（不坏）。

- **零格式 bump、零 VM 改动**；非惰性 `ScanDirs`（build/test 路径）**零改** → 自举 byte-identical 不受影响。
- **正确性**：命中 vs 未命中（open-all）产出的 nsMap/Exported 一致；skip-prelude 后表达式/字符串/Console/
  List/Math/Convert/Environment/声明/跨轮 var 全对。GREEN：自举字节不动点 + e2e 215/0 + cross-zpkg 8/0。

> 引入：change `repl-scan-nsindex-cache`（`docs/spec/changes/…`）。**T2 按符号 reconcile 已落地**（下节，
> #98/#101 completer 版）：用 `Console` 只 materialize `Console` 一个类型（+基类链），不 reconcile 整个
> Std.IO+prelude 闭包 → 首次符号 eval 1.7s→0.33s、且不需后台线程（避开 GC 坑）。**仍为 future 的只剩深层变体**：
> 把它从「REPL E0401 错误恢复路径的 completer」下沉进**编译器名字解析核心**（`SymbolTable` 惰性 miss 回调 +
> 按类型 zpkg 读 + arity-mangle/impl 合并/接口顺序等不按类型分解的整包问题）——ROI 低（completer 版已达
> ~150ms List reconcile 下限），需独立 spec + 门禁，暂缓。

## T2 按符号 reconcile（completer）+ type→ns 索引（repl-per-symbol-reconcile / repl-type-nsindex，2026-08-02）

**T2 completer（PR #98）**：没走「编译器名字解析核心」的 SymbolTable-miss 回调（改动面过大），而是在
`Script._compileSrc` 的 **E0401 错误恢复路径**装一个 completer——REPL-only、零编译器内核改动：

1. `_compileSrcOnce` 报 E0401 → `_loadReferencedTypes`：`_typeCandidates` 字符扫源取首字母大写标识符（类型
   候选）→ 对每个活跃命名空间调 `DepScan.ReconcileCandidatesInNs`（每 ns 读一次模块、批量 reconcile 命中的
   候选类型，`TsigReconcile.ReconcileOneFromDesc` 从已读 `IrClassDesc` 直接 reconcile，不整包）。
2. 仍失败 → 回退整包 `_loadUsingsPackages`（保正确性）。

**守自举**：completer 全部新增代码只在 REPL 错误恢复路径调用，build 的 `ScanDirs`/整包 `Rebuild` 一字未改
→ 字节不动点守住。首次 `Console.WriteLine` **1.7s → 0.33s（5×）**。

**type→ns 索引（repl-type-nsindex）——去掉「全扫活跃 ns」**：completer 起初对**每个**活跃命名空间都读一次
模块找候选类型（首次 Console 全扫 6 个活跃 ns/~100ms，其中只有 Std.IO 真的含 Console，其余全白读）。根因：
`.z42-nsindex` 只记「包→命名空间」，不知道 `Console` 在哪个 ns。**解**：把 ns 索引升级 `NSIDX1→NSIDX2`，每
ns 字段由 `ns` 变 `ns=T1,T2`（该 ns 声明的**类型短名**）；completer 经 `DepScanResult.NsMayHaveCandidate`
只读「索引显示声明了某候选类型」的活跃 ns。

- **数据流**：cold-start（cache-miss）的 open-all 顺带 `ReadModuleTypes` 提取每 ns 类型短名（**一次性
  ~159ms/490 类型/26 包**，落盘 `.z42-nsindex`）→ warm 命中路径 `_scanFromIndex` 从落盘 `NsTypes` 直接建
  `TypeShort[]/TypeNs[]` 映射（不重读模块）→ completer `NsMayHaveCandidate` O(候选×映射) 过滤活跃 ns。
- **效果**：Math.Max **0.26→0.18（-31%，Math 在第 4 个 using，索引跳过前 3 个白读）**、Console 0.34→0.29、
  List/Dict ~-10%。**剩余下限 = 那一个必要的 reconcile**（List reconcile ~150ms，索引消不掉）。
- **索引空（旧缓存/写失败/非惰性路径）→ `NsMayHaveCandidate` 恒 true → 退化为全扫**（旧行为，正确性不变）。
- **零格式 bump（zpkg 不动，只动 `.z42-nsindex` 本地缓存，+~5KB）、零 VM 改动**；`ScanDirs`（build/test）零改
  → 自举字节不动点 5/5 不受影响。

> 引入：change `repl-per-symbol-reconcile`（completer）+ `repl-type-nsindex`（索引）。**已知未覆盖**：完全
> 限定名 `Std.IO.Console.WriteLine`（`undefined: Std`）是 REPL **既有**限制（completer 前后一致），非本改动引入。

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

分类器 `Classifier.z42`（token 级，跳过前导修饰符）。实际落地为 `Repl.R{N}` 每轮唯一 ns 模型
（非旧 Growing-Transcript「顶层声明区」叙述）：

| 类型 | 特征（token） | 处理 |
|------|------|------|
| 表达式 | 非声明、非控制流语句 | wrap → `Eval{N}()` → 打印返回值 |
| 变量声明 | `(var\|T) x =`（token2=`=`） | 提升为 `Vars{N}` 静态字段（carry-forward） |
| 函数声明 | `[修饰符] RetType Name (`（token2=`(`） | 原样入 `Repl.R{N}` ns，`DeclNamespaces` 登记，后续轮 `using` |
| 类型声明 | `[修饰符] class\|struct\|record\|interface\|enum Name` | 同上（enum 靠 TsigReconcile 导出、类实例方法靠 world-extension 跨轮解析） |
| using | `using Std.IO;` | 追加到 using 列表 |
| 纯语句 | 赋值、有副作用调用（顶层 `=`/语句关键字） | wrap → `Eval{N}()` `; return null;` 执行不打印 |

重定义同名函数/类型 → ERROR（`DeclNames` 查重，不 supersede）。

**多行检测**：`interactive_main` **逐行**读、累积，由 parser 权威的 `Completeness.IsIncomplete` 判「写完没」
（缺 `}` / 操作数 / 未闭合 `(){}[]` 等）→ 没写完 `... ` 续行、写完才求值。续行缩进由脚本层
`Completeness.ContinuationIndent`（Lexer 数括号）算好、交 `Repl.ReadLine` 预填；native 无括号状态机。

## z42.scripting / z42.repl API

REPL 拆两包（split-z42-repl）：**`z42.scripting`(`Std.Scripting`)** = 跨平台编译+执行内核（scripting-charter
Form B，状态承载；`libs/z42.scripting.zpkg`，playground/wasm/用户代码亦可 import）；**`z42.repl`(`Std.Repl`)**
= tier1 终端行编辑 / 键位（`libs/z42.repl.zpkg`）。下方 `Std.Scripting` 面为 eval-core；`Std.Repl.Repl`（`ReadLine`
/ `SetCompleter` / `SetKeyEditor`）+ `Std.Repl.ReplEditing.KeyEdit` 为 tty 交互面。

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
| `.type <expr>` | 显示表达式的**运行期类型**（`Script.Eval` 求值 `(<expr>).GetType().Name`，类 Python `type()`；求值一次、副作用照常）| [refl] ✅ |
| `.members <T>` | 列出类型/变量成员（复用 `replComplete("<T>.", …)`：类型→静态成员、变量→live 实例成员）| [refl] ✅ |

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
  (.type/.members 已接；.time/.counters/.trace 需 diagnostics)
```

> **MVP 指令集**（目标）：`.help .exit .quit .reset .clear .history .save
> .vars .types .usings .using .mode .version`。
> **已落地**：`.help .exit .quit .reset .clear .vars .types .usings .using .type .members .version`
> （`.type` = 运行期类型，add-repl-type-metacommand；`.version` 仅格式版本）。**未接**：`.history` /
> `.save`（需 transcript 存储）、`.mode`（需 `ExecMode` 接口）；
> `.time`/`.counters`/`.trace` 随 [diagnostics.md](../runtime/diagnostics.md) 落地并入；`.load` 见 Deferred。

## 行编辑器

Rust 侧 `rustyline` 实现，通过 native builtin 暴露给 REPL 程序：

```z42
// z42.interactive 内部调用（逐行读，多行累积/完整性/缩进都在脚本层，见「多行输入」节）
string line = Std.Repl.ReadLine(">>> ", "");                                  // 主行（initial 空）
string cont = Std.Repl.ReadLine("... ", Completeness.ContinuationIndent(buf)); // 续行（预填缩进）
```

功能：历史记录（上下键）、行编辑（Ctrl-A/E/K/U）、Ctrl-D 退出、多行续读（逐行读 + 脚本层 parser 完整性判定）。

**历史跨会话持久（add-repl-history-keyword-completion）**：编辑器 init 时 `load_history`、每行后
`save_history` 到 `$HOME/.z42_history`（Windows 回退 `%USERPROFILE%`）——上一会话的输入下次上箭头可
找回。best-effort：缺文件/写失败/`$HOME` 未设都不影响 REPL（退化为纯进程内）。非 tty（`plain_readline`）无历史。

### 补全与 inline 提示

- **Tab 补全**（`ReplHelper: Completer`，add-completion-query-api）：Tab 触发，经进程全局
  `REGISTERED_COMPLETER`（z42 `Std.Scripting.replComplete`，由 `Repl.SetCompleter` 注册）回调 VM，
  返回作用域变量 / 会话声明名 / 导入类型名（含 ns 索引未 reconcile 的，`_addIndexedTypeNames`）/
  `Type.` 静态成员（未 reconcile 则按需 reconcile，`_ensureReconciled`）/ `obj.` 实例成员（live 反射，
  **含基元变量**——`s.`/`n.` 映射到 `GetType()` 同款 canonical 类 `Std.String`/`Std.Int32`… 反射其成员，
  add-repl-completion-iter2）/ **z42 语言关键字**（`_addKeywords`，以 `Z42.Syntax.Lexer` 的
  `KeywordCount`/`KeywordNameAt` 为权威源、零漂移）/ **命名空间名**（`.using ` 上下文，`_namespaceComplete`
  按 `DepScanResult.NsNames` 返回下一段候选：`.using Std.C`→`Collections`/`Cli`/…，add-repl-ns-completion-err-map）
  候选。回调在 readline 内重入 VM，临时 un-park 参与 safepoint（见上 GC-safe park）。
  - **List 模式**（add-repl-completion-iter2）：编辑器以 `CompletionType::List` 构造（bash 式：首 Tab 补最长
    公共前缀、再 Tab 列候选）——替代 rustyline 默认 `Circular`（反复 Tab 循环候选、转一圈**回到原始输入**，
    观感为「Tab 越按越退」）。
- **inline ghost 提示**（`ReplHelper: Hinter`，add-repl-multiline-completion）：**边打字**把补全以灰字
  幽灵显示（`highlight_hint` = ANSI `\x1b[90m`）。仅行尾提示：先试**补全 ghost**——复用同一 completer，
  **裸标识符与成员上下文均支持**（`Con`→`sole`、`Console.W`→`riteLine`；成员上下文原为省每键反射而跳过，
  add-repl-type-metacommand 起放开，completer 按需 reconcile 接收者、命中后缓存，成本与 Tab 相当），
  `starts_with` 严格前缀过滤后取首个扩展候选的后缀（非扩展 → 无提示，ghost 永不错）；无则回退
  **fish 式历史 ghost**（`HistoryHinter`）。
- 非交互（管道 / no-tty）走 `plain_readline`：无补全、无 ghost、续行不预填缩进（输入自带文本）。

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

### repl-future-decl-capture-vars

- **来源**：add-repl-decls-multiline（Decision 5）
- **触发原因**：REPL 声明的函数/类型体内裸引用会话变量（`Vars{N}` 在另一 ns、需限定），而声明累积
  不对声明体做 Rewriter（保「零改写」）→ 自然 `E0401`
- **前置依赖**：会话变量→声明的注入机制（参数化 / 闭包捕获）设计
- **触发条件**：「REPL 里定义的函数想用之前定义的变量」成为高频诉求
- **当前 workaround**：把会话变量作为参数显式传入声明的函数

### repl-future-decl-supersede

- **来源**：add-repl-decls-multiline（Decision 4）
- **触发原因**：MVP 同名重定义报错（`DeclNames` 查重）；supersede（新定义遮蔽旧）需会话内符号版本化
  + 旧包退役（避免 first-wins 串味）
- **前置依赖**：会话内符号版本化 / 最新-ns-wins 解析（与 repl-future-incremental-compilation 的
  load-context supersede 模型同源）
- **触发条件**：交互式迭代重定义成为高频诉求
- **当前 workaround**：`.reset` 重开会话（`.reset` 本身亦 follow-up）

### repl-future-tab-completion（已解耦 LSP；作用域级 + obj.会话变量成员 + 类型名/ns 均已落地——只剩任意-expr receiver + LSP）

- **来源**：0.3.15 设计讨论
- **重新定位（2026-07-28 add-completion-query-api）**：原判「前置 0.5.x LSP」把耦合画粗了。补全
  与 IDE/LSP 是**同一语义内核 + 两个前端**，共享的是「补全查询 API」这一层、**不是整个 LSP**；REPL
  进程内 + live VM，不需 LSP 协议栈，可先于完整 LSP 落地。
- **已落地（Phase 1，作用域级）**：`Std.Scripting.Completer.replComplete` 读当前 `ScriptState`
  （`VarNames` + `DeclNames`）返回作用域候选；rustyline `Completer` 经 `__repl_set_completer` +
  thread-local `&VmContext` 回调（机制见 `corelib/repl.rs`，`complete_via_callback`）。前缀过滤 +
  大小写敏感 + 去重。**REPL 自持数据，不依赖 compiler。**
- **已落地（Phase 3b/3c，#59/#62）**：① `obj.` 成员补全——**会话变量**走 live 反射（读 `Vars{N}.x` 静态字段
  值 + `GetType().GetMembers()`，零副作用，`Repl.MemberNames` builtin，#59）；② 类型名 + `Type.` 静态成员 +
  ns 导出补全（`CachedScan` 查询，#62）。
- **未落地（后续阶段）**：① **任意 `expr.` receiver** 成员补全（非会话变量的表达式，需静态类型推断，defer）；
  ② **基元类型会话变量**的 `obj.` 成员（现只对堆对象反射，int/string 返回空，`repl.rs` refinement）；
  ③ **关键字 / 命名空间名**补全（现补全源不含语言关键字与 ns 本身）；④ LSP 客户端复用同一「补全查询 API」内核。
- **前置依赖**：①④ 依赖静态类型推断 / 「补全查询 API」内核（非整个 LSP）；②③ 无硬依赖、REPL 侧小改即可。

### repl-future-syntax-highlight（REPL 输入行 / 输出语法着色）

- **来源**：2026-07-28 User 需求（REPL console 文本语法颜色显示）——**明确暂缓、先记录**。
- **触发原因**：交互输入行与求值输出目前是单色纯文本；语法着色（关键字 / 字面量 / 类型 / 标点分色）
  提升可读性，与主流 REPL（IPython / node）对齐。
- **实现路径（已就位的钩子）**：rustyline 的 **`Highlighter` trait** 正是输入行着色的钩子——
  `ReplHelper`（`corelib/repl.rs`，add-completion-query-api 阶段 0 已建）当前用**默认空实现**；着色
  只需实现 `Highlighter::highlight(line, pos)`：用 `Z42.Syntax.Lexer` 对行 tokenize（Rewriter 已用
  同一 Lexer）→ 按 `TokenKind` 包 ANSI 色码返回。与补全共用同一 `ReplHelper` + 同一 Lexer，无新基建。
  **输出着色**（求值结果 / 错误）另在宿主 `_fmt` / 错误打印处按类型上色，独立小项。
- **前置依赖**：无（纯 runtime `Highlighter` + 宿主输出格式化）；可随时做。
- **当前 workaround**：单色纯文本。
- **注意**：非终端（管道 / 重定向）与 `NO_COLOR` 环境下须禁用色码（同 `.clear` 的 `IsTerminal()` 守卫）。

### repl-future-incremental-compilation

- **来源**：0.3.15 设计讨论（Growing Transcript 性能权衡）
- **触发原因**：Growing Transcript 是 O(n) 重编译；小 session 可接受，大 session 慢
- **前置依赖**：增量模块加载 + 跨模块静态状态共享 VM 能力 —— 即 [load-context.md](../runtime/load-context.md)（每轮输入 = 一个加载上下文，重定义 = 新版 supersede 旧版 + 旧版无引用时回收 `whyRetained`）+ [componentized-runtime.md](../runtime/componentized-runtime.md)（运行时编译器作为可加载组件）。该增量方案 = "每行 = 一个 context" 模型，其使能基建已在 2026-06-21 运行时设计弧中落定（DESIGN）。
- **触发条件**：session 规模成为实际性能瓶颈时（benchmark 驱动）
- **当前状态（2026-07-26 perf-optimize-repl-eval 大幅缓解）**：曾是每轮 ~3.5s 全量重编译
  （几乎不可用）。诊断发现瓶颈 = 每轮 `PackageCompile → DepScan` 在解释器上重解整个 stdlib+
  编译器世界（占 ~98%）。已落地 ②跨轮缓存 `DepScanResult`（首轮建一次，后续经
  `CompileInputs.CachedScan` 复用）+ ⑤仅 var 声明轮发新 `Vars` 类（非声明轮引用现有类、赋值就地
  改静态）→ 每表达式轮从 ~3500ms 降到 **~72ms 恒定**、抹平了原 O(n) 增长。剩余仅：(a) 首轮
  一次性 ~3.3s 静态 scan 构建（见 `repl-future-persist-static-scan`）、(b) 每轮 ~72ms 中约 50ms 是
  `ImportedSymbolLoader.Load` 遍历全导出集（可进一步缓存）。真·增量（每行=一个 context、旧版
  supersede）仍是这两项的终极解，但已非「不可用」级瓶颈。bench：`bench/repl/`。

### repl-future-persist-static-scan

- **来源**：perf-optimize-repl-eval ④（2026-07-26 profiling）
- **触发原因**：首轮 eval 的 ~3.3s 是一次性构建静态库 `DepScanResult`（解释器扫 ~30 个 stdlib
  zpkg + `TsigReconcile` 重建类型世界）。跨会话不变，可持久化到磁盘一次、后续会话 mmap/反序列化复用。
- **前置依赖**：`DepScanResult` 的序列化格式 —— 需序列化 `DependencyIndex`（StrMap of DepCallEntry）
  + `Exported`（深层嵌套 `ExportedModuleZ` 树）+ `Wp`（`ReconWorldPkg`）+ 失效校验（lib zpkg 集合
  hash / mtime）。工作量≈重实现半个 zpkg 读写面。
- **触发条件**：首轮 3.3s 成为实际痛点时（用户反馈 / 高频冷启动场景）。当前一次性成本，交互中常被
  用户敲第一行的耗时掩盖，故 ROI 暂低、defer。
- **当前 workaround（2026-07-29 add-repl-prewarm 落地）**：**后台线程预热**已把这份一次性成本从
  「用户回车后干等」移到「与用户打字并行」——REPL 启动即 spawn worker 跑 `Script.Prewarm` 构建
  `DepScanResult`，首次 `Eval` 前 Join 汇合（见本页「启动预热」节）。真实 PTY 下打字间隙 ≥ 预热时长
  时首轮近瞬时；piped/无间隙时退化为同步、不更差。
- **大幅缩减（2026-07-30 lazy-type-world 落地）**：`TsigReconcile.BuildWorld` 的**一次性全量 TYPE/SIGS
  解析**改为 `LazyReconWorld` 按包懒填——Rebuild 基类链遇祖先按命名空间路由只解析引用闭包（见本页
  「惰性类型世界」节）。实测 `1+2` 首轮解析 **14/34 包**（prelude + 默认 using 闭包）而非全部，且**不随
  标准库总量增长**（O(引用闭包) 而非 O(总量)）——根治「库越大首轮越慢」。自举字节不动点 5/5 gen1==gen2 证等价。
- **本 Deferred 仍成立**：懒填把成本降到「prelude + 默认 using + 引用闭包」的解析（`1+2` 的 14 包主要是
  4 个默认 using——不预加载它们 + 后台符号名字索引是正交 follow-up，见 `lazy-type-world` design 的 Deferred），
  且残余 O(包数) 的 `Open`(STRS) 头部扫描仍在；持久化到磁盘才能连这些也跨会话消除。诸项正交、可叠加。

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

### repl-future-eof-detection

- **来源**：add-z42-repl（宿主退出机制）
- **触发原因**：`Console.ReadLine()` 无法区分 EOF（Ctrl-D）与空行；`z42i` 当前仅靠 `.exit` / `.quit`（或
  `Repl.ReadLine` 返回 `null` 的原生 EOF）退出，宿主级 `Console.ReadLine` 场景收不到 EOF 信号。
- **前置依赖**：runtime builtin 暴露 EOF 信号（区分「读到 EOF」与「读到空行」）。
- **触发条件**：需要用 `Console.ReadLine()` 写 EOF 敏感的交互逻辑时。
- **当前 workaround**：`.exit` / `.quit` 元指令，或 `ReadLine` 原生 EOF→null。

### repl-future-runtime-version

- **来源**：add-repl-mvp-metacommands（`.version` 落地时发现）
- **触发原因**：`.version` 目前只打印 zbc/zpkg **格式**版本（`Script.FormatVersion`）；z42vm **运行时版本串**
  （build profile / features / target）未经 builtin 暴露给 z42，`.version` 无法显示。
- **前置依赖**：runtime builtin 暴露版本串（可复用 `z42vm --info` 的信息面）。
- **触发条件**：用户需在 REPL 内查看运行时版本 / bug 上报时。
- **当前 workaround**：`z42vm --info`（进程外）。
