# 测试框架机制（TIDX · runner 协议 · GREEN gate）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/runtime/src/metadata/test_index.rs`（TIDX 类型 + decoder）、
> `src/runtime/src/corelib/reflection/module_load.rs` + `src/runtime/src/corelib/builtin_table.rs`
> （`__load_module` / `__invoke_static` / `__run_goldens_isolated` 三个 builtin）、
> `src/libraries/z42.test/src/`（`ModuleLoader` · `Runner` · `BundleRunner` · `TestReport` ·
> `BenchStats` · `Bencher` · `TestIO`）、`src/toolchain/builder/core/builder_test.z42`（z42b test/bench）、
> `scripts/test/xtask_test.z42`（GREEN gate）、`scripts/test/xtask_test_lib.z42`（stdlib 单元编排）。
>
> 要查 `[Test]` / `[Benchmark]` / `[Skip]` **怎么写**、`z42 test` **怎么用**、以及
> `Assert` 方法全表 → 看 reference，不在本页。本页只写这套东西**怎么实现的**。

z42 的测试发现是**编译期**的：编译器把带 `z42.test.*` attribute 的函数写进 zbc 的 `TIDX` section，
运行期**不扫** method table。运行器本身是用 z42 写的（`Std.Test.Runner`），靠三个反射 builtin
把一个已编译模块加载进活 VM 再按 FQN 调用。读这页的时机：要改运行器行为、加一个
attribute 的运行期语义、调 GREEN gate 的组成，或者搞清楚一条测试失败到底是哪一层报的。

## 1. 三层结构

| 层 | 是什么 | 在哪 |
|---|---|---|
| **发现** | 编译期写入的 `TIDX` section（method_id + kind + flags + skip/throw/timeout 槽） | 编译器 → `.zbc`；读端 `src/runtime/src/metadata/test_index.rs` |
| **执行** | z42 写的反射 runner：加载模块 → 按 FQN 调用 → 归类 pass/fail/skip | `src/libraries/z42.test/src/Runner.z42` |
| **编排** | 谁决定「跑哪些模块、怎么并行、结果怎么聚合」 | z42b（单目标）/ xtask（语料级），见 §5 |

三层之间只有两个契约：TIDX 的字段语义（§2）与 runner 的 JSON 报告形状（§4）。

## 2. TIDX：编译期发现的载体

**字节布局不在本页**——它是 zbc 格式的一部分，写在 [zbc 格式规格](../formats/zbc.md) 的 TIDX 段
（当前 `version = 3`，`TestEntry` 在 `TestCase[]` 之后追加 `timeout_ms: i32`）。本页写它的**语义**。

### 2.1 谁写、谁读

只有**含至少一条测试 attribute** 的模块才带 TIDX section；section 缺失 = 该 `.zbc` 没有测试。
读端有两个：

- Rust 侧 `read_test_index()` 解出 `Vec<TestEntry>`，挂到 `LoadedArtifact.test_index`；
- z42 侧经 `__load_module` builtin 拿到扁平化后的 `Std.Test.TestEntry[]`（§3.1）。

`kind` 是 `1=Test / 2=Benchmark / 3=Setup / 4=Teardown / 5=Doctest`（Doctest 是预留槽，
当前没有产生它的路径）。`flags` 位是 `SKIPPED / IGNORED / SHOULD_THROW / DOCTEST`，
保留位（4–15）置位会让 decoder 直接报错——这是为了让未来加位时老 VM **硬失败而不是静默忽略**。

### 2.2 字符串解析生命周期（最容易踩的一处）

TIDX 里所有 `*_str_idx` 是 **1-based** 索引，指向 **重建前的原始 STRS pool**（`0` = 无值）。
而加载器随后会跑 `rebuild_string_pool`，它**只保留被 `ConstStr` 指令引用的字符串**——
TIDX 独有的字符串（skip reason、platform 名、ShouldThrow 类型链）全部会被丢掉。

所以 `loader::load_zbc` 在 `read_zbc` 之后、`rebuild_string_pool` 之前调
`resolve_test_index_strings(entries, raw_pool)`，把索引解析进 `skip_reason` / `skip_platform` /
`skip_feature` / `expected_throw_type` 这几个 `Option<String>` 字段。

> **改 runner 代码时读 `*_resolved` 字段，别读 `*_str_idx`**。索引只为跨语言契约测试的
> round-trip 留着；在解析之后它们指向的池已经不是原来那个了。

### 2.3 编译期校验

位置与签名（`[Test]` 必须是零参 / 非泛型 / 有 body 的 free function 或 static 方法）、
以及实参语义（`[Skip]` 的 reason、`[Timeout]` 的 milliseconds、`[ShouldThrow]` 的类型实参）
由 `z42c.semantics` 的 `DeclEnforcer` 在符号收集期强制，发射 E0911–E0917。
**具体每个码的触发条件与发射点** 见 [错误码参考](../../../reference/src/appendix/error-codes.md)
的「E0911–E0917 测试框架」一节——那里逐条带 `DeclEnforcer.z42:<line>`，本页不复制。

## 3. Runner 协议

### 3.1 三个反射 builtin

runner 是普通 z42 代码，它能"加载并调用别的模块"全靠这三个 native 绑定
（登记在 `corelib/builtin_table.rs`，实现在 `corelib/reflection/module_load.rs`）：

| builtin | z42 门面 | 作用 |
|---|---|---|
| `__load_module` | `ModuleLoader.Load(path) -> TestEntry[]` | 把 `.zbc` / `.zpkg` 加载进**活 VM**（函数与类型变得可反射、可 Invoke），并返回它的 TIDX 条目。按模块名幂等 |
| `__invoke_static` | `ModuleLoader.Invoke(fqn) -> object` | 按**完全限定名**零参调用一个 free/static 函数。内部抛出的异常按原类型透传，调用方可 catch |
| `__run_goldens_isolated` | `ModuleLoader.RunGoldensIsolated(paths, entries, libsDir, jobs) -> string[]` | 每个 golden 程序在**全新隔离 VM**（各自 VmContext：堆 + 静态字段 + 函数表）里跑，返回各自捕获的 stdout。`jobs<=0` = 按核数并行 |

**为什么 Invoke 走 FQN 而不是 `Type.GetType` + `MethodInfo`**：z42 的
`[Test]`/`[Benchmark]`/`[Setup]`/`[Teardown]` 编译产物是**零参 free function**
（`<Namespace>.<func>`），根本没有承载它们的类实例。

**为什么 golden 要隔离而 unit 不用**：golden 是整程序，多个 golden 共享 `Main`/命名空间必然撞车，
且静态状态会互相渗漏；unit 是带命名空间的 `[Test]` 模块，设计上不冲突，共享一个 VM 更省。
这条区分的**代价**见 [嵌入式运行与测试 agent](embedded-app-run.md) 里"同 namespace 多文件"那一条。

### 3.2 单条 `[Test]` 的生命周期

`Runner._runOne`（`src/libraries/z42.test/src/Runner.z42`）：

```
[Ignore]?                      → skipped("ignored")，不调 body
[Skip] 且 _skipApplies()?      → skipped(SkipReason)，不调 body
否则：
  同 namespace 的全部 [Setup]  → Invoke（异常被吞）
  被测函数                      → Invoke（json 模式下 stdout 被 TestIO 捕获）
  同 namespace 的全部 [Teardown]→ Invoke（**总是跑**，异常被吞）
  有 [ShouldThrow]? → 抛了且类型链命中 = passed；否则 failed
  否则抛了            → failed（reason = "<ErrType>: <Message>"）
  否则                → passed
```

**fixture 按 namespace 而非按类分组**：free function 没有共享实例，同命名空间是唯一可用的
locality 单位。Setup/Teardown 里的异常被静默吞掉——这是当前实现的取舍，不是疏漏能推翻的，
但它意味着一个坏掉的 Setup 会表现为"测试本身失败"。

### 3.3 skip 判定

`Runner._skipApplies(entry)` 三条，按顺序：

```
SkipPlatform != null → skip iff Platform.OS() == SkipPlatform
SkipFeature  != null → skip iff !Capabilities().contains(SkipFeature)
否则                  → 无条件 skip
```

**能力集是运行期真值，不是静态推断**：`Std.Platform.Capabilities()` 背后是
`corelib/platform.rs::builtin_platform_caps`，按 cfg 拼出这个二进制**实际编译进了什么**：

| cap | 何时出现 |
|---|---|
| `jit` | cargo feature `jit` |
| `native-interop` | cargo feature `native-interop` |
| `bundled-compression` | cargo feature `bundled-compression` |
| `threads` | `not(target_arch = "wasm32")`（不是 feature——`std::thread` 在哪儿有就在哪儿有） |
| `socket` | `not(target_arch = "wasm32")` |

`Std.Platform.ExecModes()` 另报可派发后端（`interp` 恒在 + cfg 门控的 `jit` / `aot`）。
两者的正交关系见 [执行画像矩阵](exec-profile-matrix.md)。

**deny-by-default 是免费得到的**：未知 feature 名不在 `Capabilities()` 里 → 视为"缺" → 跳过。
把 typo（`multi-threading`）当成"这环境不支持"，比 fail-open 静默吞掉安全。

### 3.4 `[ShouldThrow<E>]` 的链匹配

编译期把 `E` 连同**当前编译单元里可见的每个派生类**拼成 `";"` 分隔的一串写进
`expected_throw_type` 槽（例如 `Exception;TestFailure;SkipSignal`）。运行期
`Runner._typeMatches` 逐段比：段与实际异常的 `FullName` 相等、或**短名**相等，任一命中即 pass。

这是"编译期展开继承链"的方案——运行期不需要类型层次反射。代价：只覆盖当前 CU
`using` 链上可见的类；不在 import 链上的 zpkg 依赖枚举不到，退化为直接匹配。

### 3.5 退出码

`Runner.ExitCodeFor(runnable, failed)` 是抽出来的纯函数（便于直接单测）：

```
runnable == 0 → 1        // 「测试没跑」不得与「测试全过」在退出码上等价
failed   >  0 → 1
否则          → 0
```

`runnable == 0` 判红这条是刻意的：能触发它的都是真事故——attribute 拼错（`[Tests]` / `[test]`）、
`[sources]` glob 漏掉文件、测试函数被改名或误删、或调用方把一个根本没有测试的目录当成了测试单元。
报告照常打印（json 消费方不受影响），解释另走 stderr。

## 4. 报告契约

`format` 两个取值：

- **`pretty`**（默认）：每条一行 `PASS/FAIL/SKIP <FQN>`，末尾 `Result: P passed, F failed, S skipped`。
- **`json`**：`TestReport.toJson` 手搓一个对象到 stdout。

```json
{ "tool": "z42b", "module": "<artifact path>",
  "summary": { "total": N, "passed": P, "failed": F, "skipped": S },
  "results": [ { "name": "<FQN>", "status": "passed|failed|skipped",
                 "is_benchmark": false, "reason": "…", "bench_stats": { … } } ] }
```

`reason` 仅在 failed / skipped 时出现；`bench_stats` 仅在 benchmark 且成功解析出统计行时出现。

三处约束值得记住：

1. **z42.test 不依赖 z42.json**——它是基础库，JSON 是手搓的（`TestReport.esc` 只转义
   `" \ \n \r \t`，够这份报告携带的字符串用）。
2. **`module` 是 z42 保留字**，所以那个参数在源码里叫 `artifact`。曾经把它命名为 `module`
   一次就把 parser 弄崩、且当时诊断被丢弃导致**静默误编译了这个文件本身**。
3. **`--format json` 时 stdout 只许有那一份 JSON**。构建进度必须改走 stderr
   （`BuildLog.SetToStderr(format == "json")`），否则消费方解析不了——实测症状是 xtask 转发
   bench 时基线**静默捕获到 0 条**，不报错。

### 4.1 benchmark 统计的缝

`Bencher` 与 runner 之间**唯一**的数据通道是一行文本：

```
bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>
```

`Bencher.printSummary(label)` 打它，runner 在 json 模式下用 `TestIO.captureStdout` 捕获 body 的
stdout，再由 `BenchStats.parse` 解回结构。**改了 `printSummary` 的输出形状就必须同 commit 改
`BenchStats._parseLine`**；老格式（无 `mean=` / `stddev=`）仍能解析，对应字段为 `-1`，消费方按
"没有置信区间"处理。

`new Bencher()` 是**自适应采样**：先跑一小段 pilot 估出单次耗时，再选 `n` 填满约 50 ms 的测量预算
（夹在 `[20, 2000]`）。显式 `new Bencher(W, S)` 保持固定 `S` 次，无自适应——已调过参的 benchmark
行为逐字节不变。`iter()` 返回后 `Samples` 是**实际**用的 n。

### 4.2 stdout 捕获是一个栈

`TestIO` 的四个 native 绑定（`__test_io_install_{stdout,stderr}_sink` /
`__test_io_take_{stdout,stderr}_buffer`）维护的是**栈**：install 压一层 buffer，take 弹一层。
嵌套合法（内层不影响外层）。body 抛异常时 take 在 catch 里调一次确保 sink 被弹出，再重抛——
每次 `captureStdout` 前后栈深度守恒。runner 自己捕获 benchmark stdout 时复用的就是这个栈，
所以用户在 benchmark body 里再调 `TestIO.captureStdout` 也不会串味。

## 5. 编排：z42b 与 xtask 的分工

runner 是库，**谁来驱动它**分两层：

| | z42b（`z42.builder.zpkg`） | xtask |
|---|---|---|
| 眼界 | **一个**目标（一个已编译产物、一个工程 `z42.toml`、或一份 bundle manifest） | 整个仓库的语料结构 |
| 职责 | 编译 → 部署 → 运行这一份 | 发现 `src/tests/**` + `src/libraries/<lib>/tests/**`、编全量、分片、聚合、门禁 |
| 入口 | `z42b test [target] [--name/--list/--rid/--format/--reuse-parent…]` | `xtask test <sub>` |

**缝是 bundle manifest**（一个 `{cases: [...]}` 的 JSON），`BundleRunner.RunBundle` 消费它：
golden 走隔离 VM + 比对 stdout，unit 走共享 VM + `Runner.RunModuleResults`。同一份
`BundleRunner` 被 host 侧的 `z42b test --rid host` 和设备侧的 test-agent 共用——
见 [嵌入式运行与测试 agent](embedded-app-run.md)。

两个对编排方很重要的 z42b 旗标：

- `--list`：只打印目标名（一行一个），不编不跑。xtask 靠它拿单元清单后并行拉起
  `z42b test <toml> --name <unit>`，**从此不必自带第二套发现规则**（两套规则各自漂移曾是真 bug 源）。
- `--reuse-parent`：父包 dist 已在就不重建。进程内编译器无增量，不带这个旗标时 N 个目标会把父包
  全量重编 N 遍——编排方的做法是先不带它跑一个目标把父包建好，其余目标并行时都带上。

**执行模式不是 runner 的参数**：runner 与被加载的测试函数在同一个 VM、同一模式下跑，模式由承载
z42b 的 `z42vm --mode <mode>` 决定。所以"stdlib 测试在 JIT 下跑"就是拿 `--mode jit` 的 z42vm 跑 z42b，
不需要 per-test fork。

## 6. GREEN gate 的组成

`xtask test`（不带子命令）= `_testAll`，顺序执行、**首错即停**。当前 14 个 stage：

1. `build wave (debug vm + regen)`
2. `e2e goldens (interp; jit → vm-jit-consistency)`
3. `e2e cross-zpkg`
4. `e2e multi-exe`
5. `stdlib [Test]`
6. `stdlib [Benchmark]`
7. `manifest targets ([[test]] + [[example]])`
8. `examples (learn book transcripts)`
9. `docs (relative links)`
10. `compiler`
11. `gc modes (z42c.semantics build)`
12. `vscode-syntax`
13. `lines`
14. `walkers`

build wave 也算一个 stage：它同样走 `_stageStart` 打 banner、同样计入耗时表，对读日志的人就是一个 stage。

### 6.1 清单本身有门禁

这份清单历史上被复列在五六处、互相漂移；后来把它收敛成一处"唯一权威清单"的文档页，
结果连那页自己也烂了——先后三次加 stage（multi-exe、manifest targets、examples）都没同步，
文档停在 6 个而 gate 实跑 9 个。

结论写进了代码：**纯纪律守不住一份没有测试盯着的清单**。现在 gate 名单是数据
（`_gateStageNames()`，代码侧唯一 SoT），`_checkGateStageDoc()` 在**构建波之前**（几毫秒）
把它与开发基础设施那页 test-gate 文档里 `<!-- gate-stages:begin/end -->` 区的
`` - `name` `` 行**逐项比对**，不一致就打印双方清单后返回 1。

> **加/删/改名一个 stage 的正确姿势**：改 `_gateStageNames()` + 改那份文档的 gate-stages 区，
> 两处一致才绿。只改一处 → 本门变红。对账放在最前面是刻意的：文档漂移不该让人等十几分钟的
> gate 跑完才发现。

### 6.2 `--skip`：让 CI 拆解这块整料

`xtask test all --skip stdlib,cross-zpkg` 按名跳过 stage。CI 用它给每条 leg 减负——
被跳掉的 stage 由专门的并行 job 覆盖（见 §7）。本地 `xtask test` 传空串 → 全量 gate，不变。

## 7. CI 拓扑（谁跑哪一段）

真实 job 名（`.github/workflows/ci.yml`）：

| job | 显示名 | 跑什么 |
|---|---|---|
| `build-and-test` | `test-host(<platform>)` | 4 个 OS 各跑一遍 gate，`--skip stdlib,cross-zpkg` |
| `stdlib-interp-consistency` | `test-stdlib-interp(<platform>)` | 3 个 OS 各跑**全量** stdlib `[Test]`（interp，不分片） |
| `stdlib-jit-consistency` | `test-stdlib-jit(linux-x64) shard k` | stdlib `[Test]` 在 `--mode jit` 下，2 分片 × `--jobs 4` |
| `vm-jit-consistency` | `test-vm-jit(linux-x64) shard k` | golden 的 JIT pass，2 分片；消费 `assemble-current-sdk` 编好的 `.zbc`，`--no-rebuild` |
| `test-desktop` | `test-desktop-cabi(linux-x64)` | desktop Tier-1 C ABI 冒烟（R1–R7） |
| `test-wasm` / `test-ios` / `test-android` | 各带 `shard k` | Tier-2 平台语料，见 [跨平台测试](cross-platform.md) |

两条非显然的门控规则：

- **JIT 的 leg 要看 compiler 的改动，不只看 vm**。编译器的优化 pass（inline / LICM / CSE…）
  会重构 IR，于是 JIT 的**输入**变了、它对新 IR 的 lowering 才被跑到。test-host 的 interp golden
  只覆盖新 `.zbc` 的解释执行，**不覆盖**它的 JIT lowering。历史上有一次 opt-pipeline 改动跳过了
  这条 leg，优化后的 IR 从没被 JIT 测过。
- **Tier-2（浏览器 / 模拟器 / 真机模拟器）是 nightly-only + 手动**，不进每个 PR；
  Tier-1 的 `test-desktop` 仍按 `platform` 变更过滤器进 PR。

## 8. 在哪改

| 要改什么 | 改哪 |
|---|---|
| TIDX 加字段 / 改语义 | `src/runtime/src/metadata/test_index.rs` + 编译器 emit 侧 + [zbc 格式](../formats/zbc.md)（**要 bump section version**） |
| 一个 attribute 的运行期行为 | `src/libraries/z42.test/src/Runner.z42` |
| 一个 attribute 的编译期校验 | `src/compiler/z42c.semantics/.../DeclEnforcer.z42` |
| 报告字段 | `src/libraries/z42.test/src/TestReport.z42`（json）+ `Runner._runOne`（pretty） |
| benchmark 统计形状 | `Bencher.printSummary` **和** `BenchStats._parseLine`，同一个 commit |
| 加载/调用能力 | `src/runtime/src/corelib/reflection/module_load.rs` + `builtin_table.rs` 登记 |
| gate 组成 | `_gateStageNames()` **和** test-gate 文档的 gate-stages 区 |
| 单元编排 / 并行 / 分片 | `scripts/test/xtask_test_lib.z42`、`scripts/cli/xtask_cli_test.z42`（命令面） |
