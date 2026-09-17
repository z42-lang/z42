# 批 5 核实记录（toolchain / testing / devinfra / embedding）

> 与 [batch3-verification.md](batch3-verification.md) / [batch4-verification.md](batch4-verification.md)
> 同体例。本批的输入是**流程与架构描述**（不是 API 签名），所以核实手法换成：
> 逐个路径 `ls`、逐个命令 `--help`、逐个 CI job 对 `.github/workflows/`。

## 方法（相对批 4 的调整）

批 4 的对象是 API 签名，核实靠 `grep public` + 实跑。本批对象是「怎么做」，核实靠：

1. **每个源码路径 `ls` 确认还在**
2. **每个命令 / 子命令 / 旗标 `--help` 确认还在、拼写还对**
3. **每个 CI job 名对着 `.github/workflows/*.yml` 核**
4. 三条机械筛选规则照旧（Phase 表默认过期 / `.cs` 路径作废 / **grep 命中 ≠ 事实，必须打开看**）

---

## 嵌入 C ABI（L 组）

### 旧文档 vs 事实

| # | 旧文档原话 | 事实 | 证据 |
|---|---|---|---|
| 1 | §4.4 列了 **6** 个 C 函数 | 真实导出 **10** 个，漏 `z42_host_set_stdout_sink` / `set_stderr_sink` / `z42_zpkg_read_namespaces` / `z42_host_run_app` | `host/mod.rs:96,162,192,263,355,446,455,507,523,659` |
| 2 | §5「Tier 2 Rust crate 在 `src/toolchain/workload/host-api/`」 | **该目录不存在**；真实在 `src/runtime/crates/z42-host/` | `src/runtime/Cargo.toml:2` |
| 3 | §8「实现侧 `src/runtime/src/native/io.rs`」 | **该文件不存在**；真实在 `src/runtime/src/corelib/io.rs` | `ls` |
| 4 | §10「`ERR_VERIFICATION`(11) 通过 `verify_constraints` 抛出」 | **运行时从不返回 11**：`verify_constraints` 失败被 `.context()` 包成普通 Err → 统一映射 `BadZbc`(10) | `ops.rs:173` → `mod.rs:229` |
| 5 | §9.2 C 例用 `z42_value_string("World")`（注「helper from z42_abi.h」） | `z42_abi.h` **没有这个符号**；且 string tag 会被 marshal 拒掉 → `ERR_ARG_MISMATCH` | `marshal.rs:27` |
| 6 | §9.3 Rust 例用 `Value::string("World")` | `z42-host` 的 `Value` 只有 `null` / `i64` / `f64` / `bool` 四个构造器 | `z42-host/src/lib.rs:197-255` |
| 7 | §5 `HostConfig { stdout_sink, stderr_sink }` | 真实字段名是 `stdout` / `stderr`；另有旧文档没写的 `zpkg_resolver` | `lib.rs:83-95` |
| 8 | §11.7「Android facade 构造时需传 `Context`」 | 需传的是 **resolver**（`Z42VM(zpkgResolver: ZpkgResolver, …)`，必填无默认）；`AssetZpkgResolver` 自己吃 `AssetManager` | `Z42VM.kt:16-17` |
| 9 | §3 表 facade 列 `{ios, android, wasm}` | 还有 **`desktop/`**（`testhost.c` / `apphost_embed.c` / `tests/r1_r7.c`） | `ls src/toolchain/workload/` |
| 10 | §0「详见 `docs/spec/archive/<date>-define-platform-test-contract/…`」 | `<date>` 是**未填的占位符**，死引用 | — |

**唯一没烂的部分**：§10 表里引用的 12 个测试名全部还在 `host_tests.rs`（逐个 grep 过）。

### 核实中确认的正面事实

- **三份 `z42_host.h` 其实只有一份**：iOS / Android 那两份是 7 行转发头
  （`#include "…/runtime/include/z42_host.h"`），不存在三方漂移。
  头文件 10 个声明与 Rust 导出**逐字对得上**。
- 三平台默认 resolver 名字（`BundleZpkgResolver` / `AssetZpkgResolver` /
  `bundleStdlibNode|Browser`）全部属实。
- SDK 包里头文件落点 `native/include/` 属实
  （`scripts/package/xtask_stage_components.z42:60`）。

---

## 测试体系（O 组）—— 本批落空最严重的一份

`docs/design/testing/testing.md`（1211 行）**几乎整篇是虚构的**：26 条断言落空。
典型几类——

| 类别 | 旧文档 | 事实 |
|---|---|---|
| CLI 旗标 | `--format <pretty\|tap\|json\|junit>` / `--filter` / `--list` / `--dry-run` / `--platform` / `Z42_TEST_PLATFORM` | **全部不存在**，只有 `pretty` / `json` 两种格式 |
| 退出码 | `0/1/2/3`（3 = 0 tests discovered） | 只有 **0/1**；0 discovered → **1** |
| Assert 位置 | `src/libraries/z42.test/src/Assert.z42` | 住 **`z42.core`** |
| Assert 方法名 | `Assert.eq` / `Assert.throws<E>` / `Assert.near` | `Assert.Equal` / `Assert.Throws(typeName, action)` / `Assert.EqualApprox` |
| 平台门控 | `[SkipPlatform]` + `[Feature]` + `CapabilitySet` bitflags + `CapabilityRegistry.cs` + `src/runtime/src/test_runner/capabilities.rs` | **一个都不存在**；只有 `[Skip(platform:/feature:)]` + `Std.Platform.Capabilities()` |
| 能力名 | `interp` / `jit` / `multithreading` / `filesystem` | 真 caps = `jit` / `native-interop` / `bundled-compression` / `threads` / `socket`——**三个是编的** |
| CI job 名 | `host-tests` / `wasm-tests` / `android-tests` / `ios-tests` | `build-and-test`(显示 `test-host(<plat>)`) / `test-wasm` / `test-ios` / `test-android` / `test-desktop` |
| 编译器测试 | `src/compiler/z42.Tests/` xUnit + `dotnet test` + `GoldenTests.cs` | **全没了**（C# 编译器已删）；`xtask test compiler` = 自举不动点 + smoke |
| 子命令 | `./xtask test lib <lib>` | 真名 `xtask test stdlib [lib]` |
| TIDX | `version = 2`；`expected_throw_type` "reserved for R4 — currently 0" | `TEST_INDEX_VERSION = 3`（含 `timeout_ms`）；`expected_throw_type` **在用** |
| Bencher 默认 | warmup=10 / samples=100 | **自适应采样**（pilot 估算 → 填满 ~50ms 预算，n∈[20,2000]） |
| 执行路径 | in-process vs subprocess 两条路径 + `--legacy-subprocess` + runner `--jobs N` + `bootstrap.rs`/`runner.rs`/`exec.rs`/`parallel.rs` | **全不存在**；模式由承载 z42b 的 `z42vm --mode` 决定 |

**`test-runner-bootstrap.md` 确认作废**（裁决 5.7 成立）：`src/toolchain/test-runner` 不存在；
`grep -rn "test-runner\|TestRunner" src/runtime/src src/toolchain` 命中 30 行**逐行打开看过，
全是注释 / README 叙述**，零 Rust 代码。（唯一同名残留 `z42.test/src/TestRunner.z42` 是个
**完全无关**的 z42 小类。）

**GREEN gate 的 14 个 stage 已逐项核对**：`_gateStageNames()`（`scripts/test/xtask_test.z42`）
与 `_testAll` 里 14 次 `_stageStart(...)` 顺序逐字一致，也与 `test-gate.md` 的
`<!-- gate-stages:begin/end -->` 区逐字一致。

---

## REPL / 编辑器（N 组）

| # | 旧文档 | 事实 |
|---|---|---|
| 1 | `z42.repl.zpkg` 在 `libs/` | 在 `programs/z42i/`，与 `libz42_repl.dylib` 同处 |
| 2 | 「泛型返回类型函数头 `List<int> foo()` 被 Classifier 保守漏判」 | **已修**，实跑通过 |
| 3 | 「完全限定名 `Std.IO.Console.WriteLine` 是 REPL 既有限制」 | **已通** |
| 4 | 行编辑器「由 Rust 侧实现，通过 native builtin 暴露」 | 已剥离成 **host-only cdylib** `crates/z42-repl`，VM 惰性 dlopen，不走通用 `native_search_paths()` |
| 5 | `Repl.ReadLine(prompt, initial)` 两参 | 现为单参 |
| 6 | 输入分类表列 `record` 关键字 | `record` **已删**（`[Record]` attribute 替代），TokenKind 23 空号保留 |
| 7 | 性能数字（`1+1` 1.72s→0.40s、每轮 ~72ms） | **全部严重失真**（实测快一个数量级） |
| 8 | 状态模型 `$ReplVars` / `$Eval_N` | 实际 `Vars{N}` / `Eval{N}`，且非声明轮不发新类 |
| 9 | `Script.Eval` 流程里的 `InputClassifier` / `Method.Invoke` / 直接调 `z42c.pipeline` | 类名是 `Classifier`；调用走 `Engine.Invoke`；编译经 `IReplCompiler` 门面 |
| 10 | 编辑器页「需 VSCode ≥1.89」 | 混淆了两件事：`engines.vscode: ^1.75.0` 是 grammar 贡献点下限，1.89 是**工作区本地扩展安装路径**的下限 |
| 11 | `xtask deps install vscode` symlink 到 `~/.vscode/extensions`（**生成器源码注释也这么写**） | 实际装 `<repo>/.vscode/extensions/z42.z42-lang`——**源码注释自身过期** |

---

## 工具链（M / K 组）

### 命令面与产物路径

| # | 旧文档 | 事实 |
|---|---|---|
| 1 | build-orchestrator：「相位**封闭（八个）**」 | **九个**，漏了 `Preflight`（Resolve 之后、Compile 之前） |
| 2 | 「z42b 当前为 PARKED 骨架，未接编译」 | 全假：toml 存在、14 个源文件全 included、`bin/z42b` 随 SDK 发布、8 个动词可用 |
| 3 | 「Compile 不 fork z42c 子进程」（全局断言） | 只对 build/export 成立；**publish 明确 fork `<sdk>/bin/z42c`** |
| 4 | 「z42b 读 toml → `Pipeline.Run` 产 publish 交付件」 | publish **完全不经 Pipeline**；走 Pipeline 的 `_cmdPublish` 是**死代码**（无调用点） |
| 5 | 「tail 虚分发沿 项目→workload→基类」 | `_selectWorkload()` 无条件返回 `WorkloadBase`，四个 `*Workload` 类方法体**全是注释** ⇒ tail 四阶段今天**全 no-op** |
| 6 | launcher 磁盘布局含 `bin/apphost` | SDK 里**没有**；stub 只在 desktop workload 里，名 `apphost-<rid>` |
| 7 | 「三包发布结构：SDK / **launcher** / runtime」 | **没有 launcher 包**；index 里每个桌面 RID 只有 `sdk` + `runtime` |
| 8 | export.md 实现在 `launcher_export_ios.z42` | **该文件不存在**；生成器已搬进 workload 包。`launcher_export.z42` 自己的文件头注释也仍指着三个不存在的文件 |
| 9 | export wasm 产物 `app.zpkg` / `z42vm.js` / `z42vm.wasm` | 实测 `app.zbc` / `z42_wasm.js` / `z42_wasm_bg.wasm`——**三个名字全错** |
| 10 | export Android：Gradle 8.4 + AppCompatActivity 骨架 + TODO | Gradle **9.7.1**、compileSdk/targetSdk 37、minSdk 26、MainActivity 可运行 |
| 11 | iOS `main.swift` 留 `// TODO: call z42_run_app` | 早已填好：`Z42TestHost.runApp` → `z42_host_run_app` |
| 12 | platform-export：`z42 run <plat>` / `z42 platform add` / `z42 eject` / `platform-overrides/` | **全部不存在** |
| 13 | workload 分发：`z42 install <ver>` / `z42 update` / `z42 use` / `z42 self update` | **全部不存在**；宿主 runtime 只由安装脚本装 |
| 14 | CLAUDE.md / `src/toolchain/README.md`：「debugger·builder 占位」 | **没有 debugger 目录**；builder 是 live 组件，而 `builder/README.md` 还把三个 included 的源文件列为「PARKED」 |
| 15 | `book/cli.md`：`--mode <interp\|jit\|aot>` | z42vm 只接受 `interp` / `jit`；`--mode aot` → `error: invalid value 'aot'` |
| 16 | `tools.md`：`--opt` 名 = `const-fold/copy-prop/dce/inline/all` | 实际 **12 个** + `all`/`none`（另有 `cse` / `licm` / `stack-alloc` / `loop-alloc-reuse` / `readonly-load` / `pure-call` / `dead-branch` / `devirt`） |
| 17 | `reference/toolchain/z42-toml.md:496-530`（**本书内的页，不是冻结区**）：`[profile.debug] mode = "interp"` / `optimize = 0–3` | **两条都错**：`[profile.<n>]` 下写裸键现在是硬错误；`optimize` 也不是 0–3 数值档 |

---

## 附录 A · 实现缺口（值得单开 change）

### A0 · 工具链的三条真 bug

| # | 缺口 | 证据 |
|---|---|---|
| 1 | 🔴 **`z42 export --rid browser-wasm` 永远拿不到字节码**：`_expResolveZbc` 找 `<output_dir>/<name>.zbc` / `<projDir>/.cache/<name>.zbc` / `<projDir>/<name>.zbc`，而 z42c 实际按源文件树写 `dist/src/main.zbc` ⇒ **默认布局工程 100% miss**。实跑复现：导出目录只有 `index.html` + `index.js`。它是唯一没改用 `BuildLayout` 的产物解析点（ios/android 的 `_expResolveZpkg` 都用了） | 实跑 |
| 2 | 🔴 **`[optimize]` 清单段被解析后直接丢掉**：`ManifestLoader._parseOptimize` 填 `ProjectManifest.OptimizeNames/Values/Count`，但**全仓零消费方**；`Opt.Resolve` 的唯一调用点 `BuildCommand.z42:74` 永远传 `tomlBits=0, tomlMask=0`。实测 `[optimize] inline = false` **被静默收下、不报错、不生效**。字段注释还写着「消费方（z42c.driver）按名映射」——那段代码不存在 | 源码 + 实跑 |
| 3 | 🔴 **`z42 test` 编译失败时 JSON 输出根本不是 JSON**：编译失败以未捕获异常冒出（栈顶 `Std.Exception: compile failed at Pipeline.z42:88` + 十行 z42b 内部栈帧），`--format json` 下同样 ⇒ 抓取方拿不到结构化结果 | 实跑 |

### A0e · 测试体系：给了虚假保证的中间态

| # | 缺口 | 证据 |
|---|---|---|
| 4 | 🔴 **`[Timeout(milliseconds: N)]` 端到端断掉**：编译期齐全（`DeclEnforcer` 报 E0917）、TIDX v=3 带 `timeout_ms: i32`、Rust `TestEntry.timeout_ms` 解得出来。但 ① `LoadedTestEntry`（`__load_module` 的返回形状）**没有 timeout 字段**；② z42 侧 `Std.Test.TestEntry` 也没有；③ `grep -rni timeout src/libraries/z42.test/src/` **零命中**。⇒ 一条 `[Timeout]` 编译通过、写进字节码、**运行期被完全忽略**。设计文档承诺的 hang detector / clamp / `note:` 警告全不存在。**这个中间态比没有更糟——它给了虚假保证** | 全链 grep |
| 5 | **`TestResult` 无 `duration_ms` / `failure_location` / `stack_trace`**：而 runtime **有**栈（`exception/mod.rs::populate_stack_trace` 一直在填，interp 与 jit 都覆盖），`Runner._runOne` 的 catch 只读了 `ex.Message` + `FullName`——**栈就在手边却没往 TestResult 里放** | 源码 |
| 6 | **`Runner._runLifecycle` 静默吞掉 Setup/Teardown 异常**（`catch (Exception ex) { }` 空块）⇒ 坏掉的 `[Setup]` 表现为「测试本身失败」，排查方向被带偏 | 源码 |
| 7 | **共享 VM 对「同 namespace 多 `.zbc`」静默误报**：`__load_module` first-wins 去重 → 第二个模块的 `[Test]` 不注册 → 报 `function ... not found`，伪装成测试失败。靠「每个测试文件一个独立 namespace」的约定回避 | 源码 |
| 8 | **两套并行的平台门控**：`[Skip(feature:)]`（声明式、运行期查 `Capabilities()`）与 `_targetExcludes`（编排期按**用例名前缀**硬编码，已积累约 25 条）判同一件事，真相源有两个 | `xtask_test_embedded_golden.z42` |

### A0g · GC / 平台面的三条（S 组，全部实跑复现）

| # | 缺口 | 证据 |
|---|---|---|
| 1 | 🔴 **strict OOM 在默认 JIT 模式下完全不生效**：`SetMaxHeapBytes(2MB)` + `SetStrictOOM(true)` 后，`--mode interp` 抛 `cannot allocate array[...]: heap limit exceeded`，而**默认 jit 模式下数组分配直接得 `null`**，随后炸在 `ArraySet: expected array, got Null`。根因：`make_oom_exception` 的调用点只在 `interp/exec_array.rs` / `exec_call.rs` / `exec_object.rs`，**JIT 侧一个都没有**；GC 层契约是「越限返 `Value::Null`」（`gc/heap.rs:375`），翻成异常是解释器独有的一层。**用户以为设了保险丝，实际拿到 null 并炸在无关的地方** | 实跑 |
| 2 | 🔴 **脚本侧弱引用实测永不失效**：造对象 → 返回 `WeakHandle` → churn 20 万次分配 → `ForceCollect()`（`freed=10400000`, `GcCycles=1`）⇒ **目标仍然活着**。三档 gc-mode（stw / concurrent / generational）× 两种 exec mode **全试过，无一例外**；`GCHandle.AllocWeak` 同样。Rust 单测 `handle_weak_clears_after_sweep_collects_target` 却是绿的。疑似栈帧 reg / 帧池不清槽留下 stale 强引用把目标钉住。⇒ **任何「用弱引用做缓存淘汰 / 生命周期判定」的代码在 z42 上静默失效** | 实跑 |
| 3 | 🔴 **声明未赋值的 struct 局部变量访问成员直接崩**：`GCHandle def; def.IsAllocated` → `VCall: expected object, got Null`（interp / jit 都是）。struct 局部没拿到零值实例而是 `Null`。**不止 `GCHandle`，是所有 struct 的共性** ⇒ 要么补 definite-assignment 编译期检查，要么给 struct 局部零初始化 | 实跑 |
| 4 | `PauseStatsRaw()` 的哨兵文档写错：注释说未发生 collect 时 `min_us == long.MaxValue`，**脚本侧读到的是 `-1`**（Rust `u64::MAX` 过 `Value::I64` 边界后变号） | `types.rs:231` + `corelib/gc.rs:268` |
| 5 | `GC/README.md` 的成员表**漏 9 个成员**（Finalize / 4 个停顿 API / 快照 / 2 个 OOM 旋钮）且**整个 `SoftHandle.z42` 没列**（文件表写 4 行，实际 5 个文件） | 源码 |
| 6 | `WriteHeapSnapshot` 注释仍写「v1 局限：不 stream 大堆（in-memory build → 一次 write）」，**实际早已流式**（`serialize_v8_heapsnapshot_to` 直接写 `BufWriter<File>`，注释自己都写了「避免 ~30 MB 中间 String」）——局限段没跟着改 | `corelib/gc.rs:317-327` |

### A0h · 测试框架的五条（T 组，全部实跑复现）

| # | 缺口 | 证据 |
|---|---|---|
| 1 | 🔴 **`Assert.Skip(reason)` 判失败而不是跳过**：`Runner._runOne` / `_invoke` 对 `Std.SkipSignal` 没有分支，一律落进「Threw → failed」，而 `Assert.z42:115-119` 与 `Failure.z42` 头注都明写它是「运行期跳过，runner 标记 Skipped」。**唯一的用例 `dogfood.z42` 用 `[ShouldThrow<SkipSignal>]` 验它，正好绕开了这条路径**，所以从没被抓到 | 实跑：`FAIL … Std.SkipSignal: env var missing`，退 1 |
| 2 | 🔴 **`[Skip(platform:)]` 在场时 `[Skip(feature:)]` 被静默丢弃**：`_skipApplies` 是 `if platform != null → return …`，feature 分支够不着。设计文档白纸黑字要 **OR**。代价：`[Skip(platform:"wasm", feature:"socket")]` 这种「wasm 或无 socket 就跳」的意图，在非 wasm 的无 socket 环境上**照跑并炸** | `Runner.z42:217-221`；实跑 `[Skip(platform:"linux", feature:"nonexistent")]` 在 macOS 上 **PASS** |
| 3 | 🔴 **`TestFailure` 的 `Actual` / `Expected` / `Location` 三个字段被运行器丢弃**：`_runOne` 只取 `ex.Message`，于是 `Assert.Equal` 失败**只能看到固定的 `values not equal`——期望值与实际值都拿不到**，而它们明明已经在异常对象里。`Location` 恒为空串。修法只是 `_runOne` 里多读两个字段 | 源码 + 实跑 |
| 4 | **`[Setup]` / `[Teardown]` 抛异常被静默吞掉**（与 O 组独立撞到同一条）：实跑 `[Setup]` 里 `throw` 之后**测试照常运行、照常 PASS，报告零痕迹**——fixture 挂了不会让任何东西变红 | `Runner._runLifecycle:204` 空 catch |
| 5 | **`--filter` 在 CLI 里重复注册**（`builder_cli.z42:72`/`:75`，bench 侧 `:102`/`:105`），`--help` 打出两行互相矛盾的说明；其中 `(reserved)` 那条是从未实现的「按测试方法名过滤」占位 | 实跑 |

补充实证：**`[Timeout(milliseconds: 50)]` 的测试跑满 2.0 秒仍判 PASS**（墙钟 2.773s）——
坐实了 A0e #4 的「编译通过、运行期完全忽略」。
⇒ **死循环的测试会永远挂住整个 `z42 test`，没有任何兜底。**

### A0f · 一个永远不会失败的测试

**OSKind / ArchKind 契约的守门是真空的。** `z42.io/tests/platform.z42` 只验**自洽**：
`IsLinux()` 的定义就是 `OSKindValue() == OSKind.Linux`，所以「k == OSKind.Linux ⇒ IsLinux()」**恒真**。
把 Rust 侧 macOS 改映射成 3，**全套测试照绿**（`trueCount <= 1` 仍满足，只是变成 `IsWindows()`）。
需要一条「本 CI OS 上 `OSKindValue()` 必须等于具体值」的断言。

### A1 · 嵌入 ABI

| # | 缺口 | 证据 |
|---|---|---|
| 1 | 🔴 **`exec_mode` 只被校验、不选后端**：`from_raw` + `check_feature_available` 之后**再无人读它**，句柄式 invoke 永远走 `interp::run_returning`。**宿主设 `Z42_EXEC_MODE_JIT` 不会 JIT** | `ops.rs:284` |
| 2 | 🔴 **`heap_initial_bytes` / `heap_max_bytes` 全仓无消费点**，只进 `ResolvedConfig` 和 Debug 输出——宿主以为自己设了堆上限，实际没有 | 全仓 grep |
| 3 | 🔴 **`z42_host.h` 注释整体停在「H1 scaffold」**：`load_zbc` / `resolve_entry` / `invoke` 三处仍写 "H1 status: returns ERR_INTERNAL (placeholder for H2 implementation)"，文件头 Status 块同样。**宿主开发者读头文件会被误导成「这三个函数没实现」**。Spec 指针还指向已冻结的 `docs/design/runtime/embedding.md` | `src/runtime/include/z42_host.h` |
| 4 | **`set_stdout_sink(NULL)` 语义与注释不符**：注释写 "NULL restores the configured default"，实际是**卸载**（`build_host_sink(None)` → `install_host_stdout_sink(None)`），回落到进程 stdout，**不会**恢复 `initialize` 时 config 里那个 sink | `host/mod.rs` |
| 5 | **`Z42_HOST_ERR_VERIFICATION`(11) 是空枚举值**：ABI 里公开、宿主可以 switch 到、运行时永不产生；Tier 2 `translate_status` 那一支是死代码 | 见上表 #4 |
| 6 | **sink 的线程语义比头文件注释窄**：头文件写 "runs on whichever thread emitted the output"，实际由线程局部 `HOST_SINK_ACTIVE`（`invoke_impl` 的 RAII guard 置位）把关——**只有执行 invoke 的那条线程**的输出进 sink，z42 自己 spawn 的线程不进 | 源码 |
| 7 | **`z42_host_run_app` 注释漏退出码 71**（`spawn VM thread failed`），只列了 0/1/2/70 | `mod.rs:751` |
| 8 | **死分支**：`let status = if msg.contains("not found") { EntryNotFound } else { EntryNotFound };` 两支相同 | `mod.rs:315-319` |
| 9 | `src/runtime/include/README.md` 的导出清单漏 `z42_zpkg_read_namespaces` / `z42_host_run_app` / `Z42NamespaceVisitor` | — |

---

## 附录 B · 本批的边界裁决记录

1. **ZpkgResolver 做了二次拆分**（原分配是「整节留 internals」）：
   C 回调 typedef、hit/miss 语义、字节生命周期、`z42_zpkg_read_namespaces` 是**契约**
   （它们就长在 `z42_host.h` 里）⇒ 进 reference；
   解析决策树、Tier 2 trait、`CHookResolver` 适配、三平台默认 resolver ⇒ 留 internals。
   依据 doc-system §2.2「嵌入 | reference 写 C ABI 契约 | internals 写 VM 内部如何实现」。
2. **旧 `embedding.md` 第二个 `## §11`（编号已乱）那约 60 行**讲的是
   9 个 per-arch SDK package 形态 + GitHub Releases 发布流程，**与嵌入无关**，
   已从 embedding 页删除，转交 devinfra 的 packaging / release 页承载。

---

## 批 6 · philosophy / features 的核实（21 条落空）

`docs/features.md` 与 `design/philosophy.md` 是全仓被引用最多的两份「设计北极星」，
实测 **21 条断言落空**。最要命的几条：

| # | 旧文档 | 事实 |
|---|---|---|
| 1 | features §7「Generics… **Monomorphized** at compile time」 | 既不是单态化也不是 Java 擦除，是 **C# 式代码共享 + 运行期具化**（`generics.md:30-56` 明写「为什么不选纯 Rust 单态化 / 为什么不选 Java 类型擦除」）。**三份说法互不一致**：旧 features 说单态化、源码注释（`EmitContext.z42:212` / `ClassDescBuilder.z42:158`）说「类型擦除」（其实只指方法体内看不到 class name）、`generics.md` 说代码共享 |
| 2 | features §15「`string` is **UTF-16** compatible internally」 | **UTF-8**（VM 对象是 `len + inline UTF-8`） |
| 3 | philosophy §9「Bytecode compression **40–60% smaller** than source」 | 实测只小 **21–38%**（四个包 src vs zpkg：regex 78.6% / text 62.0% / json 78.1% / z42c.syntax 77.6%）。断言已删 |
| 4 | features §10「No raw thread primitives are exposed」+ philosophy §7「No data races：类型系统阻止无同步的共享可变状态」 | **全假**。`Std.Threading.Thread` 是真 OS 线程；线程共享 GC 堆与静态字段，**竞争由程序员负责** |
| 5 | features §11/§12「`[ExecMode(Mode.Jit)]` / `[HotReload]` 注解」 | 编译器内建 attribute 只有 `Suppress`/`Native`/`Deprecated`/`Record` + 测试族；**hot reload 全仓 `src/` 零命中** |
| 6 | features §13「packed zpkg 上提 shared type table」 | TYPE **按模块内联**；真正上提的是 STRS + **SIGS**（旧文没提 SIGS） |
| 7 | features §17「stripped zbc `flags=0x01`，直接加载是 error」 | `ZBC_FLAG_STRIPPED` **零调用方**，writer 从不置位，reader 无报错路径；符号剥离实际在 **zpkg 层** |
| 8 | features §16「`z42.core` 在**每个**源文件免 `using`」 | 免 `using` 的只有 `Std` 与 `Std.Runtime`；同包的 `Std.IO` / `Std.Collections` 仍要写 |
| 9 | features §18「cargo 组件 `core/interp/jit/aot/gc/...` + `z42.toml [runtime] components=[...]`」 | 真实 features 只有 `jit`/`aot`（`aot = []` 占位）+ 平台预设；**`components` 全仓无解析** |
| 10 | features §19「NativeAOT：字节码 → **LLVM IR**」 | `aot.rs` 23 行自述 stub；规划后端是 **cranelift-object**，与 JIT 共享翻译层 |
| 11 | philosophy 示例 `VM.Eval(code)` / `VM.Call("game::on_tick")` | 不存在。真实面是 `Std.Scripting.Engine` + 宿主侧 C ABI `z42_host_*` |

### ⭐ 引用了一个不存在的段落

**`roadmap.md:27` 与 `internals/compiler/scripting-charter.md:234` 都引「philosophy §9 五指标」
（interp ≤ Python 1.5× / JIT ≥ V8 70% / AOT ≥ Go 80% / GC pause < 5ms p99 / 嵌入子集 < 200KB）——
而旧 `philosophy.md` §9 从来没有这五条**（它写的是 ≤5 cycles/instr、40–60% 压缩、<10ms GC pause）。
两处引的是一个虚构的出处。本批已把五条基线归位到 roadmap 自身。

**`roadmap.md` 的「Feature → Version 映射」表按 features.md §号索引，但大面积错位**
（roadmap §7=Control Flow / features §7=Generics；§8–§17 多数对不上；还有一个 features.md
从未有过的 §20）。本批改成按能力名索引，去掉 §号锚点。

### 新增实现缺口

| # | 缺口 | 证据 |
|---|---|---|
| 1 | **`async` / `await` 静默接受**：`async void Foo() { }` **编得过且完全无诊断**——`async` 被 `DeclParser._isModifier` 当无操作修饰词吃掉，`TokenKind.Await` 零消费者。用户会以为写了异步代码 | 实测 |
| 2 | **`ZBC_FLAG_STRIPPED` 是死代码**：常量 + `zbc_is_stripped()` 定义在 `formats.rs:30,224`，全 `src/runtime/` 零调用方 | 源码 |
| 3 | **VM 侧 module path（`Z42_PATH`）事实上是死路径**：`main.rs:324-327` 自述「log only for now」，所有生产调用方给 `resolve_namespace` 传 `&[]`，只有单测走过 ⇒ 文档说的「两条搜索路径、module 优先」目前只有一条真生效 | 源码 |
