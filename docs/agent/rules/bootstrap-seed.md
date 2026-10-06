# 自举种子依赖：编译器 / xtask 的鸡蛋问题

> 触发条件：**改动任何「构建工具自己也要被构建」的链路**——尤其是 z42c（自举编译器）、
> xtask（z42 写的 dev CLI）、stdlib（被前两者依赖）三者的 build / seed / bootstrap 路径。
> 这条规则补齐 [workflow.md](workflow.md) 缺失的**自举维度**：删一个种子兜底前，必须先确认每个入口仍有种子。

---

## 鸡蛋问题（一句话）

**z42c / xtask / stdlib 互为前置：xtask 用 z42 写、要 z42c + stdlib 才能编；z42c 自身的构建又经
xtask / build 基础设施驱动；stdlib 又被两者依赖。任何「从源码全新构建」的入口，都必须先有一个
*已存在的种子*（seed = 一套能跑的 z42vm + z42c.driver.zpkg + stdlib dist），否则无路可走。**

种子有两种来源：

| 来源 | 何时用 | 例子 |
|------|--------|------|
| **warm 种子** | 本地已建过 / CI 有缓存 / 上游 nightly 已下载 | `artifacts/build/z42c/.../z42c.driver.zpkg` 存在 → z42c 自建 z42c |
| **cold 种子** | fresh checkout / CI 全新 runner，**没有任何 in-tree z42c 产物** | 下载 nightly（`install-z42.sh` / CI `setup-z42-sdk` → 都是 `./.z42`）→ `_ensureSeed` 把 `programs/z42c` + `libs` 供种到 in-tree |

> **cold 种子的统一解析**：
> `build compiler` / `build stdlib` 冷启动由 `_ensureSeed`
> （`scripts/common/xtask_common.z42`，SDK 定位在 `_seedSdkDir`）按 **`Z42_HOME`
> （`--toolchain` 设它，或 launcher/install 设）→ 运行 xtask 的 apphost SDK
> （`Z42_PORTABLE_VM` 反推）→ `./.z42`** 找到 SDK，把 `programs/z42c` + `libs` 拷进 in-tree
> 再自建。**CI 与本地同一条 resolver、同一个位置**：CI（`.github/actions/setup-z42-sdk`）把 nightly 装进
> 仓库根 `./.z42`——与本地 `install-z42.sh` 相同，不设 `Z42_HOME`，xtask 也跑在这份 SDK 上；本地 `install-z42.sh` 后 `xtask build compiler` 开箱即用。warm 树（in-tree 已有种子）**不被覆盖**——gen2 字节不动点靠"第二次从 in-tree
> gen1 再种"收敛，故 in-tree 必须最高优先。managed 布局的 `Z42_HOME`（`runtimes/`，无
> `programs/`）不符 SDK-toolchain 布局 → 跳过（不误当种子源）；`Z42_LIBS` 显式覆盖仅在其
> 确实含 `z42.core.zpkg` 时生效。
---

## 核心约定（必须遵守）

**删除任何「构建期种子 / 兜底路径」，必须与「为所有入口提供替代种子来源」作为同一个原子变更一起做——
绝不可拆成两个 commit、两个 change，或「先删兜底，回头再补种子」。**

判定「所有入口」时，**cold-start 入口最容易漏**：

- [ ] 本地 fresh checkout（删了 `artifacts/` 后第一次构建）
- [ ] CI 每条全新 runner 上 build stdlib / build z42c / package / golden regen 的 job
- [ ] download-bootstrap 类 gate（`test-vm-jit` / `test-stdlib-jit`；job key 仍为 `vm-jit-consistency` / `stdlib-jit-consistency`）
- [ ] 打包矩阵（`package-{android,ios,wasm}` 等也会冷建 stdlib）

只要其中任一入口在删除后**没有种子来源**，该入口就会 `error: no <X> seed` 全红。

---

## 删种子前自检清单

改任何 `_buildCompiler*` / `_buildStdlib*` / `bootstrap-*.sh` / CI 的 download-nightly 步骤前：

1. **列出此路径当前的种子来源**（warm？cold？两者？）
2. **若要删 cold 兜底**：先确认每个 cold 入口（上面清单）已切到「下载 nightly 种子」或「committed 种子」——
   **种子供给的 PR 必须先合并 / 同一 commit 落地，再删兜底**。
3. **本地不可验的部分（CI / packaging）**：本地只能验 warm 路径；cold 路径只能靠 CI。
   因此删 cold 兜底的 commit **push 后必须盯 CI**，红了立即回滚或补种子。
4. **格式漂移风险（格式维度由两代自举根治）**：
   下载的 nightly 种子其 zbc / zpkg 格式与当前 z42vm 的 strict-pin 不同时，`ci-bootstrap` 的**版本差
   gate**（读种子 `programs/z42c/z42c.driver.zpkg` header minor vs 源码 `ZpkgWriterZ.Minor`）检测到
   不等就走**两代自举**：用 SDK 自带的**旧 VM**（`bin/z42vm`，与旧种子同版本、能读旧种子）跑
   Gen1（旧种子 z42c 编当前源→旧壳/新逻辑）+ Gen2（旧 VM 跑 gen1 z42c→新格式产物），再交新 VM
   接管。runtime stdlib（entry-dir 旧）与 compile stdlib（`Z42_LIBS` 新）分离解开死锁（design D7）。
   → **格式 bump 的 build-and-test / toolchain-bootstrap / package 路径 CI 自动过、免手动传种子**。
   > **残留**：纯 download-bootstrap 的 job（vm-jit / bench 等，不 feed publish-nightly）在 bump 当次
   > 仍短暂红一跑，等新 nightly 发布自愈——不阻塞发布链。删 cold 兜底照旧**不要踩在 format bump
   > 同一周期**（该残留窗口期）。
   >
   > **`DepScanCache` 与两代自举**：gen0→gen1(stdlib) / gen1→gen2(compiler) **就地覆写同一
   > `artifacts/build/{libraries,compiler}`**；`DepScanCache.Get` 按 **path + (size, mtime_ms)** 作答，
   > 命中但文件被覆写过即重读重解、并作废该条目的 Tsig/Mods/Types，于是「进程内覆写 zpkg 后重扫」不会
   > 返回陈旧 null，两代自举不依赖外部清理。回归测试 `src/compiler/z42c.pipeline/tests/depscancache/`
   > 按真实形状复现（旧 minor 头 → 缓存 null → 就地覆写成当前 minor → 必须重解）。
   > ⚠️ 残余窗口：mtime 只有毫秒粒度，「同毫秒内覆写成同样大小的另一份内容」测不出来
   > （make/ninja/rustc 同款取舍）；两代自举之间隔着整轮构建，不在这个窗口里。
   >
   > **`ci-bootstrap` §1.5 每代构建前清空其就地 artifacts**：因这条路径**本地不可验**（冷启动 + 格式 bump
   > 才走到），暂时保留。**ToDo**：下一次格式 bump 的 PR 里顺手删掉它并观察 CI —— 那时它正好被真实行使一次，
   > 红了也立刻知道是谁的锅。

---

## 现场案例

把 `_buildCompiler` 改成「缺种子即 `return 1`」的自种子，**但没动调用它的 `_buildStdlibCore` 冷启动分支**——
该分支仍把它当兜底调用。结果：CI fresh checkout 没有 z42c 种子 → `error: no z42c seed` →
**所有冷建 stdlib 的 job 全红**（build-and-test ×4 OS + package-{android,ios,wasm} + download-bootstrap gate）。

根因正是违反核心约定：**cold 兜底被删，但 CI seed-provisioning（下载 nightly）还没落地**——两者是耦合原子步，
被拆开了。

---

## 分阶段引入新语法 / zbc·zpkg 格式（自举跨版本）

> 这条是「鸡蛋问题」在**语言 / 格式演进**维度的解：**上一个已发布 nightly 的 z42c 永远能编当前 main 源码**，
> build-and-test 即「下载上一版 nightly → 自举当前源码」，无死锁。

### 鸡蛋问题（语言 / 格式维度）

自举编译器加新语法 / bump zbc·zpkg 格式时：当前源码若**立即使用**新语法 / 新格式，则**只有已经懂新
语法·格式的编译器**才能编它——而那个编译器还没发布（要靠这次构建产出）。死锁。

### 核心约定：support 与 use 必须分两个 release（必须遵守）

**任何新语法 / 新 zbc·zpkg 格式，分两阶段落地，跨两个 nightly：**

1. **阶段 1 —— 落「支持」**：给 z42c 加新语法的 lexer/parser/codegen（或新格式的 writer/reader），但
   z42c 自身源码 + stdlib + xtask **仍只用旧语法 / 仍产出旧格式**。
   → 上一个 nightly 的 z42c 能编这份源码 → 产出「**支持**新语法·格式」的新 z42c → 发布新 nightly。
2. **阶段 2 —— 落「使用」**：新 nightly 发布后，**才**在 z42c / stdlib / xtask / 用例里**使用**新语法、
   或让构建**产出**新格式。→ 刚发布的 z42c（阶段 1 能力）能编。

**「新 nightly 发布了」怎么确认**：nightly 必须**包含**阶段 1 的提交，不是「发布时间晚于合并时间」——
`gh api repos/z42-lang/z42/compare/<阶段1 的 main 提交>...$(gh api repos/z42-lang/z42/git/ref/tags/nightly -q .object.sha) -q .status`
为 `ahead` 或 `identical` 才算。阶段 1 合并后不必盯着它的 main 运行：改到 SDK 路径的 main 运行不会被后来的
合并取消，一定跑完发布（机制见 [ci.md](../../internals/src/devinfra/ci.md)「main 上的运行不互相打断」）。

> 🔴 那条保护按路径判「影响 SDK」：文档、`*.md`、`tests/` / `bench/` 下的测试源码与夹具、`examples/`、
> `scripts/test/` 等算**不影响**，会被后来的运行替换。**决定 SDK 内容的逻辑不要放进这些路径**；新增一类
> 不进 SDK 的路径时，同步改 `.github/ci/main-supersede.sh` 的 `non_sdk_re` 与 ci.md 的那张表。

### 边界的第二根轴：stdlib API 面

种子约束不止语法/格式——CI 冷启动（`.github/actions/ci-bootstrap` step 2/3）用**种子 z42c +
种子 stdlib** 编当前 xtask 源与 z42c 源，因此这两个源码域**引用的 stdlib API 也被上一 nightly
钉死**：

- **xtask / z42c 源新用一个 stdlib API**：该 API 必须已随某个 nightly 发布（加 API 本身随时可做，
  用它要晚一个 nightly——与语法同款纪律）。
- **删/改 xtask / z42c 在用的 API**：两阶段跨两个 nightly——阶段 1 加新 API、**旧 API 暂留**
  （种子例外，非兼容层）、调用点不动 → nightly 发布 → 阶段 2 切全部调用点 + **同一提交删旧 API**。
- stdlib 源自身不受此轴约束（它由自建的当前 z42c 编译）。
- **xtask 用到的编译器域 API（`Z42.Project` / `Z42.Build`）同属此轴**：
  `scripts/xtask.z42.toml` 把它们声明为 SDK 库（`deploy = "sdk"`，add-sdk-libs），编译与运行都用**编出 xtask 的那份
  SDK**（本地与 CI 都是仓库根 `.z42/` = 上一 nightly）的 `programs/z42c/`，不从源码代建。⇒ 给这两个包**加** API 随时可做，xtask
  **用**它要晚一个 nightly；**删/改** xtask 在用的 API 走上面的两阶段。另一个后果：改布局规则
  （`BuildLayout` / `WorkspaceLayout`）的那次合入里，CI 上的 xtask 仍按上一 nightly 的规则算路径 ——
  xtask 侧的跟进放到下一个 PR。

> 🔴 **阶段 2 最容易被忘掉——因为忘了不会红。** 阶段 1 的过渡形态（字面量 / 旧 API 并存）能一直跑下去，
> 没有任何东西提醒你回来收尾，于是过渡形态**沉淀成常态**（例：诊断码用字面量发码绕开 `DiagnosticCodes`
> 登记表，长出一码两义）。`xtask check diagcodes` 的**规则 ⑥**给每条过渡项挂了到期日（挂账超
> 3 天即红），把「阶段 2 该做了」变成一个会自己响的信号，而不是靠谁记得
> （见 [test-gate.md](../../internals/src/devinfra/test-gate.md)）。
> **新开一个分阶段引入时，先想好阶段 2 由什么来提醒你**——没有提醒就等于没打算做。
>
> ✅ 在过渡形态所在文件写一行
> `// STAGE2-DEBT(<tag>): <阶段 2 要做的那件事>`，`xtask check stage2 --update` 记进
> `scripts/test/stage2-debt.txt`；门做双向棘轮（源里多一条/清单多一条都红）+ 挂账超 7 天即红。
> ⚠️ **它只看得见带标记的债** —— 这个边界写在门的头注里。
>
> 🔴 **阶段 2 的收尾不止于代码：过渡形态也写在散文里，而散文不会跟着切回。** 登记表注释里若写着
> 「XX 层用字面量 `E04xx` 发码」，代码切回后它们一夜之间全成假话，而没有门盯着散文。
> **规则 ⑦**因此禁止在登记表里断言发射形态：形态的唯一 SoT 是那份由实扫重生成的清单。
> **收尾时问一句：我刚消灭的那个过渡形态，还被写在哪里当成现状？**

可操作的完整提交剧本（判定 grep / 两个 commit / 等 nightly 的检查命令）见
[`docs/internals/src/devinfra/testing.md`](../../internals/src/devinfra/testing.md)
「stdlib 破坏性 API 变更」。

### 边界的第三根轴：z42c 运行期自依赖一个 stdlib 库

比 API 面更隐蔽：**当 z42c 把自身建构期依赖的代码（IR 模型 / zpkg 后端 / 等）下沉进一个
z42c *自己运行期就要用* 的 stdlib 库**（如把 `z42c.ir`+`z42c.project`
收敛成 stdlib 单库 `z42.package`），就出现**自依赖环**：z42c 建任何 zpkg 都要调 `z42.package` 的
`ZpkgBuilder`，而 `z42.package` 本身由 z42c 构建。冷启动 flat dist 里还没有它，且上一 nightly 种子只把
等价代码作**旧包名**（`z42c.ir`/`z42c.project`）携带 → fresh z42c 被编成钉在种子旧包上的调用，
运行期加载真库时 `undefined function`（**这类漏网正因 `xtask test compiler bootstrap` 只「编」不「跑」
新建出来的 z42c**——它验语法/格式/非自依赖库的 API 越界，但从不执行产物，故运行期自依赖问题看不见；
这条只能靠 CI 冷启动全栈重建暴露——每个跑 `ci-bootstrap` 的 job
都用刚建出的 gen1 z42c 编 stdlib 与 golden，即真的**运行**了它；`compiler-checks` 再在同一份冷启动产物上跑 gen1→gen2）。

- **判据**：本次改动是否让 z42c 的**源**新 `using` 一个「z42c 运行期就要加载」的 stdlib 库，而该库
  **上一 nightly 种子里不以同名 zpkg 存在**？是 → 踩轴 ④。
- **破环**（已实现，非纪律）：`_ensureBootstrapSelfDepLibs`（`scripts/build/xtask_compiler.z42`）在建 z42c **前**用当前 driver 把当前源的
  `z42.core` → `z42.project` → `z42.build` → `z42.package` → `z42c.core` → `z42c.syntax`
  逐个单独编进 build-libs。**不 warm-skip**。机制全文见
  [`docs/internals/src/compiler/self-hosting.md` 轴 ④](../../internals/src/compiler/self-hosting.md)。

> ⭐ **轴 ③ 对这 6 个自依赖库不成立**：破环预建总是用
> **当前源**重建它们，故「z42c 源用这 6 个库的**新 API**」**无需等一个 nightly**，加 API 与用 API
> 可以同一个 commit。这已是日常操作——先例 `ExportedClassZ.IsSealed` / `Visibility` /
> `IsDeprecated` / `ExportedMethodZ.TypeParamCount` /
> `StrMap.Find`，全部同 commit 加+用、CI 绿。
>
> **轴 ③ 的「晚一个 nightly」纪律仍然适用于**：① **其余 stdlib 库**（`z42.collections` /
> `z42.threading` / …，预建不覆盖）；② **xtask 源**（`ci-bootstrap` step [2] 用种子 stdlib 编 xtask，
> 早于任何预建 → xtask 最受约束，见 self-hosting.md「为什么 xtask 最受约束」）。
>
> **残余真约束**：给这 6 个库的既有导出类型加字段，新字段**不得进 ctor 签名**，须 ctor 内给默认值 +
> 消费方构造后赋值（种子 ABI）。违反 = 旧种子构造调用元数对不上。
> 🔴🔴 **那条豁免只对「增量」成立 —— 改名 / 删除不在内**。
>
> 破环预建用**当前源**重建那 6 个库，所以「加一个新 API 并同 commit 用它」没问题：新的加上了、
> **旧的还在**，上一版 driver 二进制运行期照旧解析得到。但**重命名或删除**会抹掉旧 FQN，
> 而那个 driver 正是拿来跑这轮 bootstrap 的 ⇒ 它在中途就死。
>
> 实测（想把 `Z42.Build.Project` ↔ `Z42.Package` 互换）：改完源码
> `xtask build stdlib` 当场红在
>
> ```
> Std.MissingSymbolException: undefined function `Z42.Build.Project.ManifestLoader.Load$1$string`
>   at Z42.Driver._build (Main.z42:166)
> ```
>
> 两个方向都踩（`Z42.Package.ZpkgWriterZ` 种子也在调）。**⇒ 动这 6 个库里「driver 运行期会调」的
> 符号的名字或存在性，仍是跨 nightly 的两/三阶段改动**，不受本节豁免保护。
>
> ⭐ 顺带测出的一条：把旧 `<pkg>.zpkg` 拷进 driver **自己的 dist**（搜索序
> `[entry-dir] ++ probing ++ [libs]`，entry-dir 最优先）后，带改名的 stdlib 重建就过去了 ——
> 「让编译器对它运行期要用的 stdlib 库自包含」能消掉这一整类危险。注意 `xtask_stdlib.z42` 步骤 4
> 的注释本来就写着要给 driver「稳定 stdlib 快照」，而 `fresh member 从 workspace dist 解析、
> **优先于**快照`恰好把那个意图吃掉了。

- **教训**：**新增/收敛「z42c 自依赖的 stdlib 库」的 change，冷启动路径本地必验**（下载上一 nightly
  作种子跑一遍 cold `build compiler` + `build stdlib`），别只验 warm 就推 main。

### 边界的第四根轴：**改名**（别名形态要按轴选）

改名不是增量（见上一节 🔴🔴），所以它一定要一个「让两代并存」的兼容物。**但兼容物有三种形态，
选错就是白干一轮**，判据只有一条：

> **编译期与运行期是两个独立的解析面，一件兼容物通常只盖住其中一个。**
>
> | 面 | 按什么解析 | 依据 |
> |---|---|---|
> | 编译期 | 命名空间（`using` + FQN），**不看包文件名** | z42c 语义层（using-scoped 解析） |
> | 运行期 | **zpkg 文件名**（消费者 `DEPS` 段记的名字），**不看命名空间** | `runtime/src/metadata/loader/namespace.rs` 的 `resolve_dependency`：「the VM's lazy loader **no longer routes by namespace; it uses zpkg file names**」 |

于是**别名形态必须跟着「你改的是哪根轴」走**：

| 改的是 | 别名形态 | 先例 |
|---|---|---|
| **包名（文件名）**，命名空间没动 | 同内容**两个文件名** | `z42.ir` → `z42.package`|
| **命名空间**，文件名没动 | 只能**旁置**一份到消费者的 entry 目录（塞进同一个包 = 同短名类串味，见下面 ❌ 第二条）| 本节 |

⇒ 命名空间改名时，这两条已实测否证，别再试：

- ❌ **另立一个兼容包**（旧名副本换个包名）：编译期够用，**运行期是空气** —— 没有任何 `DEPS`
  指向那个新文件名，它永不被加载。
- ❌ **同一个包里同时声明两套命名空间**：同包内两套**同短名类**会串味（`E0401: no field … on
  <Class>`，短名键混同），**且种子 z42c 一样如此** ⇒ 阶段 1 的源码根本编不出来，修当前 z42c 也没用。

✅ **可行形态 = 编译期 overlay + 运行期旁置**：
用种子 z42c 先把**当前源**那个库编出来、覆盖进一份种子 libs 的副本供编译期用；再把这份新名产物
**`cp` 到消费者 zpkg 旁边**（搜索序 `[entry-dir, Z42_LIBS, probing]`，entry-dir 最优先）供运行期用。
两件齐了，改名就能**一步落**，不必拆成跨 nightly 的三步。

⚠️ 验这类接线的对照实验，**命令必须真的会触碰那个符号**：懒加载没走到就撤掉兼容物也照样绿，
那是空门（实测踩过：`xtask check stage2` 不读清单 ⇒ 对照无效，换 `build stage-toolchain` 才红）。

**铁律**：当前 main 的源码，**任何时刻都不得使用比「上一个已发布 nightly 的 z42c」更新的语法 / 格式**。
违反 = 跨版本自举断链。

### z42c 自举能力版本号 + 种子校验

- z42c 带一个**自举能力版本号**（bump 时机：新增语法 / 新增 zbc·zpkg 格式即 +1）。
- bootstrap 下载 nightly 种子时，校验种子 z42c 版本 **≥ 当前源码要求的最低版本**；不符 → 明确报错
  「种子太旧，等新 nightly 发布后再用新语法」，而非莫名编译失败。
- zbc·zpkg 的 strict-pin（z42vm 精确匹配 writer 的 major+minor）已是**格式**维度的天然校验；本版本号补的是
  **语法能力**维度。

### 边界检查（每次改完编译器/语言/格式相关代码必跑）

**`xtask test compiler bootstrap [rid]`**：用**已发布 nightly 的 z42c**（下载）和**仓库当前 z42c** 分别编译当前
z42c 源码，确认上一个 nightly 仍能编当前源 → 没有「用了比已发布 nightly 更新的语法/格式」的越界。
（gh/tar 作外部子进程，逻辑在 `scripts/build/xtask_bootstrap_check.z42`；需 `gh` 已登录。）

- ✅ nightly z42c 编通当前源 = 无越界，分阶段纪律守住。
- ❌ nightly z42c 编不过、仓库 z42c 编得过 = **越界**：当前源用了新语法/新格式，但 nightly 还不支持 →
  按上面「support 先行、use 晚一 release」拆分，或回退过早的使用。

**何时跑**：改动 z42c（parser/lexer/codegen/zbc·zpkg writer）、加新语法、bump 格式后；CI 的
冷启动全栈重建（`ci-bootstrap`：下载 nightly → 重建全栈）是其全量版，本脚本是开发期快速本地版。

### 为什么这与「不为旧版本提供兼容」不冲突

[philosophy.md](philosophy.md) 的「不做兼容」是**不写兼容代码 / 不留旧路径**。本约定不写任何兼容代码——
它是**纪律**（晚一个 release 再用新语法），不是兼容层。z42c 永远只懂一个语法 / 格式版本；旧 nightly 能编当前
源码，纯粹因为当前源码**克制**着没用新东西，而非 z42c 兼容了旧的。**快速开发期照样不做兼容、实现保持最简。**

---

## 与其他规则的关系

- **[philosophy.md](philosophy.md) 不为旧版本提供兼容**：种子的「format 漂移」是该规则的例外——nightly 种子是
  *跨进程的二进制接口*，删兜底要尊重发布周期，不能假设旧种子永远可读。**分阶段引入纪律（见上）正是让这个
  「发布周期」可控的前提。**
- **[workflow.md](workflow.md) 阶段 5 GREEN**：cold 路径本地不可验 → 该路径的「全绿」判定**以 CI 为准**，
  不是本地 warm 跑通就算数。
- **设计原理**（为什么自举需要种子、warm/cold 两态如何切换）落在 [`docs/internals/src/compiler/self-hosting.md`](../../internals/src/compiler/self-hosting.md)，
  本文件只管「改动时如何避免踩坑」的流程约束。
