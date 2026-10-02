# 测试怎么跑

> 对齐：2026-10-02（change `add-test-layout-spec`）｜ 代码：`scripts/test/`、
> `scripts/cli/xtask_cli_test.z42`、`src/tests/`、`src/libraries/<lib>/tests/`、`src/runtime/src/*_tests.rs`
>
> 用例放哪、写成什么形态、平台能力怎么声明见[测试用例组织规范](test-layout.md)（唯一权威）；
> gate 由哪些 stage 组成、`--skip` 与 `test changed` 的清单见[测试门禁](test-gate.md)（唯一权威）；
> 一份测试是怎么被跑起来的见[测试流水线](test-pipeline.md)；CI 把 stage 摊到并行 job 见
> [CI 拓扑](ci.md)；`[Test]` 属性怎么写、`z42 test` 怎么用见
> [参考手册](https://z42-lang.github.io/z42/reference/toolchain/cli-z42c-z42b.html)。

这页回答三个问题：有哪几层测试、各层**单跑时**有哪些旗标、**我改了 X 该验什么**。

## 1. 四层 + 一个门禁

| 层 | 测什么 | 命令 |
|---|---|---|
| 编译器 | z42c 自编自的**不动点**（gen1 == gen2 逐字节）+ 编译器源码里的 `[Test]` unit | `xtask test compiler` |
| VM golden | `src/tests/**/source.z42` 端到端，interp + JIT 双模 | `xtask test e2e` |
| stdlib `[Test]` | `src/libraries/<lib>/tests/`，由 `z42b` 调度 | `xtask test stdlib [<lib>]` |
| Rust VM 单测 | `src/runtime/src/*_tests.rs` + `src/runtime/tests/*.rs` | `xtask test runtime` |

```bash
./xtask test          # 完整 GREEN gate（串联全部 stage，任一失败立刻停）
```

裸 `test` 先跑一个重建波（debug VM + stdlib + z42c 自建 + golden `.zbc` 基线），
`--no-build` 跳过它、消费已有产物。**Rust VM 单测不在 gate 内**——它的
signal-crash helper 会挂死整套 `cargo test`，所以 CI 每条腿单列一步。

> `xtask test all --help` 里 `--skip` 只列了四个 stage 名，实际接受十二个；
> 以[测试门禁 §5](test-gate.md) 的清单为准。

除四层外，gate 里还串着若干**门禁型** stage，各自也能单跑：

| 命令 | 守什么 |
|---|---|
| `xtask test docs` | 文档里的相对链接都解析得了（有 baseline 棘轮，`--update` 只许减不许加） |
| `xtask test lines` | 单文件行数硬上限（>500 行：新增/增长判红，已知的只警告） |
| `xtask test walkers` | 注册的 exhaustive AST walker 覆盖每个节点子类 |
| `xtask test vscode-syntax` | VSCode 语法文件 ↔ Lexer 关键字表同步 |
| `xtask test fingerprint` | 编译器输出变了就必须 bump `CompilerFingerprint` / 格式 minor |
| `xtask test incremental` | 逐文件 touch，增量结果与全量**逐字节**相同 |
| `xtask test targets` | manifest 的 `[[test]]` / `[[example]]` target fixture |
| `xtask test examples` | 学习手册的会话脚本用真实 SDK 重放 |
| `xtask test packages` | `packages.toml` 的解析 / 组件安装 / 发布归档自检 |
| `xtask test bootstrap [rid]` | 自举边界：上一 nightly 的 z42c 能不能编当前 z42c 源 |

## 2. 各层的单跑姿势

### VM golden

```bash
./xtask test e2e                       # 默认重建 + interp & jit 双模跑
./xtask test e2e --mode interp         # 只一种模式
./xtask test e2e --dir cross-zpkg      # 只跑一个类别
./xtask test e2e --file <name>         # 只跑一个用例（名字或路径子串）
./xtask test e2e --no-rebuild          # 跳过重建波
./xtask test e2e --shard k/n --jobs 4  # CI 分片 + 并行
./xtask build test                     # 只重生 golden .zbc，不跑
```

默认重建是有意的：不强制刷新依赖产物时，「stdlib zpkg 旧 / golden zbc 旧」会让测试结果对当前
代码不真实——假绿和假红都出过。`--no-rebuild` 只在**确认上一次重建是新的**、且正在反复迭代
同一个用例时用。

单跑一个已编好的 golden 可以直接调 VM：

```bash
./artifacts/build/runtime/release/z42vm src/tests/<category>/<name>/source.zbc --mode jit
```

**cross-zpkg**（`src/compiler/z42c.pipeline/tests/fixtures/cross-zpkg/<name>/`）是 golden 的一个类别，验多 zpkg 协作：
驱动按 `target/` → `ext/`（依赖 target）→ `main/`（依赖两者）编译，全部 zpkg 放进 `libs/`，
再用 VM 跑 main 入口。覆盖跨包类型解析、`impl Trait for Type` 传播、跨包泛型实例化与
interface dispatch、`using` 的跨包 namespace 解析、同名 namespace 冲突。

### stdlib `[Test]`

```bash
./xtask test stdlib                    # 全部带 [Test] 的库
./xtask test stdlib z42.numerics       # 单库
./xtask test stdlib z42.text -k <name> # 再按测试名过滤
./xtask test stdlib --jobs 4           # unit 级并行批宽
./xtask test stdlib --mode jit         # JIT 下跑
./xtask test stdlib --no-build         # 消费已有工具链
./xtask test stdlib --vm <p> --driver <p> --libs <p>   # 完全用外部预建产物
```

**什么算一个 unit**：`tests/` 下的单个 `.z42` 文件，或者一个**目录内任一 `.z42` 声明了
`[Test]`** 的目录（目录里 `**/*.z42` 全部编进同一个 unit）。目录判据**只看有没有 `[Test]`**，
与入口文件叫什么无关。含 `source.z42` 但只有 `void Main()`、没有 `[Test]` 的目录不是 unit，
而是这个库的 golden 用例，由 `test e2e` 跑。

> 「发现」和「列表」用的必须是**同一条**判据，否则失配是静默的：`test list` 会照常列出一个
> 目录，`test stdlib <lib>` 却报 "all 0 file(s) passed"——绿得毫无破绽。守这一点的门是：
> **点名某个库、其 `tests/` 下有 `.z42` 源却发现不到任何 unit → 直接判红**
> （无名扫全库、以及「库根本没有 `tests/`」不受影响）。

**`--mode jit` 有个副作用**：in-process runner 硬编码走解释器，所以 `--mode jit` 会强制退化成
**subprocess fork**（每个 test 一次 `z42vm --mode jit`），而 fork 形态下 `[Setup]` / `[Teardown]`
**不执行**。CI 的 `test-stdlib-jit` 腿跑的就是这条路，用途是抓 stdlib 的 interp / JIT 分歧。
两种执行形态的机制见[测试流水线](test-pipeline.md)。

xtask 的 `--jobs N` 是**上层 unit 级**批宽（每批 N 个 unit 同时 compile + run，重叠掉每个 unit 的
`z42.core` bootstrap，这是 stdlib 测试的主要耗时），与 runner 自己的 `--jobs` 不是一回事；
每个 runner 在其 unit 内仍串行，`[Setup]` / `[Teardown]` 因此正常执行。`test all` 默认传 4。

更细的 runner 旗标（`--format pretty|json`，格式契约见[测试门禁 §8](test-gate.md)）直接对
`z42b`：`z42b <unit>.zbc --format json`。

### 编译器 / Rust VM

```bash
./xtask test compiler                                      # 不动点 + [Test] unit
./xtask test runtime                                       # cargo test（串行化）
cargo test --manifest-path src/runtime/Cargo.toml <substr>  # 按名过滤
```

CI 只在 Windows 腿跑 `cargo test`，容易静默腐烂——改 ClassDesc / 反射 / 版本相关代码后
**本地必跑**；版本 bump 还要更新 `zbc_reader_tests` 里的 version-pin 测试。

### `test fingerprint`：本地怎么给它一棵 base 树

这道门要两棵树（本树编译器重编 **base 的** stdlib 源码，逐包比字节），过去多个 change 的
tasks.md 都记着「本地无从提供 base 树」而只能等 CI。其实当 **nightly release 正好发自
当前 origin/main** 时很便宜 —— 先对一下：

```bash
gh release view nightly --json targetCommitish,publishedAt   # == origin/main HEAD？
```

相等就可以铺一棵：

```bash
BASE=../z42-base
git worktree add $BASE --detach <那个 sha>
# nightly SDK 当冷种子（解包在 $SDK）
mkdir -p $BASE/artifacts/build/{libraries/dist/release,compiler/z42c.driver/release/dist} $BASE/.seedvm
cp $SDK/libs/*          $BASE/artifacts/build/libraries/dist/release/
cp $SDK/programs/z42c/* $BASE/artifacts/build/compiler/z42c.driver/release/dist/
cp $SDK/bin/z42vm       $BASE/.seedvm/z42vm && chmod +x $BASE/.seedvm/z42vm
cp -R <warm>/.z42 $BASE/.z42 && cp <warm>/xtask $BASE/xtask      # 种子 + apphost
# xtask.zpkg 用 SDK 自带的 z42c 现建（**别**用 .z42/bin/z42c，见 bootstrap-seed 的格式墙）
(cd $BASE && Z42_LIBS=$PWD/artifacts/build/libraries/dist/release ./.seedvm/z42vm \
   artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg \
   -- build scripts/xtask.z42.toml --release)
# 🔴 关键一步：**必须真跑一遍 build stdlib**
(cd $BASE && RUSTUP_TOOLCHAIN=1.98.1 Z42_PORTABLE_VM=<本树>/artifacts/build/runtime/release/z42vm \
   ./xtask build stdlib)
```

然后在本树 `./xtask test fingerprint --base $BASE`。

两个容易踩的：

- 🔴 **只把 flat libs 铺进 `artifacts/build/libraries/dist/release/` 不够** —— 那是**运行期**
  libs，而门比的是 **per-member dist**（`artifacts/build/libraries/<pkg>/release/dist/<pkg>.zpkg`）。
  漏了这步，门会明确报「只有一侧有产物，无从比对」并拒绝出结论
  （此前它会把这种输入缺失静默报成「N 个包输出变了，请加 slug」—— 25/25 全变，
  非常像真的）。
- ⭐ `Z42_PORTABLE_VM` 可以直接借**本树刚建好的** z42vm：VM 与源码树无关，同 sha 下等价，
  省掉 base 树一次 ~10 分钟的 cargo 全量。（但**别拷 `artifacts/build/runtime/` 目录本身** ——
  里面 `-sys` crate 的 `CMakeCache.txt` 烤死了绝对路径。）

## 3. 用例放哪

「被测对象在哪，测试就在哪」+「中央 VM e2e 按特性分类」。

| 目录 | 形态 | 谁运行 |
|---|---|---|
| `src/tests/<category>/<name>/` | `source.z42` + `expected_output.txt` | `xtask test e2e` |
| `src/compiler/z42c.pipeline/tests/fixtures/cross-zpkg/<name>/` | target / ext / main 三个 toml 工程 | `xtask test e2e --dir cross-zpkg` |
| `src/tests/{zbc,zpkg}-format/<name>/` | 入库的 `.zbc` / `.zpkg` 字节基线 | `xtask test runtime`（`git diff` 即格式漂移探针） |
| `src/bench/` | 计时场景 | `xtask bench` |
| `src/compiler/z42c.<member>/tests/` | 按阶段分（lexer / parser / decl / stmt / dump…） | `xtask test compiler` |
| `src/libraries/<lib>/tests/` | 顶层 `*.z42` 是 `[Test]`；`<name>/source.z42` 是 golden | `test stdlib` / `test e2e` |
| `src/runtime/src/<mod>_tests.rs` | Rust 单元 | `xtask test runtime` |
| `src/runtime/tests/*.rs` | Rust 集成（跨语言契约 / native e2e） | `xtask test runtime` |

**加新用例往哪放**（先到先得）：库 API 行为 → 该库的 `tests/`；编译器 pipeline 单元 →
`src/compiler/z42c.<member>/tests/`；VM 内部（GC / interp / decoder）→ `*_tests.rs`，
跨语言契约 → `src/runtime/tests/`；跨多 zpkg → `src/compiler/z42c.pipeline/tests/fixtures/cross-zpkg/`；其余语言 / VM 特性 e2e
→ `src/tests/<category>/`（拿不准先归 `basic/`）。

判据是**这条断言在描述谁的契约**：测 `String.Trim` / `Enum.Parse` / `List<T>` 的行为就写在
那个库的 `tests/`，哪怕它由 VM builtin 实现；测语法、派发、GC、优化 pass 就写 `src/tests/`。

golden 用例的文件约定：

| 文件 | 含义 |
|---|---|
| `source.z42` | 必须 |
| `source.zbc` | 由 `xtask build test` 生成，**镜像到 `artifacts/`**（gitignored）；唯一例外是 `{zbc,zpkg}-format/*` 的入库基线，就地重写 |
| `expected_output.txt` | stdout 期望；**空或缺失 = 靠内置 `Assert.*` 自验**（跑通无输出即过）。强制 LF，见[开发环境](dev-setup.md#windows) |
| `interp_only` | 空文件 marker：JIT 模式跳过该用例 |
| `opt_all` | 空文件 marker：用 `--emit-zbc --opt-all` 编。默认 emit 优化集关掉了 StackAlloc / Inline / PureCall / DeadBranch / Devirt（开了会改 golden 字节），**不挂这个 sidecar 的用例一条优化 pass 都走不到** |

> 🔴 **`opt_all` 的覆盖面缺口是「类目性」的，不是「数量性」的**：到 2026-09-27 之前全仓 11 个
> `opt_all`，其中 10 个在 `optimization/`、1 个在 `closures/` —— 也就是说
> **`types/` · `generics/` · `classes/` · `interfaces/` · `cross-zpkg/` 这些「语言特性」类目一个都没有**。
>
> 这个分布看着合理（「优化的用例归优化类目」），但它测的是**pass 本身**，形状是为触发 pass 挑的；
> 「一个带 struct 字段的普通类」这种日常形状不在其中。于是 `--release` 下的对象表示对特性类目
> **整体不可见**。给 3 个泛型 fixture 补挂之后**当场炸出一个 release 必崩的真 bug**
> （`fix-stackobj-inline-struct-leaf`：栈分配对象上的内联 struct 字段读写，interp 崩、jit 正常）。
>
> 挂 `opt_all` 的成本是一个空文件 —— **写涉及对象分配 / 字段布局 / 调用约定的用例时默认挂上，
> 不要因为「这不是优化用例」就不挂**。
>
> ⚠️ 数 `opt_all` 有两种拼写（dir 形态 `opt_all` / flat 形态 `<name>.opt_all`）。
> 我第一次只数了前者、得出「只有 2 个」并写进了 PR，**是错的** —— 判据只认一种拼写。
>
> ✅ **2026-09-27 已铺开**（change `fix-inline-breaks-ref-params`）：19 个特性类目全挂，
> `opt_all` 从 **11 → 267**。这一个动作**炸出两个既存的 release-only 崩溃**，都在仓库自己的
> 测试语料里、都是 debug 全绿：
>
> | 元凶 pass | 症状 | 影响面 |
> |---|---|---|
> | **Inline** | `ref` 形参的 `Value::Ref` 流进算术（7 个 `refs/*` 用例） | 任何被内联的带 `ref` 形参的函数 |
> | **Devirt** | 去虚化到不存在的 `Std.Type.GetType`（`types/value_type_object_methods`）| 值类型收者上的 `GetType()` |
>
> Inline 那条已修（见 [逃逸分析](../runtime/escape-analysis.md) 的 `ref` 调用点一节）；
> Devirt 那条**尚未修**，故 `value_type_object_methods` 是**唯一刻意不挂 `opt_all`** 的用例 ——
> 修好那条 pass 时**连它的 sidecar 一起加上**，别忘了。
>
> ✅ **第二轮铺完（`opt-all-round2`）**：余下 13 个类目，`opt_all` 268 → **352**（32 个类目）。**这轮只红 1 个、而且是断言失败不是崩溃** ⇒ 前两轮已把优化相关的崩溃基本清完。
>
> 那 1 个（`gc/gc_oom_exception`）**不是编译器 bug，是用例的前提错了**：它 `try { var obj = new BigObj(42); }` —— `obj` 从不被用、不逸逸 ⇒ 全优化下被**栈分配**，一个字节都不碰 GC 堆 ⇒ strict OOM 不触发。**没人承诺「每个 `new` 都落 GC 堆」** —— 不逸逸对象绕开 GC 正是栈分配的目的。⇒ 已改成强制堆分配（存进数组元素 = 逸逸汇点）。
>
> ⚠️ **写 GC / 分配行为的用例时要问：我要测的那次分配，在全优化下还存在吗**？不逸逸就没了。汇点表见 [逸逸分析](../runtime/escape-analysis.md)。
>
> ⭐ **这就是「先补测量，再谈重构」的样本**：两个崩溃的修复成本远小于发现它们的成本，而发现
> 它们只需要 253 个空文件。

单文件形态（`<category>/<name>.z42`）的 marker 写成同名前缀：`<name>.interp_only` / `<name>.opt_all`。

查用例元数据不必读源码：

```bash
./xtask test list [--dir <bucket>] [--filter <kw>] [--kind golden|test] [--json]
```

它把「用例名 → 运行命令 / interp·jit / 声明的能力要求（`requires`）」列成表；缺能力的平台上，
设备上的 runner 会把这些用例记为 skipped（见[跨平台测试 §4](../testing/cross-platform.md)）。

## 4. 缩窄迭代范围

下面三种手段都**不构成 GREEN**，commit 前必须跑完整 `xtask test`。

| 手段 | 命令 |
|---|---|
| 按 `git diff` 挑 stage | `xtask test changed [base] [--dry-run]` |
| 单跑一个用例 / 类别 / 库 | `xtask test e2e --file <name>` / `--dir <cat>` / `xtask test stdlib <lib> -k <name>` |
| 跳过重建波 | `--no-build`（或 golden 那边的 `--no-rebuild`） |

标准姿势：先完整跑一次 `xtask test`（或 `build`）备好工具链，之后
`xtask test e2e --file <name> --no-build` 反复迭代——只重编 + 跑那一个用例。

`test changed` 的完整路径→命令映射见[测试门禁 §7](test-gate.md)。用它之前要知道三件事：
base 默认 `HEAD`，也可以给 ref 或用 `Z42_TEST_CHANGED_BASE`；收集范围是
`git diff --name-only <BASE>` 的 tracked 改动加 `git ls-files --others --exclude-standard`
的 untracked；它不理解跨文件的语义依赖（改 stdlib 内部 helper 不会触发依赖它的 cross-zpkg 用例），
靠「未识别路径一律坍缩为 `test all`」保守弥补。

## 5. 我改了 X，该验什么

通用前提：commit 前完整 `xtask test` 全绿。下表「快速迭代」列不构成 GREEN。

| 改动 | 快速迭代 | commit 前额外必跑 | CI 替你验的 |
|---|---|---|---|
| **编译器 `src/compiler/`** | `test changed` | 触及 lexer / parser / codegen / 格式 writer，或源里用了新写法 → `test bootstrap` | `compiler-checks`、`test-host`（含种子自举边界）、`test-vm-jit` |
| **stdlib（加 API / 改实现）** | `test stdlib <lib> -k <kw>` 或 `test changed` | — | `test-stdlib-interp` ×3 OS、`test-stdlib-jit` ×2 shard、`test-host` |
| **stdlib（删 / 改 xtask 或 z42c 在用的 API）** | 同上 + 迁调用点 | ⚠️ 两步舞，见 §6 | 每腿 `ci-bootstrap`（用**种子** stdlib 编 xtask / z42c 源） |
| **VM `src/runtime/`** | `xtask test runtime` + `test e2e` | — | `test-host`、`test-vm-jit`、`test-stdlib-*`、`package-*`（feature 组合）|
| **只改用例 `src/tests/`** | `test e2e` | — | `test-host`（`test-vm-jit` / `stdlib-*` 不跑） |
| **xtask 源 `scripts/`** | `z42 publish scripts/xtask.z42.toml` 重建后随便跑条命令冒烟 | changed 映射对 `scripts/**` = 全套 | 每腿 `ci-bootstrap` 的种子编 xtask 步 |
| **新语法 / zbc·zpkg 格式** | 阶段一只落 support（仓库源码不用）→ `test bootstrap` | 格式 bump 另跑 `docs/agent/rules/version-bumping.md` 的清单；等 nightly 发布后才 use | 全腿 `ci-bootstrap`（种子编当前源）+ `compiler-checks` |
| **打包 `scripts/package/` / `packages.toml`** | `test packages` | `xtask package sdk` + `xtask test dist` | `package-host` + `package-{ios,android,wasm}` |
| **codegen / 优化 / typecheck / IR writer（会改产物字节）** | `test compiler` | 同步 `CacheStore.CompilerFingerprint` +1；自查 `test fingerprint` | `bench-regression` 的 fingerprint guard |
| **增量编译（IncrementalBuild / CacheStore / ZbcReader）** | `test compiler` | `test incremental`（逐文件 touch 对账，增量 == 全量逐字节） | `compiler-checks` |
| **学习手册 / `examples/`** | `test examples <part>/<chapter>`（只改页面加 `--book-only`）；输出确实该变则 `--bless` 后审 diff | `xtask build sdk` + `test examples` | `test-host` 的 examples stage、`package-host`（用打包 SDK 重放，含 Windows）、`deploy-book` |
| **launcher / z42b 的命令行输出** | `test examples`（手册会话脚本记录了这些输出） | 同上 | 同上 |
| **纯文档 / `.claude/`** | 无 | 无 | `.claude/**` 不触发；`docs/**` 会触发 `test docs` 死链门 |

## 6. 自举边界：什么会断链

CI 每条腿冷启动时，**xtask 源和 z42c 源都由「上一 nightly 的种子 z42c + 种子 stdlib」编译**。
这把这两个源码域钉死了两根轴：

1. **语法 / 格式轴** —— 不得用比上一 nightly z42c 更新的语法；
2. **stdlib API 轴** —— 不得引用上一 nightly stdlib 里不存在的 API。

stdlib 源自身不受种子约束（它由自建的当前 z42c 编译）。所以加 API / 改实现是最常见也最轻的
情形，不需要任何边界动作；新 API 想被 xtask / z42c 源使用，等它随 nightly 发布之后。

**删 / 改 xtask 或 z42c 在用的 API** 要走两个 nightly 周期的剧本：

```bash
grep -rn "GetSize" scripts/ src/compiler/     # 第 0 步：判定是否踩边界（以改名 GetSize → Size 为例）
```

无命中就按普通情形处理。有命中则分两阶段，**中间必须隔一次成功的 `publish-nightly`**：

- **阶段一（commit A）**：stdlib 加新 API，**旧 API 原样保留**（这是「不留兼容」规则的种子例外，
  只为跨一个 nightly），`scripts/` 与 `src/compiler/` 的调用点**一律不改**。跑完整 gate、push。
  然后**硬等待**新 nightly 发布：

  ```bash
  gh run list --workflow=CI --branch=main -L 1     # 本次 CI 全绿
  gh release view nightly --json publishedAt       # publishedAt 晚于 commit A 的 CI 完成时间
  ```

- **阶段二（commit B）**：调用点全切到新 API，**同一个原子提交里删掉旧 API**。先
  `xtask test bootstrap`（绿 = 新种子已含新 API，z42c 源切换安全）再完整 gate。

踩线的症状：阶段二在 nightly 发布前 push → 所有腿在 bootstrap 阶段红（种子 stdlib 没有新 API），
`publish-nightly` 因 `needs` 不满足而不发布，种子链不被污染。处置是 revert commit B、
等 nightly 滚过去再重来，**不要**试图往种子里手补。不要与 zbc/zpkg 格式 bump 排在同一个
nightly 周期——两个断链窗口叠加。

`xtask test bootstrap [rid]` 是这条边界的本地快门（两轨对照的判定逻辑见
[构建编排 §9](build.md)）。注意它**只编编译器成员、不编 xtask 源**——xtask 侧的越界目前只能
靠 CI 冷启动兜底。

## 7. 平台测试与嵌入 corpus

```bash
./xtask test platform <desktop|wasm|ios|android|all> [build|assets|run]
```

三阶段：`build` 造平台原生工程（apphost / wasm-pack / xcframework / AAR）；`assets` 编
fixture `.zbc` + 收 stdlib zpkg 进平台 bundle；`run` 跑测试（C ABI harness / Playwright /
`xcodebuild test` / emulator）。省略 step 就是三段全跑。平台测试**不在** GREEN gate 里
（各需重型工具链），CI 各平台独立 job。从零的本地配方见[平台构建与嵌入](build-platforms.md)。

```bash
./xtask test embedded [--rid <rid>] [--case <name>|--filter <kw>|--shard k/n] [--format json|pretty]
```

`test embedded` 把 `src/tests` golden + stdlib `[Test]` 汇成一个 bundle，穿过**嵌入**的 VM 跑。
用例源码声明的 `// requires-caps:` 随 bundle 下发，目标 VM 缺其中任一能力时该用例记为 skipped、不加载；
`test list` 能先查每个用例声明了什么。两种覆盖模式：

- **默认（无 `--shard`）**：带 cap 的类别 round-robin 采样——每个类别都有代表用例、不偏向字母序
  靠前的类。定位是「验嵌入执行路径通不通」，本地/ 手动快速跑。
- **`--shard k/n`**：不设 cap，取能力门控后的第 k/n 片（`index%n` 分区，n 片并集 = 全集、
  零重叠、确定）。受限平台的单个 job 有时间墙（wasm 的 Playwright、android 的 emulator），
  提 cap 会撞墙，分片把编译 + 跑摊到 n 个 runner 上，墙不动而覆盖到 100%。CI 的
  wasm / iOS-sim / android-emu 三条 nightly 腿都是 `--shard k/3`。

本地验分片切分：`xtask test embedded --rid iossim-arm64 --shard 1/4`（及 2/4…）看报告里的
selected 数。
