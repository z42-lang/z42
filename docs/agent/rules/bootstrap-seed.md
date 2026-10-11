# 自举种子依赖：编译器 / xtask 的鸡蛋问题

> 触发条件：**改动任何「构建工具自己也要被构建」的链路**——尤其是 z42c（自举编译器）、
> xtask（z42 写的 dev CLI）、stdlib（被前两者依赖）三者的 build / seed / bootstrap 路径。
> 这条规则补齐 [workflow.md](workflow.md) 缺失的**自举维度**：删一个种子兜底前，必须先确认每个入口仍有种子。
>
> 四轴（① 语法 / ② zbc·zpkg 格式 / ③ stdlib API / ④ z42c 自依赖库）的机制、断环做法与实测见
> [编译器自举](../../internals/src/compiler/self-hosting.md)——本文件只管「改动时如何避免踩坑」。

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

> **cold 种子的统一解析**：`build compiler` / `build stdlib` 冷启动由 `_ensureSeed`
> （`scripts/common/xtask_common.z42`，SDK 定位在 `_seedSdkDir`）按 **`Z42_HOME`（`--toolchain` 设它，
> 或 launcher / install 设）→ 运行 xtask 的 apphost SDK（`Z42_PORTABLE_VM` 反推）→ `./.z42`** 找到 SDK，
> 把 `programs/z42c` + `libs` 拷进 in-tree 再自建。**CI 与本地同一条 resolver、同一个位置**：CI
> （`.github/actions/setup-z42-sdk`）把 nightly 装进仓库根 `./.z42`，与本地 `install-z42.sh` 相同。
> warm 树（in-tree 已有种子）**不被覆盖**——gen2 字节不动点靠「第二次从 in-tree gen1 再种」收敛，
> 故 in-tree 必须最高优先。managed 布局的 `Z42_HOME`（`runtimes/`，无 `programs/`）不符
> SDK-toolchain 布局 → 跳过（不误当种子源）；`Z42_LIBS` 显式覆盖仅在其确实含 `z42.core.zpkg` 时生效。

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
4. **格式漂移**：下载的 nightly 种子其 zbc / zpkg 格式与当前 z42vm 的 strict-pin 不同时，`ci-bootstrap`
   的**版本差 gate** 检测到不等就走**两代自举**（用 SDK 自带的旧 VM 跑 gen1/gen2，把种子推进到当前格式，
   再交新 VM），所以格式 bump 的 build-and-test / toolchain-bootstrap / package 路径 **CI 自动过、
   免手动传种子**。机制见 [CI 拓扑](../../internals/src/devinfra/ci.md)。**删 cold 兜底不要踩在 format
   bump 同一周期**：纯 download-bootstrap 的 job（vm-jit / bench）在 bump 当次仍会短暂红一跑，
   等新 nightly 发布自愈，不阻塞发布链。
5. **`ci-bootstrap` §1.5 每代构建前清空其就地 artifacts**：因这条路径**本地不可验**（冷启动 + 格式 bump
   才走到），暂时保留。**ToDo**：下一次格式 bump 的 PR 里顺手删掉它并观察 CI——那时它正好被真实行使一次，
   红了也立刻知道是谁的锅。

---

## 分阶段引入新语法 / zbc·zpkg 格式（自举跨版本）

> 这条是「鸡蛋问题」在**语言 / 格式演进**维度的解：**上一个已发布 nightly 的 z42c 永远能编当前 main 源码**，
> build-and-test 即「下载上一版 nightly → 自举当前源码」，无死锁。

### 核心约定：support 与 use 必须分两个 release（必须遵守）

自举编译器加新语法 / bump zbc·zpkg 格式时，若当前源码**立即使用**新语法 / 新格式，则只有懂新语法·格式的
编译器才能编它——而那个编译器还没发布（要靠这次构建产出）。死锁。破解办法是把落地拆成跨两个 nightly 的两阶段：

1. **阶段 1 —— 落「支持」**：给 z42c 加新语法的 lexer/parser/codegen（或新格式的 writer/reader），但
   z42c 自身源码 + stdlib + xtask **仍只用旧语法 / 仍产出旧格式**。
   → 上一个 nightly 的 z42c 能编这份源码 → 产出「**支持**新语法·格式」的新 z42c → 发布新 nightly。
2. **阶段 2 —— 落「使用」**：新 nightly 发布后，**才**在 z42c / stdlib / xtask / 用例里**使用**新语法、
   或让构建**产出**新格式。→ 刚发布的 z42c（阶段 1 能力）能编。

**铁律**：当前 main 的源码，**任何时刻都不得使用比「上一个已发布 nightly 的 z42c」更新的语法 / 格式**。
违反 = 跨版本自举断链。

**「新 nightly 发布了」怎么确认**：nightly 必须**包含**阶段 1 的提交（不是「发布时间晚于合并时间」）。
确认命令，以及「哪些路径不影响 SDK、会被后来的 main 运行替换」的判定表与 `non_sdk_re` SoT，见
[CI 拓扑](../../internals/src/devinfra/ci.md)「main 上的运行不互相打断」——**决定 SDK 内容的逻辑不要放进
那些不进 SDK 的路径**。

### 清单键同样 support-then-use（`z42.toml` / `z42.workspace.toml`）

清单里不认识的键是**错误**（`Z42.Project.ManifestKeys` 是合法键的唯一真相源），而仓内清单由**种子那一代**的
z42c / xtask 读——不认识的键在旧种子里被静默忽略、在新种子里被拒掉，两种结局都是错的。所以**加键与改键名
都必须跨两个 nightly 分两阶段**；两种情形的具体步骤见
[工程模型 · 清单键审计](../../internals/src/compiler/project-model.md)。

### 轴③：stdlib API 面

CI 冷启动用**种子 z42c + 种子 stdlib** 编当前 xtask 源与 z42c 源，因此这两个源码域**引用的 stdlib API
也被上一 nightly 钉死**（为什么 xtask 最受约束，见[编译器自举](../../internals/src/compiler/self-hosting.md)）：

- **xtask / z42c 源新用一个 stdlib API**：该 API 必须已随某个 nightly 发布（加 API 本身随时可做，
  用它要晚一个 nightly——与语法同款纪律）。
- **删 / 改 xtask / z42c 在用的 API**：两阶段跨两个 nightly——阶段 1 加新 API、**旧 API 暂留**
  （种子例外，非兼容层）、调用点不动 → nightly 发布 → 阶段 2 切全部调用点 + **同一提交删旧 API**。
- **xtask 用到的编译器域 API（`Z42.Project` / `Z42.Build`）同属此轴**：加 API 随时可做，用它要晚一个
  nightly；删 / 改走上面的两阶段。（另一个后果：改布局规则的那次合入里，CI 上的 xtask 仍按上一 nightly
  的规则算路径——xtask 侧的跟进放到下一个 PR。）
- stdlib 源自身不受此轴约束（它由自建的当前 z42c 编译）。

> 🔴 **阶段 2 最容易被忘掉——因为忘了不会红。** 过渡形态（字面量 / 旧 API 并存）能一直跑下去，于是
> **沉淀成常态**。**新开一个分阶段引入时，先想好阶段 2 由什么来提醒你**——没有提醒就等于没打算做。
> 本仓的做法是挂 `STAGE2-DEBT(<tag>)` 标记，由 `xtask check stage2` 做双向棘轮 + 挂账到期即红
> （边界见[GREEN gate](../../internals/src/devinfra/test-gate.md)）。
> 🔴 **收尾不止于代码**：过渡形态也写在散文里，而散文不会跟着切回。
> **收尾时问一句：我刚消灭的那个过渡形态，还被写在哪里当成现状？**

可操作的完整提交剧本（判定 grep / 两个 commit / 等 nightly 的检查命令）见
[testing.md](../../internals/src/devinfra/testing.md)「stdlib 破坏性 API 变更」。

### 轴④：z42c 运行期自依赖的 stdlib 库

**判据**：本次改动是否让 z42c 的**源**新 `using` 一个「z42c 运行期就要加载」的 stdlib 库，而该库
**上一 nightly 种子里不以同名 zpkg 存在**？

断环机制（`_ensureBootstrapSelfDepLibs` 的预建顺序、不 warm-skip、flat 装配细节）见
[编译器自举 · 轴 ④ 的破环预建](../../internals/src/compiler/self-hosting.md)。由此有两条与纪律不同的结论：

- 对这 6 个库，「**加**一个新 API 并同 commit 用它」**无需等一个 nightly**——预建用当前源重建它们，
  新 API 加上了、旧的还在，上一版 driver 运行期照旧解析得到
- **但改名 / 删除不受此豁免**：它会抹掉旧 FQN，而跑这轮 bootstrap 的正是那个旧 driver ⇒ 中途就死。
  要让两代并存需要兼容物，而**兼容物形态要按「编译期 / 运行期是两个独立解析面」来选**——见
  [编译器自举 · 改名](../../internals/src/compiler/self-hosting.md)
- **种子 ABI 残余约束**：给这 6 个库的既有导出类型**加字段**时，新字段不得进 ctor 签名（ctor 内给默认值 +
  消费方构造后赋值），否则旧种子的构造调用元数对不上

> ⚠️ 这类漏网**本地 warm 验不出来**：`xtask test compiler bootstrap` 只「编」不「跑」新建出来的 z42c，
> 运行期自依赖问题看不见，只能靠 CI 冷启动全栈重建暴露。

- **教训**：**新增 / 收敛「z42c 自依赖的 stdlib 库」的 change，冷启动路径本地必验**（下载上一 nightly
  作种子跑一遍 cold `build compiler` + `build stdlib`），别只验 warm 就推 main。

### 边界检查（每次改完编译器 / 语言 / 格式相关代码必跑）

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
  *跨进程的二进制接口*，删兜底要尊重发布周期，不能假设旧种子永远可读。**分阶段引入纪律正是让这个
  「发布周期」可控的前提。**
- **[workflow.md](workflow.md) 阶段 5 GREEN**：cold 路径本地不可验 → 该路径的「全绿」判定**以 CI 为准**，
  不是本地 warm 跑通就算数。
- **设计原理**（为什么自举需要种子、warm/cold 两态如何切换、四轴与断环）落在
  [编译器自举](../../internals/src/compiler/self-hosting.md)，本文件只管「改动时如何避免踩坑」的流程约束。
