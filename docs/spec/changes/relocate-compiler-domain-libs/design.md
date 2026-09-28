# Design: 编译器域库挪回 `src/compiler/`

> **状态：DRAFT，待 User 裁决**（一处真问题 + 两处取舍，见 §6）。
> 前史：`add-package-roles` 批 2.5 曾裁「`role` 不需要，**移包也不需要**」（2026-09-25）。
> User 2026-09-27 改裁「**真挪回 `compiler`**」，并指出 z42b / z42i **可以用 path 依赖**拿到它们
> —— 那正是 2.5 当时没算进去的通路（`add-path-dependencies` 已落，`PathDepPlan` #860 让
> `z42b build` 也真解析 path 依赖）。本文按新裁决重做设计。

## 1. 判据：「编译器域」按什么定

不按感觉，按**命名空间已经声明的域** + **消费方分布**。`src/libraries/README.md` 自己写着
「两类库」：`Std.*` = 用户 stdlib，`Z42.*` = 工具链/编译器域。那段脚注就是本 change 要消灭的
补丁性散文（它是「靠约定」的活化石）。

消费方分布（`grep -rl "using <ns>;"` 按目录归并，2026-09-27 实测）：

| 包 | 命名空间 | 编译器域内消费方 | 域外消费方 | 判定 |
|---|---|---|---|---|
| `z42c.core` | `Z42.Core` | src/compiler 83、z42c.syntax 26 | z42.scripting 2 | **挪** |
| `z42c.syntax` | `Z42.Syntax` | src/compiler 123 | z42.scripting 5、z42i 1 | **挪** |
| `z42.package` | `Z42.IR` + `Z42.Package` | src/compiler 27+129 | workload 4、z42i 1、devtools 2 | **挪** |
| `z42.project` | `Z42.Project` | src/compiler 12 | z42b 6、xtask 4、launcher 2、z42.build 3 | **挪** |
| `z42.build` | `Z42.Build` | src/compiler 3 | z42b 6、workload 4、z42.scripting 3、z42i 2、xtask 1 | **挪** |
| `z42.scripting` | `Std.Scripting` | — | z42i 2、z42.repl 1 | **挪**（User 2026-09-27 裁）—— 见 §4.1：挪它才让 `ExeDeps` 的不变式继续成立 |

⭐ `examples/**` 与 `docs/**` 对这六个包**零命中** —— 今天没有任何用户面示例依赖它们。

## 2. 落点：与现有编译器成员并列，**且不改包名**

```
src/compiler/z42c.core      ← 从 src/libraries/ 挪回（converge-z42-syntax-lib 当初从这里搬出去的）
src/compiler/z42c.syntax    ← 同上
src/compiler/z42.package    ← 名字先不动
src/compiler/z42.project    ← 名字先不动
src/compiler/z42.build      ← 名字先不动
src/compiler/z42.scripting  ← 名字/命名空间先不动（`Std.Scripting` 留待改名批次）
src/compiler/z42c.semantics / z42c.pipeline / z42c.driver   （已在）
```

🔴 **本次刻意不改包名**（`z42.package/project/build → z42c.*` 是 `add-package-roles` 批 3）。
理由是机制性的、不是口味：

> **挪目录不改 zpkg 文件名 ⇒ 运行期依赖解析（纯文件名查找）看到的东西一字不变 ⇒ 零种子纪律、
> 零跨 nightly、零格式 bump。** 改名才动文件名，那是跨 nightly 的活（B3b 刚付过学费）。
> 两件事合做会把一个今天就能落的改动拖成跨 nightly 的改动。

## 3. 两个解析面分别怎么接（B3b 的总钥匙照用）

| 面 | 机制 | 要改什么 |
|---|---|---|
| **编译期** | **path 依赖**。先例就在仓库里：`z42.interactive` 已写 `"z42.repl" = { path = "../repl" }`，注释原文「私有组件 = path 依赖：z42c 建闭包 + colocate 进 z42i payload，**不入 `<sdk>/libs`**」 | 各消费方清单加 `[dependencies]` path 条目 |
| **运行期** | **`deploy = "shared"` + `probing-paths`**，**不复制**。两者都已落地且**z42i 今天就在用**：它的侧车里写着 `probing-paths = "../z42c"`，dist 里只有自己 + 私有 path 依赖，z42c 那边一个都没拷。`deploy = "shared"` 的语义（`DepEntry.z42`）正是「不复制，运行期从 `probing-paths` 声明的目录解析」，消费侧 #848 已落地 | 各消费方清单加一行 `probing-paths = "../z42c"` + 依赖条目标 `deploy = "shared"` |

### 逐个消费方

| 消费方 | 今天怎么拿 | 改后 | 附注 |
|---|---|---|---|
| `z42c.driver` / `pipeline` / `semantics` | 框架 `libs/` + 自身 dist 闭包 | 同 workspace 成员 | `src/compiler/z42.workspace.toml` 的 `default-members` 加 5 个 + 拓扑注释 |
| **z42b**（builder）| 框架（清单里**根本没声明**）| path → `../../z42.project`、`../../z42.build` | ⚠️ 清单里「stdlib-only … z42.build/z42.project 均为 stdlib → 不破 stdlib-only」这段注释**挪完就是假话**，必须同 commit 改 |
| **z42i**（interactive）| `z42.package` 版本依赖 + 框架 | path → `z42.package`、`z42c.syntax`、`z42.build` | 已有 path dep 先例，最省 |
| `launcher`（`z42`）| 框架 | path → `z42.project` | |
| `workload` | 框架 | path → `z42.package`、`z42.build` | |
| `devtools`（z42d）| `z42.package` 版本依赖 | path → `z42.package` | |
| **xtask**（`scripts/`）| 框架（清单**刻意**无 `[dependencies]`）| path → `z42.project` | ⭐ **附带收益**：xtask 从此不再「编译期对上一代 libs、运行期对本代 flat」—— B3b 那整套 overlay + 旁置的**根因直接消失**。那段「NO [dependencies] on purpose」注释要改 |
| **z42.repl** | 框架（`z42.scripting` 版本依赖）| path → `z42.scripting` | z42i 的私有组件，本来就是 path 依赖形态 |

## 4. 用户应用要能**显式引用**编译器域库，并且拷进输出目录（User 2026-09-27 追加要求）

隔离**不是禁止**，是「不隐式可见」。规则一句话：

> **`libs/` = 隐式可见的框架面；`compiler-libs/` = 显式声明才可见的 opt-in 面。**
> 用户想写 linter / 格式化器 / 代码生成，就在自己工程的 `[dependencies]` 里写上
> `"z42c.syntax" = "0.1.0"`；没写 ⇒ 结构上解析不到（`E0494`）。

⭐ **「拷进输出目录」这一半是免费的**：`ExeDeps` 的复制判定是「**在不在 shipped `libs/`（= `Z42_LIBS`）**」
—— 从 `compiler-libs/` 解析到的包天然不在 `libs/` ⇒ 判为**私有依赖** ⇒ 复制进该工程的 dist，
运行期靠 entry 目录优先解析。这正是 `z42.repl` 今天在 z42i 上已经在跑的形态。

### 4.1 落点与解析域：**扁平放 `programs/z42c/`，不新增 `compiler-libs/`**（User 裁）

理由不只是「少一个目录」——**代码里早就把 `programs/z42c/` 当成编译器域的扁平落点**：
`ReplCompilerHost._findCompilerZpkg` 的探测序第 ① 档就是 `Z42_HOME/programs/z42c/`，注释原文
「其传递闭包 `z42c.semantics`/… **须与它同目录** …故 `programs/z42c/` 里的兄弟自然解析，
无需复制进 app payload」。再造一个 `compiler-libs/` 等于给同一件事开第二个落点。

🔴 **顺带修掉一条真缺陷**：`_compilerLibsDirs()`（批 1）探测 ① `Z42_HOME/compiler-libs`
② `<sdk>/compiler-libs` ③ 开发树 `artifacts/build/compiler/z42c.semantics/release/dist`。而
**发布态 SDK 根本没有 `compiler-libs/` 这个目录**（2026-09-27 实测：今天的 nightly SDK 里没有，
`z42c.semantics.zpkg` 躺在 `programs/z42c/`；全仓也没有任何代码去创建它）⇒ ①② 恒不命中，
**只有开发树靠 ③ 命中**。也就是说批 1 声称打通的「用户能写 generator」**在发布态其实是空的**
—— 又一例「本地绿、发布态空」。把解析域改指 `programs/z42c/`（真实存在的那个目录）即修复。

### 4.2 于是只需做两件事（原 ③ 被裁决消掉）

| # | 改动 | 说明 |
|---|---|---|
| ① | **解析域**：`[dependencies]` 里**显式声明**的包也查 `<sdk>/programs/z42c/`（替掉今天指向不存在的 `compiler-libs/` 的那两档）| 今天该域**只在 `kind == "analyzer"` 时生效**（`BuildPaths.z42:168`）；普通 exe 写个格式化器拿不到 |
| ② | **SDK 组装**：六个包随编译器落 `programs/z42c/`（semantics 已在那）；各 toolchain 程序靠 `probing-paths = "../z42c"` 指向它，**不复制** | publisher / `build sdk` 的组装面；z42i 的形态照抄 |
| ~~③~~ | ~~修 `ExeDeps` 的「框架包不递归」~~ | ❌ **不需要了** —— 见 4.3 |

### 4.3 为什么「`z42.scripting` 也挪出去」正好解掉那个难题（User 2026-09-27 裁）

`ExeDeps` 的不变式是「**拷了它 ⇒ 才看它的依赖**」，配套假设写在注释里：
「框架包不拷也不递归（**它的依赖同在 `libs/`**）」。破坏它的唯一情形 = **某个留在 `libs/` 的框架包
依赖了已挪走的包**。实测（逐个扫 `src/libraries/*/*.z42.toml` 的 `[dependencies]`）全仓只有三条边
指向这五个包：

```
z42.build     → z42.project      （两头都挪，内部边）
z42c.syntax   → z42c.core        （两头都挪，内部边）
z42.scripting → z42.build / z42c.core / z42c.syntax    ← 唯一的跨界边
```

⇒ **把 `z42.scripting` 一起挪走，跨界边归零，那条假设继续成立，`ExeDeps` 一行不用改。**
用户要嵌 eval 就**显式引用** `z42.scripting`：它此时不在 `libs/` ⇒ 判私有 ⇒ 拷进用户应用输出；
而「拷了才递归」这条反而帮我们把 `z42c.core`/`z42c.syntax`/`z42.build` 一并带上 ⇒
**「拷到输出目录」这件事是构造式成立的，不靠新代码**。

⭐ 对标物在本仓文档里早就写着：`Microsoft.CodeAnalysis.CSharp.Scripting` 是**可选包**，
运行时本身不带 Roslyn。`z42.scripting` 离开 `libs/` 正是这个形状（runtime 包也因此瘦 36 KB）。
它的命名空间 `Std.Scripting` 暂不动 —— 改命名空间要跨 nightly（B3b 刚付过学费），
并入后续改名批次。

### 4.4 `z42.package` / `z42.project` 留不留？—— 全挪（Claude 主张，User 表示无所谓在哪）

User 2026-09-27：「`z42.package` 和 `z42.project` 这两个相对独立可以留在 libs，但挪出去也没关系，
反正我们设计好可以给用户引用到就行，就无所谓在哪。」

**机制上两种都安全**（实测）：这两个包的依赖是 `z42.core`/`io`/`toml`/`encoding`/`crypto`，全在
`libs/` ⇒ 留下也**不产生跨界边**，4.3 那条不变式照样成立。所以这是个纯取舍。

**挪的代价**：**体积上是零**，甚至是负的 —— 见 §5（走 `probing-paths` 共享，不复制；而这五个包
今天在 `libs/` 与 `programs/z42c/` 里**各有一份**，挪出 `libs/` 等于去掉那份重复）。真实成本是
接线：每个消费方清单加一行 probing-path + 依赖条目标 `deploy = "shared"`。

**主张全挪，决定性理由是本 change 的验收信号本身**（§7 ① / `add-package-roles` 批 3.4）：
那段要删掉的脚注写的是

> 「`src/libraries/` **同时住着**用户 stdlib（`Std.*`）与工具链库（`Z42.*`：`z42.package` /
> `z42.project` / `z42.build`）」

⇒ 只要这两个 `Z42.*` 包还留在 `libs/`，**那段散文依然是真的、删不掉**，验收信号直接失败。
混合摆放等于把「三个判据一个旋钮」那个老问题原地留着。全挪之后规则收敛成一句：
**`libs/` 里只有 `Std.*` 用户面**（`z42.scripting` 的 `Std.Scripting` 命名空间是唯一的例外，
留待改名批次消掉）。

附带收益（挪 `z42.project` 才有）：**xtask 从此用 path 依赖拿清单模型，不再「编译期对上一代 libs」**
—— B3b 整套 overlay + 旁置的根因就是这条耦合，挪完这一类痛永久消失。

## 5. 体积：**净减约 672 KB**（不是增加 —— 我最初算错了，已纠）

初版这里写「payload 净增约 +1.1 MB」，那是**把「今天 ExeDeps 的默认 colocate 行为」当成了挪动的
固有代价**。两处都不对：

1. **走 `deploy = "shared"` + `probing-paths` 就不复制**（z42i 今天已是此形态），所以没有 N 份重复；
2. 更要紧的是 —— **这五个包今天本来就在两处各有一份**（2026-09-27 实测安装态 SDK）：

| 包 | `libs/` | `programs/z42c/` |
|---|:---:|:---:|
| `z42c.core` / `z42c.syntax` / `z42.package` / `z42.project` / `z42.build` | ✅ | ✅ **已有** |
| `z42.scripting` | ✅ | ❌ |

`programs/z42c/` 的那份是 driver 的自包含闭包（26 个 zpkg / 3304 KB）。⇒ 把这五个从 `libs/` 移出，
**去掉的正是那份重复**：`libs/` −672 KB，`programs/z42c/` 不变（已有），只多收一个
`z42.scripting`（+36 KB）。

**净账：SDK ≈ −672 KB**（`libs/` 2884 → 约 2176 KB，且**只剩 `Std.*`**）。runtime 包同样瘦身，
且从此不带 eval/编译器域 —— 正是本仓文档引的 .NET 形状（运行时不带 Roslyn）。

唯一真复制的是 **xtask**（repo-only、不发布）：它拿 `z42.project` 走默认 `deploy`（拷 56 KB 到
`artifacts/xtask/`）即可，不值得为一个内部工具再配 probing-path。

## 5.1 🔴 挪动会静默吃掉 24 个测试 —— PR ① 必须同时接好

`[Test]` 单元的跑法是 `_testLib` 遍历 **`_stdlibList()`**（= libs workspace 的 `default-members`）。
被挪的包各自带着 `tests/`：

| 包 | `tests/*.z42` |
|---|---|
| `z42c.syntax` | 10 |
| `z42.project` | 8 |
| `z42.package` | 4 |
| `z42c.core` | 2 |
| `z42.build` / `z42.scripting` | 0 |

而**现有编译器成员（driver / pipeline / semantics）一个 `tests/` 都没有** ⇒ 仓库里**根本没有跑
编译器成员 `[Test]` 的路径**。直接 `git mv` = **24 个测试文件静默停跑**，正是
`migration-left-a-dead-gate-behind` / `selfhost-migration-lost-negative-tests` 那一类。

**做法**：`_testLib` 的成员来源从「stdlib members」扩成「stdlib members + compiler members」，
`Z42_LIBS` 用 `_assembleAllLibs`（它本来就 = stdlib flat + 全部 compiler 成员）。

**验收判据（必须是数字，不是「看起来在跑」）**：挪动前后跑测器报的单元总数不减。
**挪动前基线已取**（2026-09-27，`xtask test stdlib <lib>` 的 `Result:` 行求和）：

| 包 | passed |
|---|---|
| `z42c.syntax` | 180 |
| `z42.project` | 52 |
| `z42.package` | 22 |
| `z42c.core` | 11 |
| **合计** | **265** |

### 5.1.1 实施时被实测推翻的两个想当然

1. ❌ **「编译器后端没有扁平测试 ⇒ 发现规则找到 0 个单元、自然跳过」** —— **不会**。清单感知的发现
   会去编**包本体**，然后报 `error: no [Test] or [Benchmark] found in …/z42c.pipeline.zpkg`，
   记作文件失败（GREEN 当场 `2 file(s) failed`）。⇒ 判据必须是**正向识别编译器格式**
   （`tests/<unit>/*.z42.toml` 存在 ⇒ 让开），不能指望「找不到就跳过」。
2. ❌ **「那就按单元数为 0 排除」** —— 也错。`z42.scripting` 的 11 个目录既无 `source.z42` 也无
   toml（清单里记的「零约定单元」），单元数正是 0，但它**必须继续被枚举**：孤儿源守卫每轮打的
   那行 ⚠ 是**有意保留的欠债信号**（清单原话「欠债不消音」），按单元数滤掉就等于悄悄消音。
   ⇒ 实测判据：修复后 `347 file(s) passed (in 23 lib(s))` 且那行 ⚠ **仍然出现 1 次**。

⭐ 两条合起来是同一个形状：**「静默跳过」和「有意的噪声」都不能靠间接指标去猜，要按形态正向判。**

## 5.2 本地/发布两种布局的差异（`deploy` 怎么选）

| 布局 | 编译器域包在哪 | 运行期怎么找 |
|---|---|---|
| 发布态 SDK | `programs/z42c/`（**一个扁平目录**）| `probing-paths = "../z42c"`（相对 entry 目录；z42i 今天就是）|
| 仓库内开发 | `artifacts/build/compiler/<member>/release/dist/`（**每成员一个目录**）| 没有单一目录可指 ⇒ 走 `Z42_LIBS = _assembleAllLibs`（stdlib flat + 全部 compiler 成员，已有）|

⇒ 同一份清单要同时适配两种布局，靠的不是两套 `probing-paths`，而是
**「发布态走 probing、开发态走 alllibs flat」**——两条路今天都已存在（`ReplCompilerHost._findCompilerZpkg`
的探测序第 ④ 档写的就是「dev warm-z42c 回路 / workspace 运行：编译器组件与 stdlib 同置一处」）。

⚠️ `xtask test stdlib` 那一档今天**刻意只给 stdlib flat**（z42b 清单注释里的 "stdlib-only"）。
六包挪走后 z42b 需要它们 ⇒ 该档的 `Z42_LIBS` 要改成 alllibs flat，**且那段注释要同 commit 改**。

## 5.3 用户项目怎么引用编译器域库（User 2026-09-27 追加设计）

User 的方向：「不同的 SDK 目录向 z42b 注入相关宏和路径，比如 `Z42_COMPILER_LIBS=<SDK 目录>`，
可以使用 `Z42_HOME` 路径通配符，项目直接用这个路径宏添加引用。」

落到现有机制上（三件都已在，只需接线）：

| 机制 | 现状 |
|---|---|
| `PathTemplate`（`${workspace_dir}` / `${member_dir}` / `${profile}` / `${output_dir}`，`$$` 转义）| 已在，但**只用于 `[build]` 路径**；`[dependencies]` 的 `path` **不走展开** |
| `deploy = "copy" / "shared"` | 已在且已被消费（#848）|
| `probing-paths` | 已在，**支持 glob、相对 entry 目录**（`expand_probing_paths`），但**不做 `${}`/环境变量展开** |

### 设计

1. **新增清单变量 `${compiler_libs}`**，由 z42c/z42b 解析，取值序（复用 `_findCompilerZpkg` 那套，
   不新造）：`Z42_COMPILER_LIBS` 环境变量（自定义 SDK 的逃生口）→ `Z42_HOME/programs/z42c` →
   `Z42_PORTABLE_VM` 反推 `<sdk>/programs/z42c` → 开发树。
2. **把 `PathTemplate` 展开接到 `[dependencies]` 的 `path` 上**。用户写：
   ```toml
   [dependencies]
   "z42c.syntax" = { path = "${compiler_libs}/z42c.syntax.zpkg" }
   ```
3. 🔴 **依赖路径里的未知变量必须硬报错**，不能沿用 `PathTemplate` 今天「未知变量 → 保留字面」
   那条。保留字面会变成「目录不存在」，**症状离原因很远**（用户看到的是一个含 `${...}` 的怪路径
   或一句 not found，而真因是这个 z42c 不认识那个变量名）。
4. **运行期分两类，刻意不同**：
   - **用户项目** → 默认 `deploy`（= copy）⇒ 引用到的编译器库**被拷进用户的输出目录**
     （User 先前那条要求），拷出去就能跑，不依赖 SDK 在不在;
   - **SDK 内的程序** → `deploy = "shared"` + `probing-paths = "../z42c"` ⇒ **零复制**。
   为什么不能给用户项目也用 shared：probing 是**相对 entry 目录 + glob**、不展开环境变量 ⇒
   用户应用装在任意位置时，没有一条相对路径能指回 SDK。**copy 是那一侧唯一自洽的答案。**
### 5.2.1 🔴 `programs/z42c/` 是 driver 的**自包含闭包**（含整套 stdlib 副本）⇒ 必须排在框架之后

实测踩到：把编译器域目录并入 `libsDirs` 时若排在 `Z42_LIBS` **之前**，用户工程里
`z42.core` 等会**先在 `programs/z42c/` 命中** ⇒ `_bundleExeDeps` 按「不是从 shipped libs/ 找到的
⇒ 私有」把**整套 stdlib 拷进用户输出目录**（dist 从 1 个 zpkg 变成十几个；同工程不引用编译器域包
时只有 1 个 —— 对照实验）。⇒ 域目录**追加在最后**：域内包按构造不在 shipped `libs/`，排最后一样
命中；stdlib 则继续从框架解析、不复制。手写的 vendored zpkg 引用仍排框架**之前**（那是「我显式
指定的这一份压过碰巧同名的框架包」的原意）。

⚠️⚠️ **验这条时我第一次用错了环境**：`Z42_LIBS` 指了**开发树 flat**，而那里**有意**留着破环预建
（`_ensureBootstrapSelfDepLibs`）写进去的那六个包 ⇒ `z42c.syntax` 被判成框架、不复制，看起来像
「修坏了」。真实用户形态是 `Z42_LIBS=<sdk>/libs`（那里已归零）。⇒ **验「用户工程会怎样」必须用
SDK 的 libs，不能用开发树 flat。**

### 5.3.0 🔴 放宽解析域时的两个已知风险（PR ② 动手前必读）

今天那块并入逻辑（`Main.z42`，批 1）条件是 **`pm.Project.Kind == "analyzer"`**，注释写着两条理由：
「只对 analyzer 工程生效 ⇒ 普通工程看不见编译器域包（解析域隔离）」＋「**z42c 自建（kind=exe）
不进此块 ⇒ 自举 byte-identical**」。要支持「用户项目显式引用」就得放宽这个条件，于是：

1. ⚠️ **自举 byte-identity**：放宽后 z42c 自己的构建（kind=exe，清单里按名依赖 `z42.package` 等）
   也会进这块。缓解是**把解析域仍然追加在最后**（搜索首命中为准）⇒ workspace/成员 dist 先命中，
   字节不变。**改完必须做字节对账**（`xtask test compiler` 的不动点 + 指纹门），不能只看绿。
2. ⚠️ **「声明一个 ⇒ 看见全部」**：最省的实现是「工程只要显式声明了任一编译器域包，就把整个
   `programs/z42c/` 追加进它的 libsDirs」。代价是该工程随后也能隐式 `using` 其余编译器域命名空间。
   判断：这类工程已经主动 opt-in、不再是「普通工程」，可接受；**但要写进 reference 文档**，
   别让它变成一条没人知道的缝。若要更严，得改成**按依赖名定向解析**（不整目录并入），成本高一档。

### 5.3.1 🔴 编译**输出**里不许出现具体路径：写 `${Z42_HOME}` 占位符（User 2026-09-27 追加）

User 原话：「我上面说的是**编译期**能找到对应类库的办法；**编译输出应该没有 `compiler-libs`**，
要被替换为 `Z42_HOME` 这种通配符。」⇒ 两侧职责分清：

| 侧 | 谁解析 | 写什么 |
|---|---|---|
| **编译期**（找得到 zpkg）| z42c / z42b | 清单里写 `${compiler_libs}/…`；由 `Z42_COMPILER_LIBS` → `Z42_HOME/programs/z42c` → `Z42_PORTABLE_VM` 反推 → 开发树 依次解析成**本机真实目录** |
| **编译输出**（侧车 `probing-paths`）| VM，运行期 | **必须写成可移植占位符 `${Z42_HOME}/programs/z42c`** —— 不烤绝对路径、也不出现 `compiler-libs` 字样 |

为什么不能烤绝对路径：侧车随产物分发，SDK 换位置/换机器就失效；而 `probing-paths` 今天的展开是
**glob + 相对 entry 目录**，对「装在任意位置的用户应用要指回 SDK」无解。

**需要的 VM 改动**：`expand_probing_paths` 在 glob 之前先做占位符替换 —— 至少 `${Z42_HOME}`，
可含由 `Z42_PORTABLE_VM` 反推的 SDK 根。解析不出来的 pattern **跳过**（与现有「目录不存在就跳过」
同一语义），不要变成一个含 `${…}` 的假路径。

⚠️ **这是 VM 侧能力 ⇒ 受种子纪律**：必须 **support 先行（VM 先认 `${Z42_HOME}`）、晚一个 nightly
再 use（z42c 才开始往侧车里写占位符）**。否则上一代 VM 读到 `${Z42_HOME}/…` 会当字面目录、跳过，
症状是「probing 形同没配」。⇒ 本 change 的 PR ② 只做 support；emit 那一步挂阶段-2 欠账。

⭐ 于是「用户项目也能共享而非复制」这条路顺带打开了（占位符展开之后，用户项目也可以
`deploy = "shared"`）。但**默认仍是 copy** —— 拷出去就能跑、不依赖 SDK 在不在，是那一侧唯一
无前提的答案；shared 留给明确知道自己带 SDK 的用户。

### 5.3.2 验收：编译输出里不得出现具体路径

`grep -r "compiler-libs" <任意构建输出>` 必须为空；侧车里的 probing 值必须是 `${Z42_HOME}/…` 形态
（不是 `/Users/...` 或 `C:\...`）。这条要有测试 —— 否则「可移植」只是说法。

## 5.4 `compiler-libs` 的处置（User：没用到就删掉）

🔴 **实测挖出的根因（比「没用到」更具体）**：`compiler-libs/` **不是没人写**，是**两条组装路只有一条真的送到**：

| 路 | 有没有 compiler-libs |
|---|---|
| `xtask build sdk`（本地 `artifacts/.z42/`）| ✅ 有（`xtask_stdlib.z42` 里显式建 + 校验 `z42c.semantics.zpkg` 在位）|
| `xtask package sdk` → 发布 tarball | 代码里**也 stage 了**（`xtask_package_desktop.z42` + `packages.toml` 的 `sdk.include[3]`，kind `compiler-libs-glob`）…… |
| **实际装出来的 nightly SDK** | ❌ **没有这个目录**（2026-09-27 实测：`ls <sdk>/` 无 compiler-libs；`z42c.semantics.zpkg` 只在 `programs/z42c/`）|

⇒ 打包侧有一个真缺陷（可疑点：那段注释自己写着 staged 到 **`artifacts/publish/compiler-libs/compiler-libs/`** —— 双层同名目录）。
**本 change 删掉整个 `compiler-libs/` 机制，这个缺陷随之消失**，不必再单独修。

⚠️ 接线面 ~46 处 / 10 文件（`xtask_stdlib` 7、`xtask_selfcheck_packages_config` 11、
`xtask_stage_components` 5、`BuildPaths` 6、`Main.z42` 3、`packages.toml` 4、
`xtask_selfcheck_stage_components` 4、`xtask_package_desktop` 3、`xtask_compiler_e2e_analyzer` 2、
compiler workspace 1）。⚠️ `packages.toml` 的组件**索引会前移**，`xtask_selfcheck_packages_config`
里那些 `sdk.include[N]` 断言必须同步（它自己注释就记着「批 1 插在 stdlib 之后 ⇒ 其后各项索引 +1」）。

## 6. 已裁决（User 2026-09-27）

| # | 裁决 |
|---|---|
| ① 落点/解析域 | **扁平放 `programs/z42c/`，不新增 `compiler-libs/`**（「之前很早就说过，避免复杂」）⇒ 顺带修掉发布态空解析域，见 4.1 |
| ② 改名不并进来 | 是。本 change **只挪不改名** ⇒ zpkg 文件名不变 ⇒ 零跨 nightly（改名仍归 `add-package-roles` 批 3）|
| ③ `z42.scripting` 一起挪 | 是 ⇒ `ExeDeps` 不需要改，见 4.3 |

**PR 拆分**（两个，原第三个被 ③ 消掉）：

1. **挪 + 接线**：六个包 `git mv` 进 `src/compiler/`、两个 workspace 的 `default-members` 对调、
   z42b / z42i / z42.repl / launcher / workload / devtools / xtask 各加 path 依赖
   （⚠️ z42b 清单里「z42.build/z42.project 均为 stdlib → 不破 stdlib-only」那段注释同 commit 改掉）。
2. **发布面**：SDK 组装落 `programs/z42c/`；z42b / launcher / workload / devtools 各加
   `probing-paths = "../z42c"` + 依赖条目标 `deploy = "shared"`（**z42i 已是此形态，照抄**）；
   显式引用解析域改指该目录；+ §7 的两段式门。

## 7. 验收信号（机制真替代了约定，才算做完）

1. **`src/libraries/README.md` 那段「两类库（别混淆）」脚注能删掉** —— 这正是 `add-package-roles`
   批 3.4 定的验收信号，本 change 提前兑现一半：物理位置替代了那段散文。
2. 🔴 **新门必须是两段式**（一段都不能少，否则守的是半个东西）：
   - **不声明就看不见**：普通工程裸写 `using Z42.Syntax;` + 用其中的类型 ⇒ **`E0443 undefined type`**
     （2026-09-27 实测）。⚠️ 本文早先写的是 `E0494 命名空间不存在` —— **实测不是那个码**：
     `using` 那一行自己不报，卡在类型解析上。门要断言**真实发生的码**，不是我以为的码；
   - **声明了就能用，且产物自洽**：同一工程在 `[dependencies]` 里加上该包 ⇒ 能编、**dist 里有那个
     zpkg**、拷到别处能跑（opt-in 真的通了）。
   ⚠️ 必须覆盖 **`z42.scripting` 这条传递路径**：用户只显式引用 scripting，`z42c.core`/`z42c.syntax`/
   `z42.build` 要靠「拷了才递归」自动带进输出。它是构造式成立的（4.3），但**正因为是白送的就更要有
   测试**——哪天有人「优化」掉那条递归，症状会是用户现场 `MissingSymbolException` 而本地全绿。
   本仓库反复吃过的亏是「以为有门，其实没有」⇒ 立门时按惯例**注入假断言验判别力**。
