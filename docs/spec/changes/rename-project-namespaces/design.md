# Design: 命名空间互换 —— zpkg 容器格式 ↔ 工程清单

> 状态：**B3a 已实现**（`Z42.Project` → `Z42.Package`）；**B3b 延后**（`Z42.Build.Project` → `Z42.Project`，
> 理由见 §6 —— 它撞上 xtask 的跨代性，需要先加一版并存）。User 裁：
> `Z42.Project`（现装 zpkg 容器格式）→ **`Z42.Package`**；
> `Z42.Build.Project`（工程清单模型）→ **`Z42.Project`**。
> 前置 `--compile-libs` = #894 + #900 + #902。

## 1. 为什么要换

名字装错了内容：

| 包 | 命名空间 | 里面是什么 |
|---|---|---|
| `z42.package` | `Z42.IR` + **`Z42.Project`** | 后者是 **zpkg/zbc 容器读写**（`ZpkgReader` / `ZpkgWriterZ` / `ZpkgBuilder` / `SidecarReader`）|
| `z42.project` | **`Z42.Build.Project`** | **工程清单模型**（`ProjectManifest` / `ManifestLoader` / `DepEntry` / `Profile`）|

即 `Z42.Project` 指的是「打包格式」，而真正的工程清单只能退到 `Z42.Build.Project`。
这是 `converge-z42c-ir-metadata-onto-stdlib` 把 `z42c.project`（写 zpkg 的那个）并入时留下的。

## 2. 为什么它比包名改名难一个数量级

包名**不出现在任何 FQN 里**，所以 `z42.ir` → `z42.package`（#896）只要一份旧文件名的运行期
兼容副本。命名空间改名会**抹掉旧 FQN**，而上一代 z42c 二进制在运行期按旧 FQN 调用：

```
Z42.Project.ManifestLoader.Load / LoadWorkspace   （读清单）
Z42.Project.ZpkgWriterZ.* / ZpkgBuilder.* / ZpkgReader.*（写/读 zpkg）
```

⇒ 文件副本救不了；z42 的 `using X = T;` 是**文件级类型替换**、不导出新 FQN，也当不了桥。

## 3. 🔴 关键约束：`Z42_LIBS` 必须与 driver 的「代」匹配

`--compile-libs`（#894/#900）把**编译期**面分出来了，但**运行期**那一档仍需按代切换：

| 阶段 | 跑哪代 driver | 运行期要 | 编译期要 |
|---|---|---|---|
| `build compiler` 的 workspace 自建 | **种子**（旧 FQN） | **旧** libs | 新（`--compile-libs`） |
| 之后 `build stdlib` 步骤 4 / 一切用 gen1 的步骤 | **gen1**（新 FQN） | **新** libs | 新 |

⇒ 不是「固定一个 `Z42_LIBS` + 一个 `--compile-libs`」。需要：
1. 一个**种子代的运行期 libs 目录** `seed-run-libs/`。⚠️ 初稿写的是「把 flat 快照过去」——
   **那是错的**，flat 正是要被预建覆盖成当前源的目录、当不了「代」的锚（见 §7 ②）。
   实际实现：把 **driver 自己 dist 里那份 colocated 闭包搬进去** + 缺口从 SDK libs 补；
2. **用种子 driver 的调用** → `Z42_LIBS=seed-run-libs`、`--compile-libs=<fresh>`；
3. **driver 换代之后的调用** → 两者都用 fresh。

这与仓库为格式 bump 准备的「多一代收敛」是同一套思路（`ci-bootstrap` 的 gen0/gen1/gen2）。

## 4. 分步（每步能单独验）

- [x] **前置 A**：`--compile-libs`（#894）
- [x] **前置 B**：workspace 路径也认它（#900）
- [x] **B2**：按代选 `Z42_LIBS`（#902）+ 代际锚换成 driver 自己的 bundle（本 change）
- [x] **B3a**：`Z42.Project` → **`Z42.Package`**（zpkg 容器那一半）。清单**由全仓 grep 得出**，
      不凭记忆列目录。`scripts/`（xtask）**零命中**——它不用这个命名空间 ⇒ 这一半对 xtask 无影响，
      这正是它能单独先落的理由。`.github/` 零命中（已查）。`docs/spec/archive/**` 不改
- [x] **B4**：冷启动验证 —— `xtask test bootstrap`（两代，真下载 nightly）+ **手搭的冷树 CI 模拟**
      （真 nightly 种子 + 清空 artifacts，跑 `build compiler` → `build stdlib`）
- [x] **B3b**：`Z42.Build.Project` → `Z42.Project` —— **一步落**（`src/**` 与 `scripts/**` 同时扫到
      新名），靠 `ci-bootstrap` 的「编译期 overlay + 运行期旁置」跨过种子代差，见 §6.2。
      > 🔴 **原计划的「三步走（support 先行：同一个包同时声明两套命名空间 → 跨 nightly → use）」
      > 已实测否证**：同包内两套同短名类会串味（`E0401`），**种子 z42c 同样如此** ⇒ 阶段 1 的源码
      > 根本编不出来。另一个备选「独立兼容包」则是运行期空气（`DEPS` 按文件名解析）。两个否证
      > 与采纳形态见 §6.1 / §6.2 —— **这是本 change 最贵的一条教训：「并存一版」有三种落地形态，
      > 两种是错的，而判据在「编译期 / 运行期各按什么键解析」，不在「抄上次别名的做法」。**
- [x] **B6**（z42c 侧）：解析**分档** `WsTier` —— 成员 dist 只答成员包、外部档只答非成员包，见 §7 ①。
      ⚠️ 它自己也受种子纪律（跑 workspace 构建的是种子 driver）⇒ 要跨一个 nightly 才生效
- [x] **B7**（CI 侧）：bench A/B 的代差判据补上「命名空间」这一维，见 §7 ③

## 5. 已知会踩的坑（来自 #896 三轮 CI）

1. **只扫自己想到的目录必漏**：先 grep、把命中面当清单，再逐类判断（改 / 兼容层 / 归档不动）；
2. **有些代码同时面对改名前后两个版本**：CI 的 base/PR 对比、两代自举、跨 nightly 种子。
   本次 `.github/` 零命中，但 **`test bootstrap` 的 `runlibs` 同时是 `Z42_LIBS` 与 `--output-dir`**，
   必须先分开（B2 覆盖）；
3. **兼容副本进编译期 libs 只在 FQN 交叠时才 `E0606`**（同 FQN 两个包）。⚠️ 这条原写成「兼容副本
   不能进编译期 libs」，B3b 实测**不成立**：旧名副本与新名本体导出的 FQN 完全不交叠 ⇒ 同处编译期
   libs 一声不响地编过。真正拦住兼容副本的是**运行期**按文件名解析（§6.1），不是 `E0606`；
4. **登记表改 id 格式要同时迁移条目**，否则旧条目成幽灵（#900 修过一次）。

## 6. 🔴 xtask 是**两头都挂在上一代 SDK 上**的消费者 —— B3b 的全部难处都在这

xtask 不像 `src/toolchain`（从源码编、对着 flat 跑）。它**编译**与**运行**都绑在上一代：

| 环节 | libs 来源 | 哪一代 |
|---|---|---|
| CI `ci-bootstrap` 步骤 2「seed z42c builds current xtask.zpkg」| `Z42_LIBS=$boot_libsw`（下载的 nightly SDK libs）| **上一代** |
| CI 步骤 3+ 及后续所有 job 运行 xtask | `Z42_LIBS=$PWD/artifacts/build/libraries/dist/release`（**flat**）| 本轮**新代** |
| 本地 `./xtask` | apphost 从 `Z42_HOME/libs`（= 安装的 SDK）加载 | **上一代** |

于是一次 CI 运行里两种排法**都不成立**：

- `scripts/` **留旧名** ⇒ 步骤 2 编得出，但步骤 4 把 flat 换代后，`Z42_LIBS=flat` 跑 xtask 立刻
  `MissingSymbolException: Z42.Build.Project.ManifestLoader.LoadWorkspace`（#905 首轮 CI 实测，
  `build stage-toolchain` 那一步）；
- `scripts/` **扫新名** ⇒ 步骤 2 的种子 libs 里还没有新命名空间，`E0494`，自举第一步就断。

还有**第三个约束**（B3b 实测补记）：[`ci.yml` 的 consume-smoke](../../../../.github/workflows/ci.yml)
用**本代 SDK** 编 `scripts/xtask.z42.toml`。所以「scripts/ 留旧名」不只坏在运行期 —— 那一步的
**编译期**也会 `E0494`。⇒ 三个约束合起来看，落点被逼到唯一一处（见下面 §6.2）。

⚠️ **本地四格全绿却 CI 红的原因**：本地 `./xtask` 是 **apphost**，从 `Z42_HOME/libs` 加载，
**永远看不到 flat**；CI 是显式 `Z42_LIBS=flat`。⇒ 判「xtask 能不能跑」必须按 CI 的方式复现：

```bash
Z42_LIBS=$PWD/artifacts/build/libraries/dist/release \
  artifacts/build/runtime/release/z42vm artifacts/xtask/xtask.zpkg -- <任意子命令>
```

## 6.1 🔴🔴 编译期与运行期是**两个独立的解析面** —— 这是本节的总钥匙

上一版 §6 只说了「必须并存一版、跨一个 nightly」，没说**并存怎么落地**，于是第一版实现选错了
形态、白跑一轮。根因是这两个面各按各的键解析，**一件兼容物只能盖住其中一个**：

| 面 | 按什么解析 | 依据 |
|---|---|---|
| **编译期** | 命名空间（`using` + FQN），**不看包文件名** | `z42c` 语义层；fix-crosspkg-static-ns-collision 的 using-scoped 解析 |
| **运行期** | **zpkg 文件名**（`DEPS` 段记的名字），**不看命名空间** | `src/runtime/src/metadata/loader/namespace.rs` 的 `resolve_dependency`，头注原话「the VM's lazy loader **no longer routes by namespace; it uses zpkg file names**」 |

运行期搜索序是 **`[entry-zpkg 目录, Z42_LIBS, probing paths]`**（`config.rs`：「Empty = search
order stays `[entry-dir, libs]`」）—— **entry 目录优先**，这就是落点。

### 三种形态：两个实测否证 + 一个成立

| 形态 | 结果 |
|---|---|
| **独立兼容包** `z42.project.compat`（旧名副本另起一个包名） | ❌ **编译期够用、运行期是空气**：它编得出来、也能进 flat，但**没有任何 `DEPS` 指向那个文件名 ⇒ 永不被加载**，xtask 照旧 `MissingSymbolException`。判别实验：把它的产物改名成 `z42.project.zpkg` 放进 flat 副本 ⇒ 同一条命令 exit 0。 |
| **同包双命名空间**（旧名副本放进 `z42.project/src/legacy-ns/`） | ❌ **根本编不出来**：同包内两套同短名类会串味 —— 新名侧 `BuildLayout.z42` 报 15 个 `E0401: no field 'Project' on 'ProjectManifest'`（短名键混同，与 `unify-type-identity-fqn` / 符号表 FQN 键那一类同源）。改成不同文件基名仍红 ⇒ 不是文件名冲突。**且种子 z42c 同样如此**（隔离实测：把双 ns 包整个拷到 scratch，用上一代 `.z42/bin/z42c` 编，报同样的 15 个 `E0401`）⇒ 就算修了当前 z42c 也没用：阶段 1 的源码必须能被**种子**编。 |
| ✅ **编译期 overlay + 运行期旁置**（采纳） | 两个面各给一件，见 §6.2。 |

⚠️ 顺带纠正一条：`z42.ir` → `z42.package` 那次的别名之所以是「同内容两个**文件名**」，因为它改的是
**包名**、命名空间没动；这次改的是**命名空间**、文件名没动 ⇒ 别名形态必须**反过来**。同一个
「别名」直觉套错了轴就是空气。

## 6.2 采纳的形态：编译期 overlay + 运行期旁置（B3b 一步做完，不再拆三步）

`scripts/**` 本轮**直接扫到新名**，两个面各接一件（都在 `ci-bootstrap` 步骤 [1.6]/[2]）：

1. **编译期 overlay**：先用种子 z42c 把**当前源**的 `z42.project` 编出来，覆盖进一份种子 libs 的
   副本（`$work/xlibs`），步骤 [2] 用 `Z42_LIBS=$xlibsw` 编 xtask ⇒ 编译期有新命名空间。
   （种子 z42c 编得动当前源 —— 本地实测 exit 0；它只是个普通库，依赖 `z42.core`/`io`/`toml`
   在种子 libs 里都有。）
2. **运行期旁置**：把那份新名 `z42.project.zpkg` **拷到 `artifacts/xtask/`**（xtask.zpkg 旁边）。
   entry 目录优先于 `Z42_LIBS` ⇒ 之后无论 `Z42_LIBS` 指向种子代（步骤 [3]、本地 apphost）还是
   本轮 flat（步骤 [4] 之后所有 job），xtask 拿到的都是新名。**隔离是构造保证的**：只有 entry 在
   `artifacts/xtask/` 的进程（= xtask 自己）看得见它。

### 为什么不能靠 z42c 的**自动** colocate

`z42c build` 对 exe 会 colocate 依赖闭包，看似只要给 xtask 声明 `[dependencies]` 就行。**不行**：
`ExeDeps.z42` 头注明写「**非框架**依赖」才 colocate，框架包（stdlib）一律走 `Z42_LIBS`。那是对的
设计（不然每个 exe 都拖一份 stdlib）。⇒ 所以旁置是**引导脚本显式 `cp` 一个文件**，不是让 z42c
去 colocate 框架包 —— 两者别混。

### 实测判据（四段，都跑过）

| # | 配置 | 期望 | 实测 |
|---|---|---|---|
| ① | 种子编的 xtask（旧名）+ 本代 flat，无旁置 | 红 | ✅ `MissingSymbolException: Z42.Build.Project.ManifestLoader.LoadWorkspace$1$string` |
| ② | 同上 + 把旧名产物旁置 | 绿 | ✅ exit 0（确认 entry 目录优先） |
| ③ | overlay 编出的 xtask（新名）+ **上一代** libs + 旁置本代产物（= 步骤 [3] 形状，**混代**） | 绿 | ✅ exit 0 ⇒ 混代风险实测不存在 |
| ④ | 同 ③ 但撤掉旁置件 | 红 | ✅ `MissingSymbolException: Z42.Project.ManifestLoader.LoadWorkspace$1$string` |

⚠️ 做 ④ 这类「撤掉它应该就红」的对照时，**命令必须真的会触碰清单加载**：先用
`xtask test stage2` 做对照，撤掉旁置件照样绿 —— 因为懒加载根本没走到 `Z42.Project`，那是个空门。
换 `build stage-toolchain` 才红。

### 阶段 2（下一 nightly 后）

种子自带 `Z42.Project` ⇒ 撤掉 overlay 与旁置两段即可，无任何源码副本要删。欠账登记在
`.github/actions/ci-bootstrap/action.yml#b3b-project-ns-overlay`（为此把 `xtask test stage2`
的扫描面扩到了 `.github/**/*.yml` —— 阶段 1 的过渡形态第二次落在门看不见的文件类型里）。

## 7. 🔴 B3 实测挖出的三个真缺陷（都不是改名本身）

### ① 成员 dist 在编译期**遮蔽** flat（`E0494`，根因在 z42c）

`WorkspaceBuild.z42` 的解析面是 `libsDirs = 全成员 dist + 外部档（--compile-libs / Z42_LIBS）`，
**纯 basename 命中、成员 dist 排在前**。而成员 dist 同时是**运行期载荷**目录——`z42c build` 对 exe
会 colocate 依赖闭包进去。于是上一代 colocate 的外部包副本盖住 flat 里刚建好的当前源新版：

```
z42c.driver/release/dist/z42.package.zpkg（种子代，声明 Z42.Project）
  遮蔽 artifacts/build/libraries/dist/release/z42.package.zpkg（当前源，声明 Z42.Package）
⇒ z42c.semantics: E0494 命名空间 `Z42.Package` 不存在   ——而 flat 里明明有
```

⚠️ **两份同尺寸**（`Z42.Project` / `Z42.Package` 都 12 字符），只核对文件大小会被骗过去；
判别要看 zsym 的 STRS 段，且命名空间按**段**存（`\x03Z42\x07Package`），整串在二进制里 grep 不到。

既有的 `_laterMemberNames`（藏后序成员）是**同一类**问题的局部补丁：它只按名挡成员，不管
成员 dist 里的**非成员**副本。正解（B6）= 成员 dist 只对**成员名**有效，外部包一律走外部档
—— **已实现**（`WsTier`，z42c.pipeline；判别力用注入法两侧验过）。
⚠️ 但它自己也受种子纪律：跑 workspace 构建的是**种子** driver ⇒ 要跨一个 nightly 才生效
（与 #902 学到的「enabler 自己也是种子纪律的对象」同一课），在那之前编排侧的「搬」（见 ②）
仍是唯一防线；进种子后那里可从「搬」退回「拷」。

### ② flat 当不了「代」的锚（`MissingSymbolException`）

B2 的做法是「把 flat 快照成 `seed-run-libs` 当种子代运行期面」。但 flat 正是本轮要被
`_ensureBootstrapSelfDepLibs` 覆盖成**当前源**的目录 —— 快照晚于覆盖、或上一轮跑到一半，
快照拿到的就是**新代**，种子 driver 随即
`MissingSymbolException: Z42.Build.Project.ManifestLoader.LoadWorkspace`。

改法：**锚换成 driver 自己 dist 里那份 colocated 闭包**（与 driver 同代是*构造保证*的），
把它**搬**进 `seed-run-libs` —— 一举同时解掉 ①（成员 dist 不再有外部包）和 ②（拿到真·种子代）。
判据不是「搬到了吗」而是「**齐了吗**」：bundle 可能不完整（冷启动 staged 的 driver 根本没 bundle；
被清理过的树只剩零星几个），缺口从 **SDK libs** 补（`_ensureSeed` staged 种子正是从那儿取的，同代）。
实现见 `scripts/build/xtask_compiler.z42` 的 `_relocateSeedRunLibs` / `_topUpSeedRunLibs`。

> 教训：判定「哪一代」不能看**会被写的目录**。锚要选与产物同生共死的东西——这里是
> driver 自己的 bundle，退而求其次是 SDK（种子的出处），**永远不是 flat**。

### ③ bench A/B 的代差判据缺「命名空间」这一维（#905 第三轮 CI）

`bench-pr.yml` 的 A/B 里，base 那一侧是「**base 的编译器源码 + PR 的 libs**」（那一步头注自己
写着：PR flat libs 就是 base 源码被类型检查时的 TSIG 面）。这对普通改动成立，但 PR **删掉或
改掉** stdlib 的某个命名空间时必破：base 源码里的 `using Z42.Project;` 在 PR 的 libs 里已不存在
⇒ base 侧 `E0494`，整个 job 红（红在 `Build base (merge-base) toolchain for A/B`）。

这与既有的 `fmtgap`（格式代差 ⇒ base 产物跑不动 ⇒ 跳过 A/B 并响亮声明「性能未被测量」）是
**同一类代差**，只是维度不同 ⇒ 扩进同一道门、复用同一条跳过路径。判据取「**base 有而 PR 没有**」
而不是「两边不相等」：纯新增命名空间不影响 base 侧类型检查，那种 PR 仍该正常测性能。

⭐⭐ **这条是 §5 坑 ① 的下一层**：那条说「改名第一步是全仓 grep 旧名」。本轮 `.github/`
**文本零命中**，我据此判了「不受影响」——错。A/B harness 从没写过 `Z42.Project` 这个词，
却**依赖它存在**。⇒ **grep 只能找出文本耦合；跨代假设要靠读。** grep 完还要再问一遍
「有哪些地方同时面对改名前后两棵树 / 两代产物」，逐个读它的假设。
