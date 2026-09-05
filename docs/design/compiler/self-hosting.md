# 编译器自举（self-hosting）— `src/compiler/` 架构

> 状态：🚧 进行中（0.3.x B 主线）｜起点：B0 [scaffold-z42c-selfhost](../../spec/changes/scaffold-z42c-selfhost/)（2026-06-07）
>
> 本文是 z42 自举编译器的**唯一权威架构文档**：布局、受限写法、构建解析、对账策略、CLI parity、1.0 切换。规划背景见 [`roadmap.md` 0.3.x](../../roadmap.md) + [`plan-0.3.x-three-streams`](../../spec/changes/plan-0.3.x-three-streams/proposal.md)。

## 目标

把 C# bootstrap 编译器（`src/compiler/`，7 个项目）**逐子系统用 z42 重写**到端到端 `build` 跑通 + 与 C# 实现 **byte-identical** 对账通过。目的：用最严苛的 dogfood 验证并改进语言机制与完整度，并提编译性能。

**核心边界**：0.3.x 期间 **default 编译器仍是 C#**；z42c-selfhost 两实现并存、逐字节对账，对既有 PR 零干扰。删除 C# bootstrap 留到 1.0。

## 目录布局

`src/compiler/` 独立顶级目录（与 `src/compiler/` C# bootstrap 平级），独立 workspace（与 `src/libraries/` stdlib 解耦）：

```
src/compiler/                          # 编译器 workspace（语义/编排/驱动 = 真编译器后端）
├── z42.workspace.toml          # members=["*"]，输出 artifacts/build/compiler/<pkg>/<profile>/
├── z42c.semantics/ → z42c.semantics.zpkg (lib)   镜像 z42.Semantics(TypeCheck+Codegen)
├── z42c.pipeline/  → z42c.pipeline.zpkg  (lib)   镜像 z42.Pipeline (编排)
└── z42c.driver/    → z42c.driver.zpkg    (exe)   镜像 z42.Driver   (= z42c 入口别名)

src/libraries/                         # stdlib workspace —— 也承载可移植编译器件
├── z42.ir/         → z42.ir.zpkg         (lib)   IR 模型 + zbc/zpkg 后端（收敛自旧 z42c.ir+z42c.project）
├── z42c.core/      → z42c.core.zpkg      (lib)   Z42.Core   (Span/Diagnostic/Features)
└── z42c.syntax/    → z42c.syntax.zpkg    (lib)   Z42.Syntax (Lexer+Parser+AST)
```

**可移植前端下沉（converge-z42-syntax-lib，route A 地基）**：`z42c.core` + `z42c.syntax` 是
**host-platform-independent 纯计算**（Lexer/Parser/AST/Token/Span/Diagnostic，无 syscall/native），
故从 `src/compiler/` 挪进 `src/libraries/`，成 z42c 编译器**与** scripting/playground/runtime 共享的
可移植库。**包名/命名空间不变**（仍 `z42c.*` / `Z42.Core` / `Z42.Syntax`）——它们**非** Std/z42.* 标准库
API 面，只是恰好与 stdlib 同处 build+ship。`z42c.semantics/pipeline/driver`（编译器后端）经**跨-workspace
dist 发现**解析它们（与 `z42.ir` / `z42.project` 同机制），冷启动由破环预建（见轴 ④）供给。

**目录名 == `[project].name` == zpkg basename**（如 `z42c.core`），与 stdlib 约定一致：member 逻辑名（WS001 / default-members）、`${member_name}` 模板、产物名三者重合，消除歧义。命名空间镜像 C#：`Z42.Core` / `Z42.Syntax` / `Z42.IR` / `Z42.Project` / `Z42.Semantics` / `Z42.Pipeline` / `Z42.Driver`。

**依赖图**（镜像 [`src/compiler/README.md`](../../../src/compiler/README.md) 邻接表）：

```
core ──(无依赖)        ir ──(无依赖)
syntax    ◄── core
project   ◄── ir
semantics ◄── core, syntax, ir
pipeline  ◄── core, syntax, semantics, ir, project
driver    ◄── pipeline, ir, core
```

> **目录名 vs 产物名**：byte-identical 目标是 **z42c 编译用户代码产出的 .zbc/.zpkg 字节** 与 C# z42c 一致，**不**要求 z42c 自身源码/内部名与 C# dll 相同。镜像命名纯为 1:1 可维护性。

## 受限写法约定（全子系统遵守）

用今天能用的语言子集写，**不**为自举强制提前半个 L3：

| 维度 | 用 | 不用（排期）|
|------|----|------|
| AST / IR 节点 | `class` 继承层级 + `virtual` / 抽象 `Visitor` 基类 dispatch | record + `match`（0.7.x）|
| 集合变换 | `for` / `foreach` + 显式累积 | LINQ（0.6.x）|
| 错误路径 | `throw` / `try-catch` + Exception 子类 | `Result<T,E>` + `?`（0.7.x）|
| 泛型 | 已落地 G1-G4 + 闭包核心 | 关联类型 G3a 等（按真卡点评估）|

**dogfood 缺口处理**：写到某处今天的 z42 子集**无法表达**（不是不优雅）→ 停下汇报 → 判定 L1/L2 可补 / 必须拉 L3 → L1/L2 当次 spec 实现；必须 L3 则 features.md 逐项评估是否为自举提前。**禁止在编译器代码里写 workaround**（[[feedback_dogfood_fill_gaps]]）。

**受限写法补充（实做中发现，均沿用 stdlib 既有模式，非 workaround）**：

| 发现 | C# 写法 | z42 受限写法 | 依据 |
|------|--------|-------------|------|
| **无 `enum` 关键字** | `enum DiagnosticSeverity { ... }` | `static class` + `int` 常量 | stdlib `SplitOptions` / `SeekOrigin` / `FileMode` |
| **无交错数组 `int[][]`** | `var remaps = new int[n][];` | 循环内按需重算（如 zpkg remap 借 `Intern` 幂等性）或平铺单数组+偏移 | ZpkgWriterZ._buildMods |
| **`new T[n]` 不能在实参位置** | `F(new string[1], ...)` | 先提升局部变量再传 | zpkg_tests |
| **`fn` / `module` 是保留字** | 参数/变量名随意 | 改名（fname / irm） | 多处 |
| **类字段不能带泛型参数**（`private List<X> f;` 的 `<X>` 被 parser 静默丢弃 → 取元素退化为无约束 `T`，无法调其方法）| `List<Diagnostic> _items` | **typed array + count**：`Diagnostic[] _items; int _count;` + 手动 `Grow()` | stdlib `TomlValue._arrayItems` / `Process._args`（typed array 元素访问会正确单态化）|
| ~~**`List<T>` 过度约束** `where T: IEquatable<T> + IComparable<T>`~~（**已解除**：stdlib-structure-batch 2026-09-03 去掉类级约束，对齐 C#）| `List<T>` 任意元素 | 历史规避：typed array（同上）。约束已放松，编译器代码可按需迁回 `List<T>`（仍受上一条「类字段不能带泛型参数」限制）| List.z42 头注释 |

> 这三条决定编译器内部**集合一律用 typed array + count 并行数组**（而非 `List<T>` 字段）。`enum` / 泛型字段 / List 约束放松若后续作为独立语言增强落地，编译器代码随之迁移。

## 构建与依赖解析

**构建**：`z42c build --workspace --release`（cwd=`src/compiler`）→ 拓扑序编译 7 子包 → `artifacts/build/compiler/<member>/<profile>/{dist,cache}/`。经 xtask：`./xtask build compiler`。

### z42c driver 自有 `build --workspace`（端口 C# orchestrator，z42c-build-workspace）

z42c.driver 实现了自己的 `build --workspace`（`Main._buildWorkspace` + `z42c.pipeline/WorkspaceBuild.z42`），替换 C# 编译器的硬前置。两个里程碑：

- **成员发现 + 拓扑序**（`WorkspaceBuild.Plan` / `DiscoverMembers` / `TopoOrder`）：`members=["*"]` 下 wsDir 每个「恰含一份 `*.z42.toml`」的子目录 = 成员；读各成员 `[project].name` + `[dependencies]`，仅保留指向 workspace 内成员的边；O(N²) 层式拓扑（就绪集按 name Ordinal 发射 = C# `TopologicalLayers` 层内 name-sort 同序）。受限子集：无交错数组 → flat 平行数组（`WsMembers.DepFlat/DepOff/DepLen`）；无 enum → 颜色用 bool/int。环 → 抛 `Exception`。
- **Milestone 1（显式 `--output-dir <flat>`）**：全成员产物落该 flat dir（= 各成员 libsDir，deps-first 解析兄弟）。产物 zpkg 与 C# `build --workspace` byte-identical（字节与输出目录无关）。注意 flat dir 须含外部 stdlib 依赖（调用方 seed `Z42_LIBS` 内容）。
- **Milestone 2（无 `--output-dir`，drop-in 替代 C# stdlib build）**：`WorkspaceBuild.PlanLayout` 按 `[workspace.build].output_dir` 模板（`PathTemplate.Expand`，`${project_name}`/`${profile}`/`${workspace_dir}`）展开 per-member 布局 → 各成员产物落各自 `<output_dir>/dist`（默认 `dist_dir=${output_dir}/dist`，镜像 C# `CentralizedBuildLayout.ResolveWorkspace`）。**兄弟解析扫全成员 dist 列表 + `Z42_LIBS`**（外部 stdlib，如 z42c.* → z42.core）——`DepScan.ScanDirs` 多目录合并后 prelude-first + Ordinal 排序（成员名唯一，无跨目录同名碰撞 → first-wins 确定）。镜像 C# `WorkspaceBuildOrchestrator` 的 `workspaceLibDirs`（成员 `EffectiveDistDir` 透传）。先建全部成员 dist（空目录）再拓扑序逐个 build，使后续成员 dist 在建本成员时已可被 `DepScan` 扫到（虽空）。
- **byte-identical 范围**：`--emit-zbc`（代码段）逐字节一致 C#；整包 zpkg 对 stdlib 有 ~1-3% pre-existing 差异（DEPS provider env-artifact / TSIG / IMPL），gate 只验 `--emit-zbc` + 功能正确，不追整包 byte-identical。

### z42c 构建 stdlib（replace-csharp-compiler S3：✅ 生产接管已落地 2026-06-22）

**S3 目标（已达成）**：`xtask build stdlib` 的产出端从 C# 换成 z42c。`_buildStdlib`（`scripts/build/xtask_stdlib.z42`）现序列为：**① C# 种子（`_csharpBuildStdlibSeed`，z42c.driver 运行期依赖）→ ② build z42c（C# driver 编 7 包）→ ③ run-libs 组装（C#-种子 stdlib + z42c 7 包，copy）→ ④ z42c 重编 stdlib（z42vm `--mode interp` 跑 z42c.driver.zpkg `build --workspace --release`，M2 per-member 覆盖 canonical 布局）→ ⑤ verify + flat view**。`_testCompilerStdlib`（S2.1）改调 `_csharpBuildStdlibSeed`（只要 C# 种子，不触发递归全接管）。

**full GREEN gate 全绿（z42c-built stdlib）**：C# 1571/1571（含 sidecar+multicast）+ vm goldens interp 169/jit 165 + cross-zpkg 2 + **stdlib 272/272（z42c-built）** + compiler 7/7 byte-identical + 17 z42c units + e2e。

**已验证可行**：
- z42c `build --workspace`（M2 per-member drop-in）编全 22 库到 `artifacts/build/libraries/<name>/<profile>/dist/`（与 C# 同布局）。
- z42c-built stdlib **全 272 stdlib [Test] 通过**（生产路径，非 test-runner bundled VM）。

**S3 dogfood 暴露并修复的 2 个 z42c bug（均已落地）**：
1. **fix-z42c-tsig-optional-params**：z42c 导出 TSIG 把**可选参数**（`Param.Default != null`，如 `MulticastAction.Invoke(T, bool=false)`）当必填 → `minArgCount = paramCount`，消费端跨包省略默认实参报 `E0402`。修 `ExportedTypeExtractor._requiredCount`（镜像 C# `SymbolCollector.BuildFuncSignature`）。`_fromImportedMethod` 继承默认参数方法 re-export 暂留全必填（Deferred `self-hosting-future-inherited-optional-param-arity`）。
2. **fix-z42c-generic-ctor-arity**：z42c `new C<T>()` 对 **arity-overloaded 同名类**（arity-0 base `MulticastException` shadow 着 arity-1 `MulticastException<TResult>`，泛型版注册键 `MulticastException$1`）误解析为 arity-0 base → `MulticastFunc.Invoke` 实际抛非泛型 base，消费端 `catch (MulticastException<int>)` 失配 → 异常逃逸（`multicast_func/predicate_aggregate` golden 失败）。修 `SymbolTable.ResolveTypeP`：泛型实例化优先取 `Name$N`（非 overloaded 泛型如 `Box<T>` 注册 bare，回退不受影响）。

z42c 自身 7 包不用这些写法 → 旧 byte-identical 门（仅 z42c 自身 7 包 + 忽略 BLID）测不到，整 stdlib build dogfood 才暴露——正是 dogfood 的价值。

**S3 dogfood 暴露并修复的 #3+#4 bug（均已落地）**：
3. **port-z42c-strip-symbols**：z42c 此前 release 只 emit 内联 DBUG、无 split。已端口 strip-symbols（`ZpkgWriter.WritePackedWithSidecar`）：主 zpkg MODS dbug_len=0 + 末尾 BLID（BLAKE3-128，via z42.crypto Blake3）+ `.zsym` sidecar（META+STRS(symPool)+MDBG+BLID）。z42c-built z42c.driver 现 **FULLY byte-identical to C#（含 BLID）**；`StdlibSidecarPairingTests` 过。
4. **fix-blake3-multichunk-root-flag**：strip 的 build_id 暴露 z42.crypto Blake3 **多块（>1024B 树）哈希错误**——中间 parent 误标 ROOT + CV 取值二次 feed-forward。修 `_parentCV`/`_chunkCV`。影响**所有 >1024 字节 BLAKE3**（文件哈希等），小向量测不到。修后 z42c build_id == C# nuget 逐字节一致。

**S3 dogfood 暴露并修复的 #5 bug（已落地）**：
5. **fix-z42c-native-named-entry**：z42c `IrGen._nativeIntrinsic` 只识别 positional `[Native("__name")]`（`Args[0] is StringLitExpr`），对 named 短形 `[Native(lib = "L", entry = "E")]`（named args = AssignExpr，z42.compression 8 文件全用）返 "" → 不发 builtin 桩 → `_CompressRaw` 等 extern 在 zpkg 无函数体 → 运行时 undefined。修：扫 named args 取 `entry=` 值发 BuiltinInstr（镜像 C# EmitNativeStub 短形 `TypeName==null → BuiltinInstr(entry)`）。compression 18→10 修。

**S3 dogfood 暴露并修复的 #6 bug（已落地）**：
6. **fix-z42c-static-call-cross-ns**：z42c 静态调用 `Class.m()` 的 OwnerClass 用 `Qualify`（当前 ns）而非 `QualifyClass`（imported/同包跨-ns aware，查 ImportedClassNs）→ `Std.Archive.Zip.Write` 调 `Deflate.Compress`（`Std.Compression`）被误发 `Std.Archive.Deflate.Compress$1` → undefined。修 `CallEmitter._emitCall` 用 QualifyClass（镜像 C# QualifyClassName）。compression 8→4。

**S3 dogfood 暴露并修复的 #7 bug（已落地）**：
7. **fix-z42c-static-field-assign**：z42c `_emitAssign` 无 `BoundStaticGet` LHS 分支 → `Class.staticField = v`（静态字段写）静默丢弃 → 静态字段 mutation 不持久（`Log.SetMinLevel(3)` 后 `GetMinLevel()` 仍返默认 2，3× DiagnosticsFilter/LogFormat）。修：加 `StaticSetInstr` 分支。z42c 自身包静态字段多为只读，mutation 路径未测到。

**S3 dogfood 暴露并修复的 #8 bug（已落地，本次收官）**：
8. **fix-z42c-ctor-this-delegation**：z42c 把构造器 `: this(args)` 委托（同类 ctor 链）误编为 `: base(args)`（基类 ctor 调用）——`Parser._parseMethodTail` 消费 `this`/`base` 关键字后**丢弃区分**，`TypeChecker._bindMethodBody` 一律解析 `curCls.BaseName` 的 ctor。`new C()`（默认 ctor 委托 `: this(W,S)`）→ 调不存在的基类（Object）N-arg ctor → 运行期 `VCall: expected object, got Null` / 字段保持默认（`Bencher() : this(10,100)` → warmup+samples=0 → `test_bencher_default_runs_warmup_plus_samples` 断言 `expected 110 but got 0`）。修：`MethodDecl` 加 `IsThisInit` 位 → parser 记录 this/base → TypeChecker 按位选 TargetCls（this→curCls，base→baseCls）。镜像 C# `Ast.BaseCtorArgs`/`ThisCtorArgs` 互斥 + `FunctionEmitter` 两分支。回归单测 `codegen_tests.test_ctor_this_delegation`。

> **「blake3 多块 codegen bug」实为测试 golden 误写**（fix-blake3-multichunk-root-flag 的回归测试 golden 一度写错）——已改正，z42c-built blake3 多块哈希 == C#-built，无 codegen bug。
> **铁律**：S3 不删 C#；C# 是种子。删 C# 见 S5（须先 S4 committed/下载种子）。

**workspace 兄弟包解析（dogfood #1 根因修复，2026-06-07）**：

- **问题**：C# 编译器的 `BuildLibsDirs` 曾把 `artifacts/build/libraries/` 硬编码为唯一会扫描的 workspace 布局根。stdlib 兄弟依赖能解析纯因 stdlib 恰好输出到那里；z42c 输出到 `artifacts/build/z42c/`，兄弟包扫不到。
- **修复**：`WorkspaceBuildOrchestrator` 收集**本 workspace** 全体成员的 `EffectiveDistDir`（排序去重）→ 透传 `CompileMember` → `RunResolved` → `BuildTarget` → `BuildLibsDirs`，在既有扫描后**按规范化 full-path 去重追加**。
- **效果**：成员从**当前 workspace** 解析其 **toml 声明的**兄弟依赖（`declaredDeps` 过滤未声明项），与输出位置无关。stdlib 等已落在被扫描根的 workspace 去重后**零新增、顺序不变 → 零字节漂移**；单工程构建（`workspaceLibDirs=null`）行为不变。
- **规则**：除 stdlib（toolchain 自带、自动可用）外，**其他依赖必须在 toml `[dependencies]` 声明**才可解析。远程 / 下载依赖（registry / git URL）暂不支持——见 [Deferred](#deferred--future-work)。

详见 [compiler-architecture.md](compiler-architecture.md) 对应段。

### 运行 / 测试 z42c（单一 flat libs 目录 — 重要）

> **已部分超越（simplify-compiler-build, 2026-07-05）**：`z42c build` 编 exe 时现自动把非
> stdlib 依赖复制进输出 dist（.NET 式自包含），故 `z42c.driver` dist 已**自带** 6 个
> `z42c.*` 兄弟包——跑它时 `Z42_LIBS` 只需 stdlib，z42vm 从 driver 自身目录解析兄弟包。
> 下面「合并 z42c+stdlib 到 alllibs」的手工步骤只对**非自包含**旧产物需要；当前机制见
> [`docs/book/src/dev/build.md`](../../book/src/dev/build.md)。

**运行期 `Z42_LIBS` 是单个目录（非 colon-list），且必须含全部依赖 zpkg。** 跑 z42c 产物
（driver / 测试）时，先把「z42c 7 包 + stdlib」**合并到一个 flat 目录**（`xtask test
compiler` 自动组装 `artifacts/build/compiler/alllibs/<profile>/`），再 `Z42_LIBS=<该目录>`：

```
Z42_PORTABLE_VM=<z42vm> Z42_LIBS=<flat 含 z42c.*+z42.*> z42vm z42c.driver.zpkg
```

> **踩坑记录（2026-06-07）**：误把 `Z42_LIBS` 当成 colon-list 传多个目录 → 运行期当成单个
> 非法路径 → 回退到仅 stdlib → z42c 包未加载 → `VCall: function not found` + 静态字段读
> null。一度误判为 VM/编译器跨包 bug，实为 harness 配置。**跨包方法调用与静态字段在运行
> 时完全正常**（driver 端到端 + z42c.core 7/7 单测验证），7 包架构运行时成立。

### 测试通道（z42c [Test]）

测试单元布局：`src/compiler/<member>/tests/<unit>/{<name>.z42.toml(kind=lib) + *.z42}`。
`xtask test compiler` = build 7 子包 smoke + 组装 flat 目录 + 逐单元 `z42c build <toml>
--release`（Z42_LIBS=flat）+ z42b（z42.builder.zpkg，Z42_LIBS=flat）经 z42vm 运行。
`z42.test` 是 stdlib 自动可用（**不**在 toml 声明，避免 WS013）。z42c-selfhost 仍为
opt-in soak，不入默认 GREEN gate。

## CLI parity（无桥接）

z42c.driver 只 ship 已就绪命令，**绝不** fallback 到 dotnet z42c.dll。逐子版本解锁：

| 起始 | 命令 |
|------|------|
| B0 | 仅 banner（无命令）|
| 已落地 | `--dump-tokens` / `--dump-ast` / `--dump-bound` / **`--emit-zbc <src> <out>`**（首个产物命令）|
| 已落地 | **`build <toml>`**（packed zpkg 端到端：源发现→编译→组装→写出；e2e：产物 z42vm 直跑。TSIG/IMPL 待 follow-up → 全段 byte-identical）|
| 0.3.10 | `build` 产物与 C# byte-identical |

## .zbc 写入器（z42c.ir/BinaryFormat/，port-z42c-zbc-writer 已落地）

自举 ZbcWriter 镜像 C# `z42.IR/BinaryFormat/`，**功能完整**（const[int/f64 IEEE754/bool/null/str]/算术/比较/位/一元/控制流/调用 token/对象/字段/数组/is·as/字符串拼接 → 全 8-section .zbc，z42vm 直接执行）：

- **架构**：`ByteWriter`（int[] 0..255 + LE 助手，规避 byte 型）→ `ZbcInstr`（集中 if-is 编码）→ `ZbcWriter`（intern 预扫 + 8-section 组装）；`TokenAllocator`（插入序 index；跨模块 `ImportBase(1<<31)|STRS idx`）；`ZbcStringPool`（插入序 = STRS 字节序）。
- **确定性铁律**：字符串池 intern 序须 1:1 复刻 C# `InternPoolStrings`（模块名 → const.str 池 → 类 → "?" → 每函数[名/ret/param → 每块 label→指令串]）；IMPT 写前 Ordinal 排序。
- **验证双轨**：① `empty` fixture 逐字节对账（版本随 C# bump 同步，见 version-bumping.md 第 5 步）；② **e2e 执行验证**（xtask `test compiler` 的 e2e 步：自检程序经 `z42c --emit-zbc` → z42vm 执行，div-by-zero oracle + 负向用例）。
- **受限/缺口**：runtime builtin 直接 `[Native("__double_to_bits")]` 自声明（免 stdlib 改动）；ctor-less 类合成 `"Class.Class"` ctor token（VM 查无即跳）；char 字面量整链已落地（port-z42c-char）；**异常 try/catch/throw 整链已落地**（port-z42c-try）；**interface 类型+分派已落地**（port-z42c-interface）；**闭包/lambda 整链已落地**（port-z42c-closures：lambda 解析/BindLambda+捕获分析/lift+LoadFn·MkClos·CallIndirect；closcheck 对账，zpkg 5/5——**三大件 interface·异常·闭包全数收官**）；**跨 zpkg `impl Trait for Type` 已落地**（port-z42c-impl-block：parser ImplDecl + 同包 binder（collector 合并/typecheck 绑体/IrGen 发 `<qualTarget>.<m>`）+ IMPL 段 emit（5 组件，布局 1:1 C# BuildImplSection，结构 byte-identical）+ 消费端 _readImpl + _mergeImpl（trait 方法并入 imported target）；端到端 cross-zpkg/impl_propagation 经 z42c 建+跑 → "hi from R2"，C# 测试 2/2 不回归）；**DBUG 已落地**（add-z42c-source-spans：AST/Bound 全节点携 Span → TrackLine[每语句,同行去重,file basename] → IrFunction.LineTable/LocalVarTable[RegId 序] → DBUG 第 9 section + HasDebug flag）。

## 开发迭代验证流程（staged bootstrap + 不变量）

把「下载种子 → 编 xtask → 驱动编项目 → 测试 → 不动点」串成一张端到端图，标注每步守哪条
不变量。**纪律**见 [`.claude/rules/bootstrap-seed.md`](../../../.claude/rules/bootstrap-seed.md)；本节是总览，
下文 byte-identical 自举闭环是其细节。

### 依赖图：环在哪

```
z42vm (Rust)  ──cargo 建，非自举──►  锚点（永远能产「懂当前格式」的 VM）
z42c / stdlib / xtask (z42)  ──互为前置──►  ★ 自举环 ★
```

**环的本质**：用 vN-1 的工具编 vN 的源，而 vN 源可能用了 vN-1 工具不懂的东西。拆成三轴：

| 轴 | 鸡蛋问题 | 怎么断 |
|---|---|---|
| **① 语法** | vN 源用新语法 → vN-1 z42c 编不了 | **纪律**（support/use 隔一 release，bootstrap-seed.md）|
| **② zbc/zpkg 格式** | vN-1 z42vm 读不了 vN 格式 | **锚点自动断**：z42vm 是 Rust 建的，新格式产物跑新建 vm；旧 z42c 产中间件用旧格式跑旧 vm，再 re-stage 产新格式 |
| **③ stdlib API** | xtask/源用新 API → 旧 stdlib 没有 | **纪律**（同 ①）|
| **④ z42c 自依赖的共享库**（converge-z42c-ir-metadata / converge-z42-syntax-lib） | z42c 运行期/编译期依赖 `z42.ir` + `z42c.core` + `z42c.syntax`（均落 stdlib workspace），但它们**由 z42c 构建** → 冷启动 flat dist 里还没有它们 | **破环预建（种子先编这些库进 build-libs）**，见下 |

> 关键：**格式轴不需纪律**——z42vm 不自举（Rust 建）是打破格式环的锚点。真正靠纪律约束的只有
> 语法/API 轴。

**轴 ④ 的破环细节**（`_ensureBootstrapSelfDepLibs`，`scripts/build/xtask_compiler.z42`）：z42c 把
IR 模型 + zbc/zpkg 后端下沉到 stdlib 单库 `z42.ir`（收敛自旧 `z42c.ir` + `z42c.project`），于是
z42c **运行期依赖 `z42.ir`**——它建任何 zpkg 都要调 `Z42.Project.ZpkgBuilder.Sha256Hex` 等。冷启动
（fresh checkout / CI 新 runner）flat dist 里没有 `z42.ir`，而上一 nightly 种子只把等价代码作
**`z42c.ir` + `z42c.project`** 两个包携带（包名不同）。若直接建 z42c，编译器只能拿种子的
`z42c.ir/z42c.project` 作 `Z42.IR/Z42.Project` 的**命名空间兜底**来解析 → fresh z42c emit 的
`ZpkgBuilder.Sha256Hex` 调用钉在种子包上，运行期加载真正的 `z42.ir` 时**解析不到**
（`undefined function ...Sha256Hex`，即 main CI 冷启动全红根因）。破环：`_buildCompilerViaZ42c`
在 workspace build **前**先用当前 driver（冷启动=上一 nightly 种子，自带等价 `ZpkgBuilder`）把
当前源码的 `z42.ir` **单独编进 build-libs**（`build <toml> --output-dir <flat>`），fresh z42c 就
对着**真 `z42.ir`** 编译+运行，一致。**幂等**：warm 树（`z42.ir` 已在 flat dist）直接跳过——故
warm 构建与 byte-identical 不动点**逐字节不受影响**；随后的 `build stdlib` 全量 workspace 构建
用 fresh z42c 把 `z42.ir` 覆盖为规范产物。与轴 ② 的两代自举同构，但触发条件是**包结构收敛**而非
格式 bump。

> **A1 扩展（consolidate-core-intrinsics，2026-08-03）**：`z42.ir` 现调 `Std.BitConverter`（当前源
> `z42.core` 新增门面，位转换 intrinsic 单一声明点）。冷/首暖构建时 flat 里躺的是种子/上一次的旧
> `z42.core`（缺 `BitConverter`），若直接单包编 `z42.ir` → `undefined: BitConverter`。故
> `_ensureBootstrapSelfDepLibs` 在建 `z42.ir` **前**，用同款「先预建覆盖种子」把**当前源 `z42.core`** 也编进
> build-libs——即：凡 z42c 运行期自依赖链上、且被当前源新引用了新 API 的库（此处 `z42.core`），都须
> 先于其消费者（`z42.ir`）进 flat。这不是格式/包结构问题，而是**轴 ④ 在「stdlib 库新增 API」维度的
> 同一破环**：z42c 运行期自依赖库的新 API，必须在自建前就存在于 flat。实测：老 core 编 `z42.ir` 复现
> `undefined: BitConverter`，预建当前源 core 后通过。

> **前端下沉扩展（converge-z42-syntax-lib，route A 地基）**：`z42c.core` + `z42c.syntax`（可移植前端）
> 挪入 `src/libraries/` 后，`z42c.semantics/pipeline/driver` 对它们成**跨-workspace 共享库依赖**——正是
> 轴 ④ 的又一实例（编译器后端自依赖一组由 z42c 自己构建的库）。冷启动 flat 里躺的是上一 nightly 种子
> 的旧 `z42c.core/syntax`；`_ensureBootstrapSelfDepLibs` 在 `z42.ir` 之后**追加**预建**当前源**
> `z42c.core` → `z42c.syntax`（顺序：core 靠 `z42.core` 隐式 prelude、syntax 靠 `z42c.core`），
> 覆盖种子旧版，让 fresh z42c 永远对着**当前源**前端编译+运行。**与 z42.ir 同款「不 warm-skip」**：源
> 未变时增量缓存近零成本；源变了本就该重建。两代自举 CI 路径（`ci-bootstrap`）天然覆盖——它每代先
> `build --workspace`（stdlib，现含 z42c.core/syntax）再建 `src/compiler`，前端先于后端进 flat。

### 分阶段流程（每阶段守哪条不变量）

```
Stage 0  种子 = 上一个 pinned nightly（z42vm₍ₙ₋₁₎ + z42c₍ₙ₋₁₎ + stdlib₍ₙ₋₁₎）或本地 warm 产物

Stage 1  锚点 + 用种子建新工具（只用种子能力）
  cargo            → z42vm_N                         （Rust，独立，无环）
  z42c₍ₙ₋₁₎ 编 xtask 源  → xtask_N                   ◄── INV-1：上版必须能编 xtask（最受约束）
  xtask_N 驱动：
    z42c₍ₙ₋₁₎ 编 z42c 源  → z42c_N⁽¹⁾               ◄── INV-1：上版必须能编 z42c 源
    z42c_N⁽¹⁾ 编 stdlib 源 → stdlib_N

Stage 2  自举不动点 + 全量验证
  z42c_N⁽¹⁾ 再编 z42c 源 → z42c_N⁽²⁾（新格式，跑 z42vm_N）
  z42c_N⁽²⁾ 再编一次     → z42c_N⁽³⁾；assert ⁽²⁾==⁽³⁾ 逐字节   ◄── INV-3：byte-identical
  xtask_N 驱动全量编译 + 跑 [Test]/[Benchmark]                 ◄── INV-2：测试全绿
```

**为什么 xtask 最受约束**：xtask 是 Stage 1 **最先**被种子编出来的（它还要回头驱动编 stdlib/z42c），
所以它只能用种子（上版 SDK）已有的语法 + stdlib API——「上版必须能编 xtask」(INV-1) 是绑定不变量，
xtask 绝不能依赖本提交新加的语法/stdlib API。

### CI 三道门

| 门 | 干什么 | 守 |
|---|---|---|
| **A. forward-bootstrap**（`bootstrap-no-csharp` job）| 下载上一 nightly 种子 → 重建全栈 → 跑测试 | INV-1 + INV-2 |
| **B. self-host fixpoint** | 新工具重建自己，byte-identical 7 日零漂移 | INV-3（详见下节）|
| **C. 本地快门**（`xtask test bootstrap`）| 下载 nightly z42c 编当前 z42c 源，越界立即红 | INV-1（改 parser/codegen/格式后必跑）|

## byte-identical 对账（0.3.x 退出标准）

每个 `z42c.<sub>.zpkg` 的产物（token stream JSON / AST JSON / manifest 解析 / .zbc / .zpkg 字节）与对应 C# `z42.<Sub>.dll` 产物逐字节对账。全 7 子系统 7 日零飘移 = B 主线达标。

- **现状（2026-06-10）**：**`.zpkg` 包级对账已上线 FULL-FILE byte-identical**——gate 的 zpkg byte-compare 步把 **4 个工程**（调用/对象/字段 exe + namespaced 类 + stdlib-using hello（跨包 import 消费链）+ **textapp（实例跨包链：imported 类 VCall+依赖追踪 / prim receiver→DepIndex FQ Call / prim 包装类 typecheck）**）经 `z42c build` 与 C# CLI（--strip-symbols=false）逐字节 diff（九段含 TSIG/IMPL）。TSIG 内建面（Object 四方法前置/11 接口/GCHandleType/11 委托）以静态表镜像 C# prelude 注入（ExportedTypeExtractor）；五条校准字节真相 + prelude 漂移防护见 archive/2026-06-10-port-z42c-tsig tasks。`.zbc` 对账 **per-construct byte-identical**——`xtask test compiler` 的 e2e byte-compare 步把 3 个真实程序（算术/控制流、调用/对象/字段/字符串、浮点/is·as/继承）经 z42c 与 C# 同源编译逐字节 diff（含 DBUG）。已知 lowering parity 约定：int 字面量恒 ConstI64 / str `+`=Add(tag str) / while 块 label cond_·body_·end_ / 无基类→Std.Object / 函数序=类方法→自由函数 / var-decl=专用寄存器+copy（详见 archive/2026-06-10-add-z42c-source-spans tasks 实施记录）。
- **现状（2026-06-16）= 功能性自举里程碑**：`z42c build` **编译自己全部 7 个源包**（z42c.{core,ir,syntax,project,semantics,pipeline,driver}）**无错**。途中在 z42c 内填补 8 个语言/语义缺口（镜像 C#，非 workaround，change `port-z42c-self-compile`）：用户类数组局部声明 `T[] x` / string 属性访问 `s.Length`（prim receiver→stdlib 包装类）/ **C 风格强制转换 `(int)x`**（CastExpr→BoundConvert→ConvertInstr op 0xB1，镜像 C# VisitCast no-op 规则）/ 整型字面量 radix 解析 `0xFF`·`0b…`（`ZbcInstr._parseIntLit`）/ 类型别名 `byte≡u8`·nullable `string?`·数组元素 unknown 吸收（`Z42Type.Canon` + Array/Prim 可赋性）/ 无初始化器声明 `T name;`（codegen 不发 IR，首赋值绑寄存器）。**下一级 = 逐包 byte-identical 对账**（需 workspace 成员默认 `[sources]` + 跨包 namespace/版本上下文对齐）。
- **现状（2026-06-18）= 🎉 z42c 自身 7/7 包全 byte-identical = 自举逐字节对账完成**：dual-build（z42c-self vs C# CLI，绝对 toml 路径消除 DBUG 源路径串差）逐包对账 7 个自身源包 —— **core FULL-FILE 逐字节一致**；`syntax/ir/project/pipeline/driver/semantics` 全段 delta 0、disasm **0 非-env diff**，仅剩 STRS −1 = **DEPS provider env-artifact**（同一 namespace 由哪个 stdlib zpkg 提供取决于环境枚举序，`z42.core.zpkg` vs `z42.cli.zpkg` 差 1 字节，非 codegen bug）。落地的 parity 根因：① 整型字面量越界 → long（`0xFFFFFFFFL`，`TypeChecker._bindExpr`，镜像 C# LitIntExpr）；② 无 init 局部 `T m;` 首赋值 → **专用寄存器 + copy**（`_emitAssign`，镜像 C# `WriteBackName`）；③ cast-to-class receiver 的 field/method 结果 → Unknown，结构化沿 cast→field→call 链传播（`_castUnknownChain`，更正 G18i 过窄跨-ns 门，顺带清掉 ir 的 4 处残差）；④ var-decl-with-init 寄存器 tag = init 表达式类型；⑤ **STRS 非 ASCII 串按 UTF-8 字节写入**（`ByteWriter.WriteUtf8Bytes`，原每 scalar 写 1 字节与 `ByteLength` 错位 → driver banner em-dash 串损坏）；⑥ **重载 arity-mangling**（`TypeChecker._overloadKey` arity-first，修 `mods.Split(" ")`→`Split$1`）；⑦ **修 C# bug：方法签名返回类型骨架升级**（最后一处 = semantics WithClassGeneric——`TypeEnv.Root(s).WithClassGeneric(...)` C# 原因方法返回类型解析为无方法 stub class 而发 Unknown，z42c 正确发 ref；最小复现 `E.Root().With(5)` C# 直接报错拒绝合法代码。User 裁决「修 C# bug 让两边一致」→ `SymbolCollector.TypeFixup.UpgradeType` 加 `Z42ClassType` 骨架升级分支[接口不升级，避免剥 TypeArgs]，C# 现正确解析 → 与 z42c 一致。dotnet test 1568/1568 + 7 包全 byte-identical）。
- **不纳入默认 GREEN gate**：z42c-selfhost 是 0.3.x 期间 opt-in soak（`./xtask test compiler`），既有 `./xtask test` 不含它。

## C#-free 自举闭环（replace-csharp S4，2026-06-22）

**目标**：fresh checkout / CI / dev 用**预建 z42c 种子**重建 z42c，全程**无 C#（无 dotnet）**——删 C#（S5）的唯一前置（铁律）。User 决策：种子**从 nightly 下载**（不 commit 进仓库）。

**不动点已证**：z42c-built `z42c.driver.zpkg` 与 C#-built **逐字节一致**；用 z42c-built 种子重建 z42c，7/7 包逐字节一致（含 BLID，确定性）。即 z42c（z42 写的编译器）忠实编译自身。

**C#-free bootstrap 序**（`scripts/selfhost-bootstrap.sh`，z42vm only）：
1. `cargo build z42vm`（Rust，非 C#）。
2. 种子 z42c 编 stdlib（源，`build --workspace --output-dir`）→ fresh stdlib。
3. 种子 z42c **单 toml 逐成员**编 z42c（源，runlibs=fresh-stdlib+种子 z42c，累积 fresh siblings）→ fresh z42c。（`build --workspace` 自建 z42c 有 E0402 wrinkle → 单 toml 拓扑绕过，待独立 change 根因修。）
4. fresh z42c 编 xtask.zpkg。
5. 不动点检查：rebuilt z42c == 种子（cmp + 16B BLID 尾容差）。

**种子分发**（2026-07-01 修订）：种子随 **SDK package**（`z42-sdk-<ver>-<rid>`）的 `programs/z42c/`（z42c-written z42c.* 7 zpkg，apphost 布局，`_packageDesktop`）分发，而非 runtime package——z42c 是 host 平台工具，runtime package 定位为纯嵌入式运行时（`native/` + `libs/`，见 `_buildRuntimePackage`），可能被跨 host 使用（如 android runtime 装在 Windows/macOS host 上），塞一个单一 host 平台意义的 z42c 没有意义。CI job `bootstrap-no-csharp (linux-x64)`：**无 setup-dotnet**（dotnet 缺席 = 结构性无 C#）→ `gh release download nightly z42-sdk-nightly-<rid>.tar.gz` → 取 `programs/z42c/*.zpkg` + `libs/*.zpkg` 组装 seed → 跑脚本。

**鸡蛋滚动**：种子由 source-bootstrapped publish-nightly（用 C# 造）产 —— S4 期允许（C# 造种子，下游重建不用 C#）。新 nightly 携 z42c/ 前，bootstrap-no-csharp 在旧 nightly transient 失败（明确 error）→ publish-nightly republish 后自愈。S5 删 C# 后 publish-nightly 自身改用前夜种子（自持闭环，S5 收尾）；生产 `xtask build stdlib`（flip `_buildStdlib`）的 C# 种子步亦移交 S5 脱 C#。

## C# 彻底移除（replace-csharp S5，✅ 完成 2026-06-26）

S5 走完 Phase A（切构建站点）→ B（gate 切 z42c 不动点）→ C（删 C# + 清 dotnet）。**硬前置**是
z42c 达到 golden 编译 parity（编通全部 ~333 golden，含 reflection/closure 等边缘特性）—— 于 commit
`da0d547b` 全清。随后：

- **删 C# 源**：`src/compiler/`（旧 280 .cs C# 编译器）+ `z42.Tests` + `z42.slnx` 全删；`src/z42c/`
  重命名为 `src/compiler/`（产物路径 2026-07-05 起亦为 `artifacts/build/compiler/`，镜像源码目录、与 `libraries/` 一致）。仓库无 `.cs`/`.slnx`。
- **清 dotnet**：全部 5 个 workflow（ci / bench-pr / bench-update / release）无 `setup-dotnet` /
  `dotnet-version` / 真实 `dotnet build|test|run`；保留一处 C#-free guard stub（PATH 注入假 `dotnet` →
  被调即 `exit 97`），结构性保证「无 C#」。
- **cold-start 种子**：CI fresh runner 不再 C# 现编 z42c，改 `gh release download nightly` 解包 z42c/
  种子（4 平台 runtime 包均携带）。warm 路径 z42c 自建。
- **闭环验证**：stdlib / bench / build-and-test 全 4 OS C#-free（commit 9d489854 / b36da336 /
  17e342fc / 4dc2896b / 08ef874d）+ bootstrap-no-csharp fixpoint + cross-zpkg + jit-consistency 多次绿 run。

> 操作层流程（SDK/Current 两套 toolchain、共享 host SDK、边界不变量、CI 冗余清单）见
> [`docs/workflow/testing/bootstrap.md`](../../workflow/testing/bootstrap.md)。后续 CI 去冗余
> （compile-once：编一次全下游复用 + fixpoint gate 发布 + format-bump 兜底）规划见
> `docs/spec/changes/compile-once-toolchain/`。

## compile-perf gate

最终目标：z42c-selfhost（z42-JIT）编译同一 corpus wall time ≤ 3× dotnet z42c.dll（median ≤ 3.0 / P99 ≤ 5.0）。0.3.3–0.3.9 铺设期 per-subsystem micro-bench 入 bench-baselines、不设硬阈值；0.3.10 起 end-to-end gate 启用。

## 1.0 切换路径

0.3.x 完成全自举 + byte-identical + **C# 彻底移除（S5，2026-06-26 ✅）** 后，1.0 仅剩 launcher 内置 `z42c` 短命令指向 `z42c.driver.zpkg` + 跨架构 NativeAOT + SemVer 启用。（旧 `git rm -r src/compiler/` C# 删除已于 S5 完成。）

## Deferred / Future Work

### self-hosting-future-remote-deps

- **来源**：B0 scaffold-z42c-selfhost（2026-06-07 User 裁决）
- **触发原因**：现无整体包管理；workspace 兄弟解析先只支持「从当前 workspace 找本地项目」
- **前置依赖**：registry / git-URL 依赖来源 + 版本求解 + 下载缓存机制的整体设计
- **触发条件**：跨 workspace / 第三方包分发需求出现时
- **当前 workaround**：所有非 stdlib 依赖必须是同 workspace 的本地 member 且在 toml 声明

### self-hosting-future-z42c-stdlib-jit

- **来源**：dogfood-z42c-stdlib-build（replace-csharp-compiler S3，2026-06-21）
- **触发原因**：S3 已落地——`_buildStdlib` 用 z42c **interp** 重编 stdlib（~30s）加到每次 `build stdlib`（dogfood 税）。换 z42c `--mode jit` 可显著加速。
- **前置依赖**：z42c `--mode jit` 实测 22 库 jit-built == interp-built 功能等价（fix-jit-cross-zpkg-call 已修 JIT 跨包，理论可行）；需验证 jit 编 stdlib workload 无 cross-zpkg undefined-fn。
- **触发条件**：stdlib 构建成迭代瓶颈时（S3 已落地，interp 税现已存在）。
- **当前 workaround**：`_buildStdlib` step 4 用 `--mode interp`（稳）；C# 种子 step 仍快，z42c 重编是增量税。

> **~~self-hosting-future-s3-remaining-codegen-bugs~~ ✅ 已全解（2026-06-22，S3 落地）**：dogfood S3 暴露的全部 z42c codegen bug 均已修复并各自独立归档——① 「blake3 多块 codegen」实为测试 golden 误写（无 codegen bug）；② 静态字段 mutation 不持久 → fix-z42c-static-field-assign；③ ctor `: this(...)` 委托误编为 base → fix-z42c-ctor-this-delegation。S3 full gate 全绿（stdlib 272/272 z42c-built）。

### self-hosting-future-inherited-optional-param-arity

- **来源**：fix-z42c-tsig-optional-params（2026-06-21）
- **触发原因**：z42c `Z42FuncType` 不携 `MinArgCount`（构造器仅 params/paramCount/ret），import 时丢失 → `ExportedTypeExtractor._fromImportedMethod`（子类 re-export 继承自其它包的默认参数方法）只能 emit 全必填 arity。直接定义的方法已修（`_requiredCount` 读 AST `Param.Default`）。
- **前置依赖**：给 `Z42FuncType` 加 `MinArgCount` 字段 + 所有构造点透传 + `ImportedSymbolLoader` 从 TSIG 读回。
- **触发条件**：stdlib/用户码出现「子类继承其它包默认参数方法并 re-export」且该方法被第三包省略默认实参调用时（当前 stdlib 未触发，full gate 绿）。
- **当前 workaround**：此类继承方法在 TSIG 中标全必填；调用方需显式传全部实参（或在定义类直接覆写）。

### ~~self-hosting-future-indexed-zpkg~~（✅ 已解决 2026-07-08，add-indexed-zpkg-min-patch）

> indexed/FILE 模式已按重定义布局实装（zpkg 0.24）：主文件 FILE 目录 + 自包含散装
> fullMode zbc + 内容 hash 校验；z42c writer/reader + VM 装载 + fixture 全部落地，
> `indexed-minimal` fixture 解除 minor=22 搁浅。以下为历史记录：

#### 原条目（历史）

- **来源**：add-params-varargs 5.6（2026-07-01 regen zpkg-format fixture 时发现代码级延后未记录）
- **触发原因**：`z42c.project` 的 `ZpkgWriter.z42`/`ZpkgReader.z42` 自举重写时未实现 indexed/FILE（增量编译 cache）模式（源码内联注释"延后：indexed/FILE"/"indexed 不在消费面"已如此声明，但从未补记到 design doc）；当前 `z42c.pipeline`（`WorkspaceBuild.z42`/`PipelineSkeleton.z42`）没有任何路径读写 indexed zpkg，Rust `zbc_reader.rs` 对 indexed zpkg 也是显式 `bail!`（"indexed zpkg cannot be loaded directly by the VM"）。
- **前置依赖**：出现真实 indexed 消费方需求时，先设计（谁在什么时机读/写 indexed zpkg、debug-态按文件加载策略），再实现 `ZpkgWriterZ.WriteIndexed` + `ZpkgReader` 对称读。
- **触发条件**：需要 debug-态 indexed（按文件散装）zpkg 时。
- **当前 workaround**：`z42c build` 永远产 packed zpkg；`src/tests/zpkg-format/indexed-minimal/` fixture 保留 C# 时代旧字节基线（`minor=22`），随 add-params-varargs 的 zpkg minor 0.23 bump **不跟随 regen**——该 fixture 当前无法用 z42c 生成，其余 3 个 zpkg-format fixture 已 regen 到 0.23。
- **范围收窄（port-incremental-build-cache，2026-07-05）**：本条原兼指「增量编译 cache」——packed 模式单文件 fullMode cache `.zbc` 落盘 + 整包级增量 probe 已在该 change 落地（见 [project.md 增量编译节](project.md)），本条仅剩 **indexed/FILE zpkg 模式**本身；workspace 构建的增量布线另见 [project.md#incremental-future-workspace-wiring](project.md#incremental-future-workspace-wiring)。

### ~~self-hosting-future-single-vm-bootstrap-gap~~（✅ 已解决 2026-07-09，fix-bootstrap-format-bump-deadlock）

> ci-bootstrap 加版本差 gate + 两代自举(旧 VM 从 SDK bin/z42vm 跑 gen1/gen2 → 新 VM 接管;
> runtime/compile stdlib 经 entry-dir vs Z42_LIBS 分离)。已本地用真实种子 + 人造 bump 端到端
> 验证。格式 bump 从此 CI 自动过、免手动传种子。以下为历史记录:

#### 原条目（历史）

- **来源**：add-params-varargs（2026-07-01，zpkg minor 0.22→0.23 bump 实测触发）
- **触发原因**：`scripts/build/xtask_compiler.z42:_buildCompiler()` 与 CI `.github/actions/ci-bootstrap/action.yml` 共享同一结构缺陷——都先用**当前源码**新鲜 `cargo build` 出一个 strict-pin 到**新** zpkg/zbc minor 的 z42vm，再立即用这个新 VM 去跑**旧格式**的种子 driver（本地：`artifacts/build/z42c/...z42c.driver.zpkg`；CI：下载的上一个 nightly SDK 种子）。zpkg/zbc 是 strict-pin（无兼容回退），新 VM 天然读不了旧种子 → `zpkg minor 22 not supported (writer is at 0.23)`。`version-bumping.md` "bump 与 xtask↔nightly bootstrap 循环" 段落记载的"自愈设计"（`build-and-test` 全源码 bootstrap，不依赖 download-bootstrap）看来从未在**完全 C#-free**（C# 种子已于 2026-06-26 移除）的条件下针对一次真实 zpkg minor bump 做过验证——本次 bump 是 C# 移除后第一次触发的 minor bump。
- **前置依赖**：需要真正的两代自举：Gen1（旧 VM + 旧种子编译当前源码 → 产出仍是旧格式容器，但内含新逻辑字节码）→ Gen2（旧 VM 运行 Gen1 产出的 driver，其内部执行的正是新 writer 逻辑 → 产出真正的新格式输出）→ 之后再切到新鲜 VM 消费 Gen2 起的产物。当前脚本/CI 都跳过了"旧 VM 运行 Gen1 driver"这一中间步骤，直接对旧种子用新 VM。
- **触发条件**：任何后续 zpkg/zbc minor version bump（下一次改动 zbc/zpkg wire format 时会立刻复现）。
- **当前 workaround**：本次 add-params-varargs 的验证改为手工两代自举（用 `/private/tmp/nightly-seed/` 遗留旧种子 + `.z42/bin/z42vm` 旧 VM 手动分步 build）证明 Option A（`ExportedMethodZ`/`ExportedFuncZ` 的 `ParamsFrom` 改为累加式而非新增必填构造参数）本身不会破坏自举链；`_buildCompiler()`/CI 的结构性两代自举缺口本身未修，留给独立的 `fix` 变更处理。

> **已排查排除（2026-07-02）**：此前记录的 `self-hosting-future-attributesynth-cross-pkg-resolution`（`AttributeSynth.z42` 手工两代自举下 8 处 `E0401 unknown type in 'new'`）经复现排查确认是**手工构建脚本的假阳性**，非编译器缺陷——根因是 `z42c build --output-dir <dir>` 直接写到传入目录、不会自动建 `dist/` 子目录；此前的手工脚本误假设有嵌套 `dist/`，导致 `z42c.core.zpkg`/`z42c.ir.zpkg` 未真正复制进 runlibs，下一步跨包 `new` 解析自然报 unknown type。用显式校验 `.zpkg` 是否落地的脚本重跑同一路径（旧 VM + 旧种子编译当前源码），`z42c.semantics`（含 `AttributeSynth.z42`）零错误构建成功。该条目已移除。
