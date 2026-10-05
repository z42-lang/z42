# 编译器自举（self-hosting）— `src/compiler/` 架构

> **页型**: 机制页 ｜ **代码**: `src/compiler/` · `scripts/build/xtask_compiler.z42` · `scripts/build/xtask_bootstrap_check.z42`
> **相关**: [架构总览](architecture.md) · [源代码编译流程](source-compile.md) · [构建编排](../devinfra/build.md) · [自举种子纪律](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/bootstrap-seed.md)

z42c 是用 z42 写的编译器，由上一代 z42c 编译自己。结论：**种子（上一个 nightly SDK）→ 破环预建自依赖库 → 自建 z42c → 用 fresh z42c 建 stdlib**；z42vm 由 Rust 构建，是打破格式环的锚点。

## 目录布局

编译器域的全部包在 `src/compiler/` 这一个 workspace（`z42.workspace.toml`，`members = ["*"]`），产物在 `artifacts/build/compiler/<pkg>/<profile>/{dist,cache}/`；`src/libraries/` 只放用户 stdlib（`Std.*`）。目录名 == `[project].name` == zpkg basename。

| 包 | kind | 内容 | 编译器域内依赖 |
|---|---|---|---|
| `z42c.core` | lib | Span / Diagnostic / Features | — |
| `z42c.syntax` | lib | Lexer + Parser + AST | core |
| `z42.package` | lib | IR 模型 + zbc/zpkg 读写后端 | — |
| `z42.project` | lib | 工程清单模型 | — |
| `z42.build` | lib | 构建管线接口（`ICompiler` / `IReplCompiler`） | project |
| `z42c.semantics` | lib | TypeCheck + Codegen | core, syntax, package |
| `z42c.pipeline` | lib | 编排（workspace 构建、依赖扫描、缓存）| core, syntax, semantics, package, project, build |
| `z42c.driver` | exe | `z42c` 命令入口 | pipeline, semantics, syntax, core, package, project |
| `z42.scripting` | lib | eval 内核 | core, syntax, build |

各包另依赖 stdlib（`z42.core` / `z42.io` / `z42.toml` 等，自动可用）。`z42.package` / `z42.project` / `z42.build` / `z42c.core` / `z42c.syntax` 与 `z42.core` 合称**自依赖库**：z42c 运行期与编译期都要用它们，而它们又由 z42c 构建（见轴 ④）。普通工程的解析域只有 shipped `libs/`，要用编译器域库须在 `[dependencies]` 按名声明。

依赖解析：成员从**当前 workspace** 解析 toml 声明的兄弟依赖，与输出位置无关；除 stdlib 外的依赖必须在 `[dependencies]` 声明。远程依赖（registry / git URL）见 [Deferred](#deferred--future-work)。

## 受限写法约定

用当前语言子集写，遇到无法表达的写法：停下汇报 → L1/L2 可补则当次补进语言；**禁止在编译器代码里写 workaround**。

| 限制 | 受限写法 |
|---|---|
| 类字段的泛型参数被 parser 丢弃（`private List<X> f;` 取元素退化为 `<unknown>`，无法调其方法）| **typed array + count**：`Diagnostic[] _items; int _count;` + 手动 `Grow()`，故编译器内集合一律是并行数组 |
| `new T[n][]` 不能创建交错数组 | 平铺单数组 + 偏移，或平行数组 + 线性查（如 `WsMembers.DepFlat/DepOff/DepLen`）|
| AST / IR 节点 | `class` 继承层级 + `virtual` / 抽象 `Visitor` 基类 dispatch |
| 错误路径 | `throw` / `try-catch` + Exception 子类 |

`TokenKind` 等「枚举」是 `static class` + `int` 常量。

## 构建

`./xtask build compiler` = `_buildCompiler`（`scripts/build/xtask_compiler.z42`）：确保种子（`_ensureSeed`）→ 建 z42vm → 必要时先建 stdlib → `_buildCompilerViaZ42c` 用种子 driver 跑 `build --workspace --release`。命令面见 `./xtask --help`。

**z42c driver 自有 `build --workspace`**（`z42c.pipeline/src/WorkspaceBuild.z42`）：

- **成员发现 + 拓扑序**：`[workspace] members`（缺省 `["*"]`，段内通配）展开、`exclude` 剔除；读各成员 `[project].name` + `[dependencies]`，只保留指向 workspace 内成员的边；层式拓扑，层内按 name Ordinal 排序，环抛异常。
- **显式 `--output-dir <flat>`**：全成员产物落该 flat 目录，兼作各成员的兄弟解析目录。
- **无 `--output-dir`**：按 `[workspace.build].output_dir` 模板展开 per-member 布局，产物落各自 `<output_dir>/dist`；兄弟解析扫全成员 dist + 外部档（`Z42_LIBS` / `--compile-libs`）。先建全部成员 dist（空目录）再按拓扑序逐个 build。
- **解析分档**（`WsTier`）：成员 dist 只回答**成员**包名，外部档只回答**非成员**包名；拓扑序在后的成员整个不可见（`Hidden`）。成员 dist 同时是 exe 的运行期载荷目录（`z42c build` 会把依赖闭包 colocate 进去），里面的外部包副本是载荷不是解析源；`build stdlib` 的外部档是 flat（全成员 dist 的聚合），里面有每个成员上一轮的副本——分档防止这两种旧副本遮蔽本轮 fresh 产物。

**stdlib 由 z42c 构建**：`xtask build stdlib` 用 z42vm `--mode interp` 跑 `z42c.driver.zpkg build --workspace --release`，per-member 落 `artifacts/build/libraries/<name>/<profile>/dist/`，再 verify + 装配 flat view。整个 stdlib 构建同时是 z42c 的 dogfood。

### 运行 z42c 的库目录

运行期 `Z42_LIBS` 是**单个目录**（非 colon-list），传多个会被当成一个非法路径，回退成仅 stdlib，表现为 `VCall: function not found` + 静态字段读 null。`z42c build` 编 exe 时把非 stdlib 依赖复制进输出 dist（自包含），所以 `z42c.driver` dist 自带它依赖的编译器包，跑它时 `Z42_LIBS` 只需 stdlib。xtask 的单元测试经 `Z42_PROBING_PATHS` 拿各编译器成员 dist，见 [产物布局](../devinfra/artifacts-layout.md)。

## CLI

`z42c.driver` 提供 `--dump-keywords` / `--dump-tokens` / `--dump-ast` / `--dump-bound` / `--dump-ir` / `--emit-zbc <src> <out>` / `build [<manifest>]`（含 `--workspace`、`--output-dir`、`--compile-libs`）。完整列表见 `z42c --help`。

## .zbc 写入器（`z42.package/src/BinaryFormat/`）

- **架构**：`ByteWriter`（`int[]` 0..255 + LE 助手，规避 byte 型）→ `ZbcInstr`（集中 if-is 编码）→ `ZbcWriter`（intern 预扫 + 分段组装）；`TokenAllocator`（插入序 index）；`ZbcStringPool`（插入序 = STRS 字节序）。格式规格见 [zpkg](../formats/zpkg.md)。
- **确定性铁律**：字符串池 intern 序须固定（模块名 → const.str 池 → 类 → "?" → 每函数[名/ret/param → 每块 label→指令串]）；IMPT 写前 Ordinal 排序。
- runtime builtin 在 z42 侧直接 `[Native("__double_to_bits")]` 自声明。

## 自举：环与破环

### 环在哪

```
z42vm (Rust)  ──cargo 建，非自举──►  锚点（永远能产「懂当前格式」的 VM）
z42c / stdlib / xtask (z42)  ──互为前置──►  自举环
```

环的本质：用 vN-1 的工具编 vN 的源，而 vN 源可能用了 vN-1 工具不懂的东西。拆成四轴：

| 轴 | 鸡蛋问题 | 怎么断 |
|---|---|---|
| ① 语法 | vN 源用新语法 → 上一代 z42c 编不了 | **纪律**：support 先行，晚一个 nightly 再 use |
| ② zbc/zpkg 格式 | 旧 z42vm 读不了新格式 | **锚点自动断**：新格式产物跑新建的 z42vm；`ci-bootstrap` 在版本差时做两代自举（旧 VM 跑 gen1/gen2 → 新 VM 接管）|
| ③ stdlib API | xtask / 源用新 API → 旧 stdlib 没有 | **纪律**（同 ①）；自依赖库例外，见轴 ④ |
| ④ z42c 自依赖库 | 自依赖库由 z42c 构建，冷启动 flat 里只有种子带来的旧版 | **破环预建**：自建前先用当前 driver 把当前源的它们编进 flat |

格式轴不需要纪律；真正靠纪律约束的只有语法 / API 轴。

### 轴 ④ 的破环预建

`_ensureBootstrapSelfDepLibs`（`xtask_compiler.z42`）在 workspace 自建**前**，用当前 driver（冷启动 = 上一 nightly 种子）把**当前源**的 `z42.core` → `z42.project` → `z42.build` → `z42.package` → `z42c.core` → `z42c.syntax` 依次编进 flat（顺序即依赖序，先于其消费者）。理由：z42c 源若消费这些库的新 API 而 flat 里是种子带来的旧版，自建会 `unknown type` / `no field`；若 fresh z42c emit 的调用钉在旧库上，运行期加载真库时解析不到（`undefined function`）。

- **不 warm-skip**：「已在 flat 就跳过」等价于「z42c 不消费这些库的新 API」，不成立；源未变时增量缓存近零成本，源变了本就该重建。
- **推论**：对这 6 个自依赖库，「z42c 源用它们的新 API」由预建自动破环，可同 commit 加成员并消费，无需等 nightly。轴 ③ 的「晚一个 nightly」只约束**其余 stdlib 库**（`z42.collections` / `z42.threading` / …）与 **xtask 源**（xtask 用种子 SDK 编，不受预建保护）。
- **种子 ABI 残余约束**：给这些库的既有导出类型加字段时，新字段不得进构造函数签名——ctor 内给默认值，由消费方构造后赋值。
- 预建前先 `_breakHardLink`：flat 由硬链接装配，就地写会穿透到规范 dist。
- 自建完成后 `_purgeBootstrapPrebuildsFromFlat` 把非 stdlib 成员从 flat 清走（规范产物在各成员 dist），使 flat 保持「只有 stdlib」，否则开发树的 `Z42_LIBS` 里会出现 `z42.project` 等包被误判为框架包。
- 自建首遍失败（跨成员新增符号：driver 自包含的是旧兄弟包）时，用本轮 fresh 成员 dist 重新自包含 driver 并重试一遍。

### 运行期 libs 与编译期 libs 分离

自建这一步跑的是**上一代** driver：它运行期要加载那一代的 stdlib，而它编译的源码要解析**当前源**刚预建的库。两者挤在 `Z42_LIBS` 一个变量里，预建一覆盖 flat，种子 driver 自己就崩（追加 API 撞不到，改名 / 删符号必撞）。

| 面 | 谁读它 | 怎么给 |
|---|---|---|
| 运行期 | 被执行的 z42c 加载自己的依赖 | `Z42_LIBS`（+ zpkg 旁 colocated 副本优先）|
| 编译期 | z42c 为被编译工程解析依赖 | `--compile-libs <dirs>`（缺省回落 `Z42_LIBS`）|

自建步骤是 `Z42_LIBS=<种子代 run-libs>` + `--compile-libs=<flat>`；driver 换代后的步骤两者都用 flat。

- **旗标探测**：早于 `--compile-libs` 的种子 driver 会静默忽略未知旗标，故先 `_driverSupportsCompileLibs` 跑 `build --help` 认关键字，不认则退回共用 flat。driver 加载不到自己那代 stdlib 会直接崩（输出无旗标名，易被误判为「不支持」），所以做两次尝试取第一个真打出 `usage:` 的；两次都没有 = 探测失败，要出声（`_helpRan`）。
- **种子代 run-libs 不能取自 flat**：flat 正是要被预建覆盖的目录，快照晚于覆盖会拿到新代，种子 driver 即 `MissingSymbolException`。锚取 driver 自己 dist 里的 colocated 闭包（与 driver 同代是构造保证），**搬**进 `artifacts/build/compiler/seed-run-libs/<profile>/`（`_relocateSeedRunLibs`）；判据是「齐了吗」而非「搬到了吗」，缺口从 SDK libs 补（`_topUpSeedRunLibs`）。搬（而非拷）同时清掉成员 dist 里的外部包副本，避免它遮蔽 flat 里的新版。
- 该旗标自身也受种子纪律；`WsTier` 分档生效后，「搬」可退回「拷」。

## 分阶段流程与不变量

```
Stage 0  种子 = 上一个 nightly SDK（z42vm + z42c + stdlib）或本地 warm 产物
Stage 1  cargo → z42vm_N
         种子 z42c 编 xtask 源 → xtask_N                  ◄── INV-1：上版必须能编 xtask
         xtask_N 驱动：种子 z42c 编 z42c 源 → z42c gen1    ◄── INV-1：上版必须能编 z42c 源
                       gen1 编 stdlib 源 → stdlib_N
Stage 2  gen1 再编 z42c 源 → gen2；各成员 zpkg 除 BLID 段外逐段一致   ◄── INV-3：不动点
         全量 [Test] / [Benchmark]                          ◄── INV-2：测试全绿
```

xtask 最先被种子编出来、还要回头驱动编 stdlib / z42c，所以它只能用种子已有的语法与 stdlib API，INV-1 是最受约束的不变量。

**种子来源**：SDK package（`z42-sdk-<ver>-<rid>`）的 `programs/z42c/` + `libs/`，而非 runtime package（runtime package 是纯嵌入式运行时，可能跨 host 使用，不携带单一 host 的 z42c）。冷启动由 `_ensureSeed` 按 `Z42_HOME` → 运行 xtask 的 apphost SDK → `./.z42` 找到 SDK 并供种到 in-tree；warm 树不被覆盖（gen2 不动点靠「从 in-tree gen1 再种」收敛）。CI 与本地走同一条 resolver，详见 [自举种子纪律](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/bootstrap-seed.md)。

### 门

| 门 | 干什么 | 守 |
|---|---|---|
| forward-bootstrap：CI composite action `ci-bootstrap`，由 `build-and-test`（test-host ×4 OS）与 `toolchain-bootstrap`（compile-toolchain）两个 job 的 Bootstrap 步骤调用 | 下载上一 nightly SDK 作种子 → 编当前 xtask → xtask 自建 z42c + stdlib → 验证工具链；zpkg minor 与种子不同时先走两代自举 | INV-1 |
| self-host 不动点：`compiler-checks` job 的 `xtask test compiler` 步骤（仅 z42c 源变动 / schedule / 手动触发才跑，test-host 跳过它）| 先验全成员 zpkg 在、z42c `[Test]` 单元、e2e，再 `_testSelfHostByteIdentical`：快照现有 dist（gen1）→ 用其 driver 同参再 `build --workspace` 一遍（gen2）→ 逐成员比 zpkg 各段（BLID 段不比，其余段的集合与内容须一致）| INV-3 |
| 本地 `xtask test bootstrap`（CI 不调用）| 上一 nightly z42c 编当前源，越界立即红；经同一个 `_ensureBootstrapSelfDepLibs`，所以只守语法 / 格式 / 非自依赖 stdlib 轴，不验运行期自依赖 | INV-1（改 parser / codegen / 格式后必跑）|

INV-2（测试全绿）由 `test-host` 的 `xtask test all` 守。

测试单元布局 `src/compiler/<member>/tests/<unit>/{<name>.z42.toml(kind=lib) + *.z42}`，经 z42b 运行；`z42.test` 自动可用，不在 toml 声明。`xtask test all` 含 compiler。

## Deferred / Future Work

### self-hosting-future-remote-deps

- **触发原因**：现无整体包管理；workspace 兄弟解析只支持「从当前 workspace 找本地项目」
- **前置依赖**：registry / git-URL 依赖来源 + 版本求解 + 下载缓存机制的整体设计
- **触发条件**：跨 workspace / 第三方包分发需求出现时
- **当前 workaround**：所有非 stdlib 依赖必须是同 workspace 的本地 member 且在 toml 声明

### self-hosting-future-z42c-stdlib-jit

- **触发原因**：`_buildStdlib` 用 z42c **interp** 重编 stdlib（~30s），换 `--mode jit` 可显著加速
- **前置依赖**：22 库 jit-built == interp-built 功能等价；jit 编 stdlib 无 cross-zpkg undefined-fn
- **触发条件**：stdlib 构建成迭代瓶颈时
- **当前 workaround**：`--mode interp`

### self-hosting-future-inherited-optional-param-arity

- **触发原因**：`Z42FuncType` 不携 `MinArgCount`，import 时丢失 → `ExportedTypeExtractor._fromImportedMethod`（子类 re-export 继承自其它包的默认参数方法）只能 emit 全必填 arity；直接定义的方法已正确
- **前置依赖**：给 `Z42FuncType` 加 `MinArgCount` + 构造点透传 + `ImportedSymbolLoader` 从 TSIG 读回
- **触发条件**：出现「子类继承其它包默认参数方法并 re-export」且被第三包省略默认实参调用时
- **当前 workaround**：此类方法在 TSIG 中标全必填；调用方显式传全部实参，或在定义类直接覆写
