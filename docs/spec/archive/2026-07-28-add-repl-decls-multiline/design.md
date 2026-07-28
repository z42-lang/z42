# Design: REPL 多行输入 + 声明累积

## Architecture

```
z42i (interactive_main.z42)
  └─ ReadBlock(">>> ", "... ")  ← 括号平衡多行（native __repl_readblock，已存在）
        │  一个完整输入块（可跨行的 fn/class/expr）
        ▼
  Script.Eval(state, input)
        │
        ├─ _classify(input) → { using | var 声明 | 顶层声明(fn/type) | 表达式/语句 }
        │
        ├─[顶层声明轮]  build prelude(ns=Repl.R{N}, usings + using 全部声明ns + [vars]using)
        │               + 原样附声明文本  →  PackageCompile → bytes
        │               → 报错? 报错(会话不推进) : ExtendWithPackage + LoadBytes
        │                 + DeclNames.add(name) + DeclNamespaces.add(Repl.R{N}) + Counter=N
        │               （不 Invoke——声明无返回值）
        │
        └─[其余轮]      不变（表达式/语句/var/using），但 prelude 现额外 `using` 全部声明 ns
```

关键：声明**不**包裹进任何壳类，**原样**成为 `Repl.R{N}` 的命名空间成员。跨轮可见性完全靠 `using Repl.R{N};`——自由函数经 `fix-imported-free-func-namespace`、类型经既有 `ImportedClassNs`，裸调/裸用都限定到声明所在 ns 正确解析（回归夹具 `src/tests/cross-zpkg/free_func_cross_pkg/` 证同机制）。

## Decisions

### Decision 1: 声明进 `Repl.R{N}` + `using`，不包裹壳类、不改写调用点
**问题：** 如何让第 N 轮声明的 fn/type 在第 M>N 轮可见？
**选项：**
- A — 包裹进一个 `Decls` 类 + 每轮把裸调重写成 `Decls.foo()`（`add-z42-repl` 旧 design D3 设想）。缺：需函数 Rewriter，脆、与类型不统一。
- B — 声明进本轮 ns `Repl.R{N}`，后续轮 `using Repl.R{N}`，裸调经编译器跨包解析。缺：**依赖 fix A**（否则自由函数裸调误限定到调用方 ns → undefined）。
**决定：** 选 **B**。fix A 已在 main → 前置满足；函数与类型走**同一** `using` 机制，零改写、零壳类，最简且与语言语义一致。这正是先做 A 的目的。

### Decision 2: 分类靠 token 头部、以 token2 区分 var/fn
**问题：** 如何把 `int add(int a)`（fn 声明）与 `add(1,2)`（调用）、`int x = 1`（var）区分？
**决定：** 复用 `Z42.Syntax.Lexer` 取头部 token（跳过可选前导修饰符 `public`/`private`/`protected`/`internal`/`static`/`abstract`/`virtual`/`override`/`sealed`/`extern`）：
- 首实义 token ∈ {`class`,`struct`,`record`,`interface`,`enum`} → 类型声明，名 = 其后标识符。
- 形 `<type> <ident> (`（type=标识符或内建类型 token；token2=`(`）→ 自由函数声明，名 = 该标识符。
- 形 `<type> <ident> =`（token2=`=`）→ var 声明（原路径）。
- 否则 → 表达式/语句（原路径）。

保守漏判（如泛型返回 `List<int> f()`）只退回表达式路径（自然报错），**不误判**纯表达式为声明（`add(1,2)` 是 `<ident> (`，只 2 token 到 `(`，不匹配 `<type><ident>(` 的 3-token 形）。

### Decision 3: 声明轮不 `Invoke`，但要 `LoadBytes` + `ExtendWithPackage`
**问题：** 声明轮无返回值，运行期要做什么？
**决定：** 编出的 `Repl.R{N}` 包必须：① `Engine.LoadBytes` 进 VM（否则后续轮调用其函数运行期 undefined）；② `DepScan.ExtendWithPackage` 并入 `CachedScan`（否则后续轮编译期 `using Repl.R{N}` 找不到符号）。**不** `Engine.Invoke`（无 `Eval{N}` 自由函数）。`EvalResult` = `Success=true, HasValue=false`。

### Decision 4: 重定义 ERROR（不 supersede）
**问题：** 同名重声明如何处理？
**决定（User 2026-07-27 锁定）：** MVP 报错、会话不推进。`ScriptState.DeclNames`（`List<string>`，充当集合）记已声明名；`Eval` 声明轮**编译前**查重，命中即 `EvalResult(Success=false, Error="...'X' already declared...")`。不覆盖、不遮蔽——避免「同名多 ns first-wins 串味」和运行期二义。supersede 留 Deferred。

### Decision 5: 声明体不捕获会话变量（Out of Scope）
**问题：** 声明的 fn 体裸引用会话变量 `x`（在 `Vars{K}` 另一 ns）怎么办？
**决定：** 本 change **不**对声明体做 Rewriter（保持 D1 的「零改写」）。声明体裸引用会话变量 → 自然 `E0401 undefined`，会话不推进（错误恢复既有）。列 Out-of-Scope + Deferred `repl-future-decl-capture-vars`。理由：捕获需把会话变量以参数/闭包注入声明，是独立设计面，不塞进 MVP。

### Decision 6: 多行对所有输入统一，不特判声明
**问题：** 多行是否只对声明开？
**决定：** `ReadBlock` 对**所有**输入统一做括号平衡续读——多行表达式（未闭合 `(`）、多行声明（未闭合 `{`）一视同仁。元指令单行无括号即时返回。分类在**整块**上做，多行 fn/class 整体到达 `_classify`。

### Decision 7: 实例方法——`ExtendWithPackage` 把增量包并入 world（compiler）
**问题（实施期实测）：** REPL 声明的类，构造 + 静态字段跨轮 OK，但**实例方法** `new C().m()` → `E0401: no method 'm' on 'C'`（加 `public` 无效）。
**根因：** `TsigReconcile._rebuildClass`（`z42.ir`）按 FQ 名在 **world**（`wp`）里 `_locate` 类自身 + base 链，再从 `wp[pkg].Sigs` 读实例方法。全量 `DepScan` 的 world 含所有包故 OK；但 `ExtendWithPackage`（perf ② 增量路径）复用的是**静态 world**（stdlib+compiler），**不含**刚 emit 的 REPL 包 → 类定位不到自身 → 导出 0 方法。
**决定：** `ExtendWithPackage` 在 `Rebuild` **前**把本包（`ReconWorldPkg(ReadModuleTypes(z), ReadModuleSigs(z))`）**并入 `scan.Wp`**（增量、persistent → 后续轮 base 链亦可跨声明解析）。定位恢复 → 实例方法从 SIGS 完整重建。纯内存、无格式 bump。实测 `new Adder().add(10)=10` ✅。

### Decision 8: enum——`TsigReconcile` 导出本地 enum（compiler，跨包能力）
**问题（实施期实测）：** REPL 声明 `enum Color{...}` 后跨轮用 → `E0401: undefined: Color`。
**根因：** `TsigReconcile._rebuildModule` **显式跳过本地 enum**（`if (Flags&32) continue`），恒只导出内建 `GCHandleType`——drop-tsig-expt 时代 reconcile-oracle 的遗留。P3 后 `Rebuild` 成了实际导入路径 → 本地 enum 从此对**任何**消费包不可见（不止 REPL）。
**决定：** `_rebuildModule` 对每个本地 enum（`Flags&32`）从 TYPE 段的 `EnumMemberNames`/`EnumMemberValues`（add-enum-type-metadata，zbc 1.22 已携带）重建 `ExportedEnumZ` 导出。消费侧 `ImportedSymbolLoader` 已就绪（`EnumTypeNames`/`EnumConsts` → `SymbolCollector` 并入与本地同表）。**无格式 bump**（数据已在 zbc）。这令**一般跨包 enum 导入**首次工作。**enum=long 常量语义不变**（`Color.Green` 恒 `long`，`Color c=` 本地亦非法——见 MemberResolver:30）。self-host 安全：z42c 源无 enum。实测 enum 声明 + 声明体内跨轮用 `pickColor()=1` ✅。

## Implementation Notes

- **prelude 构造（`Script.Eval`）**：在既有「accumulated usings + [vars] using Repl.R{VarsRound}」基础上，**新增**遍历 `state.DeclNamespaces` 追加 `using Repl.R{k};`——**所有轮**（声明/表达式/语句/var）都要加，使任何轮都能看见已声明的 fn/type。
- **声明轮 body**：`prelude + "\n" + 声明原文`（声明是命名空间成员，不再拼 `Eval{N}` 函数）。复用 `_compileBody` 需微调：它现固定拼 `public static object {efn}() { body }`；声明轮改为「prelude + 原文」直编。抽一个 `_compileDecl(state, n, prelude, declText)` 或给 `_compileBody` 加「裸源」模式。
- **`ParsedInput` 扩展**：加 `bool IsDecl; string DeclName;`（`IsVarDecl` 保留）。`_classify` 填充。
- **`ExtendWithPackage` 复用**：与 var 声明轮同调用 `DepScan.ExtendWithPackage(state.CachedScan, bytes, "repl_r{N}")`。
- **Counter 推进**：声明轮推进 `Counter`（占用轮号 N，保证 ns 唯一），但**不**动 `VarsRound`（声明非变量）。
- **修饰符 token**：实现时以 `Z42.Syntax.TokenKind` 实际枚举为准（`Public`/`Private`/`Static`/…）；若某修饰符无独立 TokenKind（关键字表），按 Lexer 实际产出调整跳过集。
- **文件行数**：`Script.z42` 现 223 行，加分类+声明分支后需盯 300 软限；若逼近，把分类逻辑抽到 `Classifier.z42`（独立文件）。**实现期按实际行数裁决**（软限触发才拆，拆为独立 refactor 心智但可同 change 内完成，因是新增代码组织）。

## Testing Strategy

- **驱动测试**（自动回归）：`src/toolchain/scripting/tests/repl_decls_multiline/driver.z42`——`using Std.Scripting;` 建一个 `ScriptState`，顺序 `Script.Eval` 驱动：
  1. `int square(int x) { return x * x; }` → 断言 `Success && !HasValue`
  2. `square(7)` → 断言值 `49`
  3. `class Point { public int x; public int y; }` + `var p = new Point(); p.x = 5; p.x` → `5`
  4. `int square(int x){return 0;}`（重定义）→ 断言 `!Success`，且 `square(7)` 仍 `49`
  5. 多行 fn（含换行）声明 → 调用求值
  打印结果，与 `expected_output.txt` 对比。经 warm-z42c 回路运行（`z42vm z42c.driver.zpkg -- ...` 组装 `Z42_LIBS`=scripting+z42c+stdlib）。
- **z42i 手动 smoke**：`z42 repl` 交互——多行粘贴 fn/class、跨轮调用、重定义报错、`.help`/`.exit` 不受多行影响。
- **不回归**：`Script.Eval` 既有路径（表达式/var carry-forward/using/语句/错误恢复）；`xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）全绿；z42c 自举不动点不受影响（本 change 不碰 z42c 源）。
- **GREEN 权威**：z42.scripting 是 compiler-consuming 库（bootstrap-seed 轴④），冷路径 GREEN 以 CI 为准；本地验 warm 回路 + z42i smoke。

## Deferred / Future Work

登记到 `docs/design/toolchain/repl.md` Deferred 段 + `docs/roadmap.md` Deferred Backlog Index：

### repl-future-decl-capture-vars
- **来源**：本 change（add-repl-decls-multiline）Decision 5
- **触发原因**：声明体捕获会话变量需把变量注入声明（参数化/闭包），是独立设计面
- **前置依赖**：会话变量→声明的注入机制设计
- **触发条件**：用户反馈「REPL 里定义的函数想用之前定义的变量」成为高频需求
- **当前 workaround**：把会话变量作为参数显式传入声明的函数

### repl-future-decl-supersede
- **来源**：本 change Decision 4
- **触发原因**：MVP 重定义报错；supersede（新定义遮蔽旧）需版本化 ns 解析 + 旧包退役
- **前置依赖**：会话内符号版本化 / 最新-ns-wins 解析
- **触发条件**：交互迭代式重定义成为高频诉求
- **当前 workaround**：`.reset` 重开会话（`.reset` 本身亦 follow-up）
