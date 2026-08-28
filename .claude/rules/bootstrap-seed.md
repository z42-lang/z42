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
| **cold 种子** | fresh checkout / CI 全新 runner，**没有任何 in-tree z42c 产物** | 下载 nightly（`install-z42.sh` → `./.z42` / CI `ci-bootstrap` → SDK）→ `_ensureSeed` 把 `programs/z42c` + `libs` 供种到 in-tree |

> **cold 种子的统一解析（2026-07-04；env 于 2026-07-05 simplify-compiler-build 折叠）**：
> `build compiler` / `build stdlib` 冷启动不再报错，由 `_ensureSeed`
> （`scripts/common/xtask_common.z42`，SDK 定位在 `_seedSdkDir`）按 **`Z42_HOME`
> （`--toolchain` 设它，或 launcher/install 设）→ 运行 xtask 的 apphost SDK
> （`Z42_PORTABLE_VM` 反推）→ `./.z42`** 找到 SDK，把 `programs/z42c` + `libs` 拷进 in-tree
> 再自建。**CI 与本地同一条 resolver**：CI（`.github/actions/ci-bootstrap`）只设
> `Z42_HOME=<下载的 SDK>`，不再手动拷种子；本地 `install-z42.sh` 后 `xtask build compiler`
> 开箱即用。warm 树（in-tree 已有种子）**不被覆盖**——gen2 字节不动点靠"第二次从 in-tree
> gen1 再种"收敛，故 in-tree 必须最高优先。managed 布局的 `Z42_HOME`（`runtimes/`，无
> `programs/`）不符 SDK-toolchain 布局 → 跳过（不误当种子源）；`Z42_LIBS` 显式覆盖仅在其
> 确实含 `z42.core.zpkg` 时生效。（`Z42C_DIR` / `Z42_TOOLCHAIN` 已于 simplify-compiler-build
> 折叠进 `Z42_HOME`，不再存在。）

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

改任何 `_buildCompiler*` / `_buildStdlib*` / `bootstrap-*.sh` / CI 的 setup-dotnet / download-nightly 步骤前：

1. **列出此路径当前的种子来源**（warm？cold？两者？）
2. **若要删 cold 兜底**：先确认每个 cold 入口（上面清单）已切到「下载 nightly 种子」或「committed 种子」——
   **种子供给的 PR 必须先合并 / 同一 commit 落地，再删兜底**。
3. **本地不可验的部分（CI / packaging）**：本地只能验 warm 路径；cold 路径只能靠 CI。
   因此删 cold 兜底的 commit **push 后必须盯 CI**，红了立即回滚或补种子（见案例）。
4. **格式漂移风险（格式维度已由两代自举根治，2026-07-09 fix-bootstrap-format-bump-deadlock）**：
   下载的 nightly 种子其 zbc / zpkg 格式与当前 z42vm 的 strict-pin 不同时，`ci-bootstrap` 的**版本差
   gate**（读种子 `programs/z42c/z42c.driver.zpkg` header minor vs 源码 `ZpkgWriterZ.Minor`）检测到
   不等就走**两代自举**：用 SDK 自带的**旧 VM**（`bin/z42vm`，与旧种子同版本、能读旧种子）跑
   Gen1（旧种子 z42c 编当前源→旧壳/新逻辑）+ Gen2（旧 VM 跑 gen1 z42c→新格式产物），再交新 VM
   接管。runtime stdlib（entry-dir 旧）与 compile stdlib（`Z42_LIBS` 新）分离解开死锁（design D7）。
   → **格式 bump 的 build-and-test / toolchain-bootstrap / package 路径 CI 自动过、免手动传种子**
   （实测 0.25→0.30 连续 5+ 次真实 bump 全绿）。机制见
   [`docs/spec/archive/…-fix-bootstrap-format-bump-deadlock`](../../docs/spec/archive/)。
   > **残留**：纯 download-bootstrap 的 job（vm-jit / bench 等，不 feed publish-nightly）在 bump 当次
   > 仍短暂红一跑，等新 nightly 发布自愈——不阻塞发布链。删 cold 兜底照旧**不要踩在 format bump
   > 同一周期**（该残留窗口期）。

---

## 现场案例（2026-06-24 fix f8a16812）

`d4471a85` 把 `_buildCompiler` 从「dotnet 现编 z42c」改成「C#-free 自种子（缺种子即 `return 1`）」，
**但只改了函数、没动调用它的 `_buildStdlibCore` 冷启动分支**——该分支仍把它当「C# z42c 兜底」调用。
结果：CI fresh checkout 没有 z42c 种子 → `error: no z42c seed` → **所有冷建 stdlib 的 job 全红**
（build-and-test ×4 OS + package-{android,ios,wasm} + 3 个 download-bootstrap gate）。

根因正是违反核心约定：**cold 兜底被删，但 CI seed-provisioning（下载 nightly）还没落地**——两者是耦合原子步，
被拆开了。修复：恢复 `_csharpBuildCompilerZ42Seed`（cold 用 C# 现编 z42c），warm 路径保持 C#-free 不变。
彻底删 C# cold 兜底，要等 CI 全面切到下载 nightly 种子那一刻**同时**做。

---

## 分阶段引入新语法 / zbc·zpkg 格式（自举跨版本 —— 彻底删 C# 种子的关键，2026-06-25）

> 这条是「鸡蛋问题」在**语言 / 格式演进**维度的解。没有它，跨版本自举只能靠 C#
> （永远从源码现编、永远当前能力）；有了它，**上一个已发布 nightly 的 z42c 永远能编当前 main 源码**，
> C# 种子可彻底移除——build-and-test 改「下载上一版 nightly → 自举当前源码」即可，无死锁。

### 鸡蛋问题（语言 / 格式维度）

自举编译器加新语法 / bump zbc·zpkg 格式时：当前源码若**立即使用**新语法 / 新格式，则**只有已经懂新
语法·格式的编译器**才能编它——而那个编译器还没发布（要靠这次构建产出）。死锁。C# 一直当种子，正因它
每次从源码重编、永远具备当前能力，绕开了这个环。

### 核心约定：support 与 use 必须分两个 release（必须遵守）

**任何新语法 / 新 zbc·zpkg 格式，分两阶段落地，跨两个 nightly：**

1. **阶段 1 —— 落「支持」**：给 z42c 加新语法的 lexer/parser/codegen（或新格式的 writer/reader），但
   z42c 自身源码 + stdlib + xtask **仍只用旧语法 / 仍产出旧格式**。
   → 上一个 nightly 的 z42c 能编这份源码 → 产出「**支持**新语法·格式」的新 z42c → 发布新 nightly。
2. **阶段 2 —— 落「使用」**：新 nightly 发布后，**才**在 z42c / stdlib / xtask / 用例里**使用**新语法、
   或让构建**产出**新格式。→ 刚发布的 z42c（阶段 1 能力）能编。

### 边界的第二根轴：stdlib API 面（2026-07-02 补）

种子约束不止语法/格式——CI 冷启动（`.github/actions/ci-bootstrap` step 2/3）用**种子 z42c +
种子 stdlib** 编当前 xtask 源与 z42c 源，因此这两个源码域**引用的 stdlib API 也被上一 nightly
钉死**：

- **xtask / z42c 源新用一个 stdlib API**：该 API 必须已随某个 nightly 发布（加 API 本身随时可做，
  用它要晚一个 nightly——与语法同款纪律）。
- **删/改 xtask / z42c 在用的 API**：两阶段跨两个 nightly——阶段 1 加新 API、**旧 API 暂留**
  （种子例外，非兼容层）、调用点不动 → nightly 发布 → 阶段 2 切全部调用点 + **同一提交删旧 API**。
- stdlib 源自身不受此轴约束（它由自建的当前 z42c 编译）。

可操作的完整提交剧本（判定 grep / 两个 commit / 等 nightly 的检查命令）见
[`docs/workflow/testing/verify-by-change.md`](../../docs/workflow/testing/verify-by-change.md)
「stdlib 破坏性 API 变更」。

### 边界的第三根轴：z42c 运行期自依赖一个 stdlib 库（2026-07-22 补）

比 API 面更隐蔽：**当 z42c 把自身建构期依赖的代码（IR 模型 / zpkg 后端 / 等）下沉进一个
z42c *自己运行期就要用* 的 stdlib 库**（如 `converge-z42c-ir-metadata` 把 `z42c.ir`+`z42c.project`
收敛成 stdlib 单库 `z42.ir`），就出现**自依赖环**：z42c 建任何 zpkg 都要调 `z42.ir` 的
`ZpkgBuilder`，而 `z42.ir` 本身由 z42c 构建。冷启动 flat dist 里还没有它，且上一 nightly 种子只把
等价代码作**旧包名**（`z42c.ir`/`z42c.project`）携带 → fresh z42c 被编成钉在种子旧包上的调用，
运行期加载真库时 `undefined function`（**这类漏网正因 `xtask test bootstrap` 只验语法/格式/API
越界，不跑 z42c 的运行期自依赖**）。

- **判据**：本次改动是否让 z42c 的**源**新 `using` 一个「z42c 运行期就要加载」的 stdlib 库，而该库
  **上一 nightly 种子里不以同名 zpkg 存在**？是 → 踩轴 ④。
- **破环**（已实现，非纪律）：`_ensureBootstrapZ42Ir`（`scripts/build/xtask_compiler.z42`）在建 z42c
  **前**用种子 driver 先把当前源的该库单独编进 build-libs（两代自举，warm 幂等跳过）。机制全文见
  [`docs/design/compiler/self-hosting.md` 轴 ④](../../docs/design/compiler/self-hosting.md)。
- **教训**：**新增/收敛「z42c 自依赖的 stdlib 库」的 change，冷启动路径本地必验**（下载上一 nightly
  作种子跑一遍 cold `build compiler` + `build stdlib`），别只验 warm 就推 main。

**铁律**：当前 main 的源码，**任何时刻都不得使用比「上一个已发布 nightly 的 z42c」更新的语法 / 格式**。
违反 = 跨版本自举断链 = 被迫退回 C# 种子。

### z42c 自举能力版本号 + 种子校验

- z42c 带一个**自举能力版本号**（bump 时机：新增语法 / 新增 zbc·zpkg 格式即 +1）。
- bootstrap 下载 nightly 种子时，校验种子 z42c 版本 **≥ 当前源码要求的最低版本**；不符 → 明确报错
  「种子太旧，等新 nightly 发布后再用新语法」，而非莫名编译失败。
- zbc·zpkg 的 strict-pin（z42vm 精确匹配 writer 的 major+minor）已是**格式**维度的天然校验；本版本号补的是
  **语法能力**维度。

### 边界检查（每次改完编译器/语言/格式相关代码必跑）

**`xtask test bootstrap [rid]`**：用**已发布 nightly 的 z42c**（下载）和**仓库当前 z42c** 分别编译当前
z42c 源码，确认上一个 nightly 仍能编当前源 → 没有「用了比已发布 nightly 更新的语法/格式」的越界。
（gh/tar 作外部子进程，逻辑在 `scripts/build/xtask_bootstrap_check.z42`；需 `gh` 已登录。）

- ✅ nightly z42c 编通当前源 = 无越界，分阶段纪律守住。
- ❌ nightly z42c 编不过、仓库 z42c 编得过 = **越界**：当前源用了新语法/新格式，但 nightly 还不支持 →
  按上面「support 先行、use 晚一 release」拆分，或回退过早的使用。

**何时跑**：改动 z42c（parser/lexer/codegen/zbc·zpkg writer）、加新语法、bump 格式后；CI 的
`bootstrap-no-csharp` job 是其全量版（下载 nightly → 重建全栈），本脚本是开发期快速本地版。

### 为什么这与「不为旧版本提供兼容」不冲突

[philosophy.md](philosophy.md) 的「不做兼容」是**不写兼容代码 / 不留旧路径**。本约定不写任何兼容代码——
它是**纪律**（晚一个 release 再用新语法），不是兼容层。z42c 永远只懂一个语法 / 格式版本；旧 nightly 能编当前
源码，纯粹因为当前源码**克制**着没用新东西，而非 z42c 兼容了旧的。**快速开发期照样不做兼容、实现保持最简。**

---

## 与其他规则的关系

- **[philosophy.md](philosophy.md) 不为旧版本提供兼容**：种子的「format 漂移」是该规则的例外——nightly 种子是
  *跨进程的二进制接口*，删兜底要尊重发布周期，不能假设旧种子永远可读。**分阶段引入纪律（见上）正是让这个
  「发布周期」可控、从而 C# 种子可彻底删除的前提。**
- **[workflow.md](workflow.md) 阶段 8 GREEN**：cold 路径本地不可验 → 该路径的「全绿」判定**以 CI 为准**，
  不是本地 warm 跑通就算数。
- **设计原理**（为什么自举需要种子、warm/cold 两态如何切换）落在 [`docs/design/compiler/self-hosting.md`](../../docs/design/compiler/self-hosting.md)，
  本文件只管「改动时如何避免踩坑」的流程约束。
