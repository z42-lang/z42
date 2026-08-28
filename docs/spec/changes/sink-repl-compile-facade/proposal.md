# PR-B：把 REPL 的「有状态增量编译世界」下沉进 z42.build 门面 → scripting 变 stdlib-only 搬 src/libraries

> 程序：`stdlib-interop-and-repl-split-program` 轴 2（编译器/REPL 库进 stdlib）。
> 前置：PR1（#314 拆 z42.repl）、PR-A（#317 前端 core+syntax 进 stdlib）均已合 main。
> route A（门面 + 运行期注入），User 定；非 route B（不搬 semantics/pipeline）。

## 背景与唯一真障碍

PR-A 把编译器**前端**（`z42c.core`=`Z42.Core`：Span；`z42c.syntax`=`Z42.Syntax`：Lexer/Parser/
Token/CompilationUnit）搬进了 `src/libraries/`，与已在 stdlib 的 `z42.ir`/`z42.project` 并列。故
scripting 里 **tokenize/completeness/classify/rewrite（Lexer/Parser 用法）已是 stdlib-only，无需门面**。

scripting 变 stdlib-only 的**唯一剩余障碍** = 它还直接依赖两个 compiler-only 包：

| 包 | 命名空间 | scripting 用到的符号 |
|----|---------|---------------------|
| `z42c.semantics` | `Z42.Semantics` | `IrDump.ParseAll` |
| `z42c.pipeline`  | `Z42.Pipeline`  | `PackageCompile.Compile` + `CompileInputs`/`CompileArtifacts`；`DepScanResult`（增量依赖世界，`ScriptState.CachedScan`）；`DepScan.{ScanDirsLazy,ExtendWithPackage,ReconcileCandidatesInNs,EnsurePackageLoaded}` |

且这不是「thin `Compile(req)→result`」——REPL 把编译器当**有状态增量服务**：`DepScanResult` 世界跨轮
缓存、逐轮增量并入、per-type 惰性 reconcile、E0401 回退重编；Completer 还**直接读** `DepScanResult`
内部（`Exported[].Classes/Enums/…`、`NsNames`、`TypeShort/TypeNs` 索引）做补全。

## 设计：opaque session 门面（coarse-query）

`z42.build`（stdlib）新增 `IReplCompiler` 门面 + `ReplCompileResult` 数据载体。**依赖世界
`DepScanResult` 作 opaque `object` 句柄穿越边界**——scripting 从不具名任何 pipeline/semantics 类型。
门面方法只收/返 `string[]`/`byte[]`/`int`（z42.build 依赖面**零增长**，仍 core/io/project）。所有
DepScanResult 遍历 + reconcile + PackageCompile 编排移到实现侧。

```z42
namespace Z42.Build;

/// 一次 REPL 轮次编译结果：packed zpkg 字节（失败为空）+ 原始诊断（未回映行号）。
[Record] public class ReplCompileResult(
    byte[]   Bytes,         // WritePacked 后 packed zpkg 字节；ErrorCount>0 时为空
    int      ErrorCount,
    string[] Diagnostics,   // 原始诊断串；Script 侧 _remapDiag 回映用户行号
    int      DiagCount);

/// REPL 增量编译 + 补全服务门面。实现住 z42c.pipeline（Z42cReplCompiler），运行期反射注入。
public interface IReplCompiler {
    /// 建依赖世界骨架（惰性）。返回 opaque 世界句柄（实为 DepScanResult）。
    object CreateWorld(string[] libsDirs, int libsDirsN, string[] declaredDeps, int depCount);

    /// 编一轮源 → packed 字节。内部含 E0401 惰性 per-type / 整包 reconcile 重试（据 usings 集）。
    ReplCompileResult CompileRound(object world, string name, string src, string[] usings, int usingsCount);

    /// 刚编出的包字节增量并入世界（carry-forward 声明轮）。
    void ExtendWorld(object world, byte[] bytes, string pkgName);

    /// 补全①：全部命名空间名（`.using` 段补全；分段/前缀在 scripting 侧做，纯串操作）。
    string[] NamespaceNames(object world);

    /// 补全②：作用域候选——已 reconcile 顶层符号(类/接口/枚举/自由函数,去 mangle) ∪ 活跃 ns 内索引类型短名。
    string[] ScopeTypeNames(object world, string[] activeNs, int nsCount);

    /// 补全③：`Type.` 静态成员（按需 reconcile + 列 public 静态方法/字段或枚举成员，去 mangle）。
    string[] StaticMembersOf(object world, string typeName, string[] activeNs, int nsCount);
}

/// 未注入实现时的兜底（L1「空值 + 调用方检查」）。
public class NoReplCompiler : IReplCompiler { /* CreateWorld→null；CompileRound→ErrorCount=1；查询→空 */ }
```

**边界划分**：
- **实现侧（`Z42cReplCompiler : IReplCompiler`，住 `z42c.pipeline`，零新依赖）**：port `Script._compileSrc/_compileSrcOnce/_loadReferencedTypes/_loadUsingsPackages/_typeCandidates` 的编译编排 + `Completer` 的 `_addImportedNames/_addIndexedTypeNames/_typeStaticMembers/_ensureReconciled/_namespaceComplete` 世界遍历。内部用 `IrDump.ParseAll`/`PackageCompile.Compile`/`DepScan.*`/`ZpkgWriterZ.WritePacked(...).ToBytes()`。
- **scripting 侧（纯 stdlib）保留**：prelude 组装、Classifier、Rewriter、`_isStatement`（Lexer，stdlib）、`_remapDiag/_countNewlines`（纯串）、Completer 的 `_wordStart/_addCand/_cleanName/_isSessionVar/_memberComplete(Engine.MemberNames)/_addKeywords(Lexer)` + 三上下文分派。补全的**前缀过滤/去重/ns 活跃策略**（含 Std/Std.Runtime 免-using）留 scripting。

## 运行期注入（mirror `_hostCompiler`，wire-z42b-dynamic-injection-preference）

scripting 新增 `ReplCompilerHost.Get() : IReplCompiler`（惰性单例），**照搬 builder.z42 `_hostCompiler`
+ `_findCompilerZpkg`**：`ModuleLoader.Load(z42c.pipeline.zpkg)` → `Type.GetType("Z42.Pipeline.Z42cReplCompiler")`
→ `Activator.CreateInstance` → `as IReplCompiler`；找不到组件 → `NoReplCompiler` 兜底。zpkg 定位序同
z42b：Z42_HOME/programs/z42c → Z42_PORTABLE_VM 反推 SDK → dev artifacts。

**行为迁移（可接受，与 z42b 一致）**：scripting 不再**静态**链 compiler 闭包 → interactive apphost
bundle 变小、运行期**动态**加载 z42c.pipeline 组件。runtime-only SDK（无 compiler 组件）REPL 退化
NoReplCompiler——但 REPL 本就必须有编译器才有意义，与 z42b「无组件则 build 不可用」同构，非回归。

## scripting 依赖收缩（物理搬迁 DEFERRED）

toml 依赖：**去** `z42c.semantics`/`z42c.pipeline`；**留/加** `z42.build`（门面）+ `z42.test`
（ModuleLoader）+ `z42c.core`（Span）/`z42c.syntax`（Lexer/Parser/Token/CU）/`z42.ir`
（ZpkgWriterZ/ZbcVersion/Exported*）/`z42.io`/`z42.core`/`z42.threading`。→ scripting 编译期 **stdlib-only**。

**物理搬迁 `src/toolchain/scripting` → `src/libraries/` 推迟到 follow-up**（User 定，2026-08-28）：
探明搬迁会连带 `z42.repl` 也变 stdlib-only 得一起搬 + 拆 `_buildScriptingLib` 特例步 + 改两 workspace
default-members / packages.toml / CI 成员表 / apphost 解析——一大块**本地不可验**的构建系统重布。而**架构
意义 + 打包可见变化（apphost 改动态加载）本次已达成**，物理搬迁纯机械、收益（scripting 作 stdlib 库供
playground）可分离。故本 PR 保 scripting 在 `src/toolchain`（仍走 `_buildScriptingLib`，合并目录是超集编得动）。

**无 z42c 自依赖环（轴④）**：scripting 是 REPL/toolchain 消费方，**编译器构建不消费 scripting**；反射加载
z42c.pipeline 是**运行期**非构建期 → 无环。新 deps（z42.build/z42.test）只 → z42.core/io/project，无环。

## 半径（改动清单）

- **新增**：`z42.build/src/{IReplCompiler,ReplCompileResult,NoReplCompiler}`（合一文件 `IReplCompiler.z42`）；`z42c.pipeline/src/Z42cReplCompiler.z42`（实现，port 编排+遍历）；`z42.scripting/src/ReplCompilerHost.z42`（反射注入）。
- **改写**：`Script.z42`（去 semantics/pipeline using，改调门面）；`Completer.z42`（世界遍历改调门面）；`ScriptState.z42`（`CachedScan: DepScanResult`→`object`，去 `Z42.Pipeline` using）。`Completeness.z42`/`Rewriter.z42`/`Classifier.z42`/`Engine.z42` 用的都是 stdlib（Lexer/Parser/native）→ **不改**。
- **toml**：`scripting` 去 `z42c.semantics`/`z42c.pipeline`，加 `z42.build`/`z42.test`。
- **build/wiring**：`scripts/build/xtask_toolchain.z42` `_buildScriptingLib` 仅注释更新（合并目录是超集，scripting 仍编得动）。packages.toml / workspace members / CI 成员表 / apphost toml **无需改**（scripting 未搬、位置不变、z42c.pipeline 仅新增文件非改成员）。
- **docs**：`docs/design/toolchain/repl.md`（新增「编译门面 + 运行期注入」节）；scripting README（职责 + 功能索引 ReplCompilerHost 行）。
- **DEFERRED（follow-up PR）**：`git mv scripting+z42.repl → src/libraries`；拆 `_buildScriptingLib`；两 workspace default-members；packages.toml；CI member 表；organization.md scripting 入 stdlib。

## 验证

**本地不可验**（种子墙：本机种子 z42c 缺 z42.ir 近期字段编不了编译器；z42vm 退出期挂起）→ **CI 权威**：
verify-selfhost 字节不动点 + test-host×4（含 REPL 端到端）+ test-stdlib interp/jit + bootstrap(轴④/格式)。
零格式 bump（不动 zbc/zpkg writer）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
