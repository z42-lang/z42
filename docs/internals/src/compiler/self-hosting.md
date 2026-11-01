# 编译器自举（self-hosting）— `src/compiler/` 架构

> **页型**: 机制页 ｜ **代码**: `src/compiler/` · `scripts/build/xtask_compiler.z42` · `scripts/build/xtask_bootstrap_check.z42`
> **相关**: [架构总览](architecture.md) · [源代码编译流程](source-compile.md) · [构建编排](../devinfra/build.md)

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
| `z42c.optimization` | lib | IR → IR 优化管线 + `Opt` 开关 | package |
| `z42c.semantics` | lib | 符号收集 + 绑定 + TypeCheck（不含代码生成）| core, syntax, package |
| `z42c.emission` | lib | Codegen（Bound → IR）+ 单文件 / 包编译编排 | core, syntax, package, optimization, semantics |
| `z42c.pipeline` | lib | 编排（workspace 构建、依赖扫描、缓存）| core, syntax, semantics, emission, optimization, package, project, build |
| `z42c.driver` | exe | `z42c` 命令入口 | pipeline, semantics, emission, optimization, syntax, core, package, project |
| `z42.scripting` | lib | eval 内核 | core, syntax, build |

各包另依赖 stdlib（`z42.core` / `z42.io` / `z42.toml` 等，自动可用）。`z42.package` / `z42.project` / `z42.build` / `z42c.core` / `z42c.syntax` 与 `z42.core` 合称**自依赖库**：z42c 运行期与编译期都要用它们，而它们又由 z42c 构建（见轴 ④）。普通工程的解析域只有 shipped `libs/`，要用编译器域库须在 `[dependencies]` 按名声明。

依赖解析：成员从**当前 workspace** 解析 toml 声明的兄弟依赖，与输出位置无关；除 stdlib 外的依赖必须在 `[dependencies]` 声明。远程依赖（registry / git URL）见 [Deferred](#deferred--future-work)。

## 受限写法约定

z42c 只用当前语言子集写；子集表达不了的写法，补进语言而不是在编译器里绕。目前的受限写法：

| 限制 | 受限写法 |
|---|---|
| 类字段的泛型参数被 parser 丢弃（`private List<X> f;` 取元素退化为 `<unknown>`，无法调其方法）| **typed array + count**：`Diagnostic[] _items; int _count;` + 手动 `Grow()`，故编译器内集合一律是并行数组 |
| `new T[n][]` 不能创建交错数组 | 平铺单数组 + 偏移，或平行数组 + 线性查（如 `WorkspaceMembers.DepFlat/DepOff/DepLen`）|
| AST / IR 节点 | `class` 继承层级 + `virtual`。IR 指令的操作数 / 编码 / 头部 tag / 可删性是指令类上的虚方法（基类给安全默认）；AST 与 bound tree 的穷举遍历一律走 visitor 接口（`Accept<R, D>` 双分派），完备性由编译器保证。bound tree 有 visitor 接口 `IBoundExprVisitor<R, D>` / `IBoundStmtVisitor<R, D>` / `IBoundPatternVisitor<R, D>`（节点上抽象的 `R Accept<R, D>(v, d)`，用不上的 R / D 填 `Unit`），实现接口的遍历由编译器保证完备（新节点必须加 Visit 方法；`DepUseCollector`、三个发射器、`FlowAnalyzer` 与语法树侧的三个 binder / `MethodTypeParamUse` 都走这条）。按设计只看部分节点的遍历（ConstEval / MacroRegistry / DocSig …）不实现接口，照旧 `is` 分派 |
| 错误路径 | `throw` / `try-catch` + Exception 子类 |

`TokenKind` 等「枚举」是 `static class` + `int` 常量。

## 构建

`./xtask build compiler` = `_buildCompiler`（`scripts/build/xtask_compiler.z42`）：确保种子（`_ensureSeed`）→ 建 z42vm → 必要时先建 stdlib → `_buildCompilerViaZ42c` 用种子 driver 跑 `build --workspace --release`。命令面见 `./xtask --help`。

**z42c driver 自有 `build --workspace`**（`z42c.driver/src/WorkspaceBuild.z42`，规划在 `z42.project/src/WorkspacePlan.z42`）：

- **成员发现 + 拓扑序**：`[workspace] members`（缺省 `["*"]`，段内通配）展开、`exclude` 剔除；读各成员 `[project].name` + `[dependencies]`，只保留指向 workspace 内成员的边；按 name Ordinal 逐轮扫描、依赖已发射即发射，环抛异常。z42c 恒编全部成员，`default_members` 归编排器（z42b）。
- **显式 `--output-dir <flat>`**：全成员产物落该 flat 目录，兼作各成员的兄弟解析目录。
- **无 `--output-dir`**：按 `[workspace.build].output_dir` 模板展开 per-member 布局，产物落各自 `<output_dir>/dist`；兄弟解析扫全成员 dist + 外部档（`Z42_LIBS`）。先建全部成员 dist（空目录）再按拓扑序逐个 build。
- **解析分档**（`WsTier`）：成员 dist 只回答**成员**包名，外部档只回答**非成员**包名；拓扑序在后的成员整个不可见（`Hidden`）。成员 dist 同时是 exe 的运行期载荷目录（`z42c build` 会把依赖闭包 colocate 进去），里面的外部包副本是载荷不是解析源；`build stdlib` 的外部档是 flat（全成员 dist 的聚合），里面有每个成员上一轮的副本——分档防止这两种旧副本遮蔽本轮 fresh 产物。

**stdlib 由 z42c 构建**：`xtask build stdlib` 用 z42vm `--mode interp` 跑 `z42c.driver.zpkg build --workspace --release`，per-member 落 `artifacts/build/libraries/<name>/<profile>/dist/`，再 verify + 装配 flat view。整个 stdlib 构建同时是 z42c 的 dogfood。

### 运行 z42c 的库目录

运行期 `Z42_LIBS` 是**单个目录**（非 colon-list），传多个会被当成一个非法路径，回退成仅 stdlib，表现为 `VCall: function not found` + 静态字段读 null。`z42c build` 编 exe 时把非 stdlib 依赖复制进输出 dist（自包含），所以 `z42c.driver` dist 自带它依赖的编译器包，跑它时 `Z42_LIBS` 只需 stdlib。xtask 的单元测试经 `Z42_PROBING_PATHS` 拿各编译器成员 dist，见 [产物布局](../devinfra/artifacts-layout.md)。

## CLI

`z42c.driver` 提供 `--dump-keywords` / `--dump-tokens` / `--dump-ast` / `--dump-bound` / `--dump-ir` / `--emit-zbc <src> <out>` / `build [<manifest>]`（含 `--workspace`、`--output-dir`）。完整列表见 `z42c --help`。

## .zbc 写入器（`z42.package/src/BinaryFormat/`）

- **架构**：`ByteWriter`（`byte[]` + LE 助手）→ 各指令类的 `IrInstr.Write(ZbcEncoder)`（指令布局跟指令类走；`ZbcEncoder` 打包缓冲 / 串池 / token 分配器，每函数一个）+ `ZbcInstr`（终结符编码、串池预扫、字面量解析）→ `ZbcWriter`（intern 预扫 + 分段组装）；读端 `ZbcReaderInstr.Retype` 按 `IrInstr.TagReg` 回填 REGT；`TokenAllocator`（插入序 index）；`ZbcStringPool`（插入序 = STRS 字节序）。格式规格见 [zpkg](../formats/zpkg.md)。
- **预扫之后池只读**：STRS 字节序 = intern 调用序，故全部串先按固定序预扫 intern，各段构建器（`BuildFunc` / `BuildType` /
  `BuildDbug` / `BuildTidx`、`TokenAllocator.ResolveMethod/Type`）只 `Idx` 查（漏扫直接抛，不静默追加）。zpkg 写出因此能把
  逐模块段体并行算（`ZpkgWriterZ._buildMods` → `ParallelFor`，各模块写自己的槽，按模块序串行拼装 ⇒ 产物与串行逐字节一致）。
  新加的构建器若在构建期 `Intern`，会在并行下写共享池 —— 改到预扫里。
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

> **两代自举与 `DepScanCache`**：gen0→gen1(stdlib) / gen1→gen2(compiler) **就地覆写同一
> `artifacts/build/{libraries,compiler}`**；`DepScanCache.Get` 按 **path + (size, mtime_ms)** 作答，
> 命中但文件被覆写过即重读重解、并作废该条目的 Tsig/Mods/Types，于是「进程内覆写 zpkg 后重扫」不会返回
> 陈旧 null，两代自举不依赖外部清理。回归测试 `src/compiler/z42c.pipeline/tests/depscancache/` 按真实
> 形状复现（旧 minor 头 → 缓存 null → 就地覆写成当前 minor → 必须重解）。
> ⚠️ 残余窗口：mtime 只有毫秒粒度，「同毫秒内覆写成同样大小的另一份内容」测不出来（make / ninja / rustc
> 同款取舍）；两代自举之间隔着整轮构建，不在这个窗口里。

### 轴 ④ 的破环预建

`_ensureBootstrapSelfDepLibs`（`xtask_compiler.z42`）在 workspace 自建**前**，用当前 driver（冷启动 = 上一 nightly 种子）把**当前源**的 `z42.core` → `z42.project` → `z42.build` → `z42.package` → `z42c.core` → `z42c.syntax` 依次编进 flat（顺序即依赖序，先于其消费者）。理由：z42c 源若消费这些库的新 API 而 flat 里是种子带来的旧版，自建会 `unknown type` / `no field`；若 fresh z42c emit 的调用钉在旧库上，运行期加载真库时解析不到（`undefined function`）。

- **不 warm-skip**：「已在 flat 就跳过」等价于「z42c 不消费这些库的新 API」，不成立；源未变时增量缓存近零成本，源变了本就该重建。
- **推论**：对这 6 个自依赖库，「z42c 源用它们的新 API」由预建自动破环，可同 commit 加成员并消费，无需等 nightly。轴 ③ 的「晚一个 nightly」只约束**其余 stdlib 库**（`z42.collections` / `z42.threading` / …）与 **xtask 源**（xtask 用种子 SDK 编，不受预建保护）。
- **种子 ABI 残余约束**：给这些库的既有导出类型加字段时，新字段不得进构造函数签名——ctor 内给默认值，由消费方构造后赋值。
- 预建前先 `_breakHardLink`：flat 由硬链接装配，就地写会穿透到规范 dist。
- 自建完成后 `_purgeBootstrapPrebuildsFromFlat` 把非 stdlib 成员从 flat 清走（规范产物在各成员 dist），使 flat 保持「只有 stdlib」，否则开发树的 `Z42_LIBS` 里会出现 `z42.project` 等包被误判为框架包。
- 自建首遍失败（跨成员新增符号：driver 自包含的是旧兄弟包）时，用本轮 fresh 成员 dist 重新自包含 driver 并重试一遍。

### 改名：编译期与运行期是两个解析面

**重命名 / 删除一个 driver 运行期要调的符号不是增量**：预建用**当前源**重建自依赖库，加 API 时旧 FQN 还在、
上一版 driver 运行期照旧解析得到；但**改名或删除会抹掉旧 FQN**，而跑这轮 bootstrap 的正是那个 driver ⇒
它在中途就死。实测（想把 `Z42.Build.Project` ↔ `Z42.Package` 互换）：

```
Std.MissingSymbolException: undefined function `Z42.Build.Project.ManifestLoader.Load$1$string`
  at Z42.Driver._build (Main.z42:166)
```

⇒ 动自依赖库里「driver 运行期会调」的符号的**名字或存在性**，是跨 nightly 的两 / 三阶段改动。

要让两代并存就得有兼容物，**而兼容物形态选错就是白干一轮**。判据只有一条：

> **编译期与运行期是两个独立的解析面，一件兼容物通常只盖住其中一个。**

| 面 | 按什么解析 | 依据 |
|---|---|---|
| 编译期 | 命名空间（`using` + FQN），**不看包文件名** | z42c 语义层的 using-scoped 解析 |
| 运行期 | **zpkg 文件名**（消费者 `DEPS` 段记的名字），**不看命名空间** | `runtime/src/vm/loader/locate/namespace.rs` 的 `resolve_dependency` |

于是别名形态跟着「改的是哪一面」走：

| 改的是 | 别名形态 | 先例 |
|---|---|---|
| **包名（文件名）**，命名空间没动 | 同内容**两个文件名** | `z42.ir` → `z42.package` |
| **命名空间**，文件名没动 | 只能**旁置**一份到消费者的 entry 目录 | — |

这两条已实测否证，别再试：

- ❌ **另立一个兼容包**（旧名副本换个包名）：编译期够用，**运行期是空气**——没有任何 `DEPS` 指向那个新
  文件名，它永不被加载。
- ❌ **同一个包里同时声明两套命名空间**：同包内两套**同短名类**会串味（`E0401: no field … on <Class>`，
  短名键混同），**且种子 z42c 一样如此** ⇒ 阶段 1 的源码根本编不出来。

✅ **可行形态 = 编译期 overlay + 运行期旁置**：用种子 z42c 把**当前源**那个库编出来、覆盖进一份种子 libs
的副本供编译期用；再把这份新名产物 `cp` 到消费者 zpkg 旁边（搜索序 `[entry-dir, Z42_LIBS, probing]`，
entry-dir 最优先）供运行期用。两件齐了，改名就能**一步落**，不必拆成跨 nightly 的三步。

> ⭐ **消掉这一整类危险的思路**：把旧 `<pkg>.zpkg` 拷进 driver **自己的 dist**（同样靠 entry-dir 最优先），
> 带改名的 stdlib 重建就过去了——即「让编译器对它运行期要用的 stdlib 库自包含」。注意
> `xtask_stdlib.z42` 步骤 4 的注释本就写着要给 driver「稳定 stdlib 快照」，而
> 「fresh member 从 workspace dist 解析、优先于快照」恰好把那个意图吃掉了。

⚠️ 验这类接线的对照实验，**命令必须真的会触碰那个符号**：懒加载没走到，撤掉兼容物也照样绿——那是空门
（实测踩过：`xtask check stage2` 不读清单 ⇒ 对照无效，换 `package ci` 才红）。

### 运行期 libs 与编译期 libs 分离

自建这一步跑的是**上一代** driver：它运行期要加载那一代的 stdlib，而它编译的源码要解析**当前源**刚预建的库。两者挤在 `Z42_LIBS` 一个变量里，预建一覆盖 flat，种子 driver 自己就崩（追加 API 撞不到，改名 / 删符号必撞）。

| 面 | 谁读它 | 怎么给 |
|---|---|---|
| 运行期 | 被执行的 z42c 加载自己的依赖 | VM 搜索序：入口 zpkg 目录 → `Z42_PROBING_PATHS` → `Z42_LIBS` |
| 编译期 | z42c 为被编译工程解析依赖 | `Z42_LIBS`（只此一处，probing 不进编译期面）|

自建步骤是 `Z42_PROBING_PATHS=<种子代 run-libs>` + `Z42_LIBS=<flat>`（xtask `_z42cProcSeed`）：同名包运行期先取
probing 那份（上一代），编译期只解析 flat（当前源）。driver 换代后的步骤两者同代，只用 flat。夹具
`z42c.driver/tests/fixtures/cli/compile-surface-z42-libs` 守住「probing 不漏进编译期面」。
- **种子代 run-libs 不能取自 flat**：flat 正是要被预建覆盖的目录，快照晚于覆盖会拿到新代，种子 driver 即 `MissingSymbolException`。锚是**上一次自建成功那一刻**的 flat stdlib——新 driver 正是对着它编译的（与 driver 同代是构造保证）：`_snapshotDriverGenLibs` 记进 driver dist 的兄弟目录 `z42c.driver/<profile>/gen-libs/`（不进 dist ⇒ 不随 publish 进 SDK；`package ci` 单独带上），下一次自建由 `_relocateSeedRunLibs` 拷进 `intermediate/compiler/seed-run-libs/<profile>/`；判据是「齐了吗」而非「搬到了吗」，缺口从 SDK libs 补（`_topUpSeedRunLibs`）。冷启动从 SDK 供种时旧锚作废（它与种子不同代）。成员 dist 里若还有外部包副本（老 driver 建的）同样搬走，避免它遮蔽 flat 里的新版。
- **driver dist 不带 stdlib**：exe 复制判据的「框架目录」与解析面的外部档是同一份（driver `_extLibDirs()` = `Z42_LIBS`）。自建时 stdlib 从 flat 解析到 ⇒ 判框架、不复制，所以锚只能由 xtask 自己记。
- `WsTier` 分档生效后，「搬」可退回「拷」。

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
xtask 用到的编译器域 API（`Z42.Project` / `Z42.Build`）同属这一约束；另一个后果是：改布局规则的那次合入里，CI 上的
xtask 仍按上一 nightly 的规则计算路径，xtask 侧的跟进要放到下一个 PR。

**种子来源**：SDK package（`z42-sdk-<ver>-<rid>`）的 `programs/z42c/` + `libs/`，而非 runtime package（runtime package 是纯嵌入式运行时，可能跨 host 使用，不携带单一 host 的 z42c）。冷启动由 `_ensureSeed` 按 `Z42_HOME` → 运行 xtask 的 apphost SDK → `./.z42` 找到 SDK 并供种到 in-tree（按当前 `default_members` 逐个拷；**种子那一代还没有的成员**——新拆出 / 新增的包——跳过，由随后的 `build --workspace` 按拓扑序从源码建出，只有 `z42c.driver` 必须在种子里）；warm 树不被覆盖（gen2 不动点靠「从 in-tree gen1 再种」收敛）。CI 与本地走同一条 resolver。

### 门

| 门 | 干什么 | 守 |
|---|---|---|
| forward-bootstrap：CI composite action `ci-bootstrap`，由 `build-and-test`（test-host ×4 OS）与 `toolchain-bootstrap`（compile-toolchain）两个 job 的 Bootstrap 步骤调用 | 下载上一 nightly SDK 作种子 → 编当前 xtask → xtask 自建 z42c + stdlib → 验证工具链；zpkg minor 与种子不同时先走两代自举 | INV-1 |
| self-host 不动点：`compiler-checks` job 的 `xtask test compiler` 步骤（仅 z42c 源变动 / schedule / 手动触发才跑，test-host 跳过它）| 先验全成员 zpkg 在、z42c `[Test]` 单元、e2e，再 `_testSelfHostByteIdentical`：快照现有 dist（gen1）→ 用其 driver 同参再 `build --workspace` 一遍（gen2）→ 逐成员比 zpkg 各段（BLID 段不比，其余段的集合与内容须一致）| INV-3 |
| 本地 `xtask test compiler bootstrap`（CI 不调用；下载 nightly 需 `gh` 已登录）| 上一 nightly z42c 编当前源，越界立即红；经同一个 `_ensureBootstrapSelfDepLibs`，所以只守语法 / 格式 / 非自依赖 stdlib 轴，不验运行期自依赖 | INV-1（改 parser / codegen / 格式后必跑）|

INV-2（测试全绿）由 `test-host` 的 `xtask test` 守。

测试单元布局 `src/compiler/<member>/tests/<unit>/{<name>.z42.toml(kind=lib) + *.z42}`，经 z42b 运行；`z42.harness` 自动可用，不在 toml 声明。`xtask test` 含 compiler。

## Deferred / Future Work

### self-hosting-future-seed-syntax-capability

- **当前形态**：种子的校验只有**格式**维度——`ci-bootstrap` 的版本差 gate 比种子 `z42c.driver.zpkg` 的
  zpkg minor 与当前 writer minor（不等则走两代自举），strict-pin 也天然拒读旧格式。
  **语法能力维度没有校验**：种子 z42c 若不懂当前源码用的新语法，只会在编译期以普通编译错误暴露，
  而不是"种子太旧"这类明确诊断。
- **触发原因**：没有 z42c 自举能力版本号这类常量，也没有对种子的能力校验；按纪律
  （support 先行、晚一个 nightly 再 use）人为克制，把这个问题挡在前面。
- **前置依赖**：给 z42c 引入自举能力版本号（新增语法 / 新格式时 +1）并写进种子元数据，供下载侧比对。
- **触发条件**：纪律被实际违反并造成一次"莫名编译失败"的排障成本时。
- **当前 workaround**：靠 `xtask test compiler bootstrap` 在开发期判定"上一 nightly 能否编当前源"。

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
