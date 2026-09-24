# Tasks: 包角色（role）与编译期扩展的解析域

> 设计 SoT：[design.md](design.md)｜动机：[proposal.md](proposal.md)。
> 每批单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| 批 | 内容 | bump | 状态 |
|----|------|:---:|------|
| 0 | 断 scripting 对 `z42.ir` 的假依赖：`FormatVersion` → z42i | 否 | ✅ 完成 |
| **1** | **`kind="analyzer"` + `compiler-libs/` 解析域** —— 让用户能写 generator | 否 | ✅ 完成 |
| 2 | `[analyzers]` 支持 `path` + 隔离校验 + handler ABI 握手 | 否 | 🟡 2.1–2.4 + 2.6 完成；**2.5 前提作废、待裁** |
| 2.5 | `role` 字段（support 先行 → 跨 nightly → publisher 读 role + 五包移出 `libs/`） | 否（**跨 nightly**）| ⬜ |
| 2.6 | scripting 拆两包 + `IReplCompiler` 门面搬家 | 否 | ⬜ |
| 3 | 大重命名：`std.*` 用户库 + `z42c.*` 编译器域 | 否（跨 nightly）| ⬜ |
| 4 | `z42c.abi` 契约包 | 待评估 | ⏸️ 推迟 |

---

## 批 0 —— 断 scripting 的假依赖 ✅

**为什么先做它**：独立、纯收益、不阻塞讨论，且让批 1 的拆包干净（少一条要重新安置的依赖）。

- [x] 0.1 查证 `z42.ir` 在 scripting 里的全部用量 → **只有** `Script.FormatVersion()` 一处，
      为拼 `"zbc M.m, zpkg M.m"` 一句话（`ZbcVersion` + `ZpkgWriterZ` 两个编译期常量）。
- [x] 0.2 查证 `Script.FormatVersion()` 的调用点 → **唯一**：
      [interactive_main.z42:76](../../../../src/toolchain/interactive/core/interactive_main.z42#L76) 的 `.version` 元指令。
- [x] 0.3 `Script.z42`：删 `FormatVersion()` + `using Z42.Project` + `using Z42.IR.BinaryFormat`。
- [x] 0.4 `z42.scripting.z42.toml`：删 `"z42.ir"` 依赖 + 头注记由来与终局。
- [x] 0.5 z42i：新增 `_formatVersion()`（含终局注：该由 VM 自报）+ 两条 using + 清单加 `z42.ir`。
- [x] 0.6 GREEN：`build stdlib` 25/25 绿（scripting 断依赖后照常编过 = 假依赖坐实）；
      `build toolchain` z42i apphost ready；实测 `.version` → `zbc 1.44, zpkg 0.49`（与 Rust 侧
      `ZPKG_VERSION_MINOR = 49` 一致，零回归）。

> **不做**：不在批 0 引入 VM builtin 暴露格式版本（见 design 批 0 注）。那是 vm 类型完整变更流程，
> 不搭本批的车。批 0 是职责归位，不是终局。

### 批 0 踩到的坑（记账）

- `./xtask test stdlib` 在默认 toolchain（stable/1.88）下 **exit 0 但一个测试都没跑**，
  输出只有 22 行 cargo 版本抱怨（cranelift/wasmtime 要 rustc 1.95）。
  必须 `RUSTUP_TOOLCHAIN=1.98.1`。**这是一个假绿门**——exit code 不反映「什么都没跑」。
  单独记，不在本批修。

---

## 批次重排（2026-09-24，实施中发现）

原计划批 1 = 「role 落地」打头。**实测后调整**，理由是两条硬事实：

1. **`role` 是新跨成员符号，卡 nightly。** z42c.driver 大量引用 `pm.Project.*`
   （[Main.z42:268](../../../../src/compiler/z42c.driver/src/Main.z42#L268) 等），给 `ProjectInfo`
   加 `Role` 字段并让 z42c 读它 = #788/#789 那个「support 先行、晚一个 nightly 再 use」的形状。
2. **但核心目标不需要 role。** `kind` 字段**已存在且 z42c 已在读**（`pm.Project.Kind == "exe"`），
   给它加一个**取值** `"analyzer"` 不是新符号；`[analyzers]` 段也早就存在。

⇒ 「让用户能写 generator」（proposal 的起因）可以**立即交付**，不等 nightly；`role` 降级为
批 2.5 的独立轨，它真正的价值是收编 publisher 那两条代理判据，与核心目标解耦。

## 批 1 —— `kind="analyzer"` + `compiler-libs/` 解析域

**目标**：proposal §Why 那个端到端复现从 `E0443 / 依赖未找到` 变成编过。

- [x] 1.1 `kind = "analyzer"` 第三种取值。`kind` 的既有判定全是 `== "exe"`，故 analyzer
      天然走 lib 路径产出 zpkg —— **零新符号、种子编得动**。
- [x] 1.2 `_compilerLibsDirs()`（[BuildPaths.z42](../../../../src/compiler/z42c.driver/src/BuildPaths.z42)）：
      三档探测 `Z42_HOME/compiler-libs/` → `Z42_PORTABLE_VM` 反推 SDK 根 → 开发树（自 `Z42_LIBS`
      上溯到 `artifacts/build/` 再拼 compiler member dist）。
- [x] 1.3 z42c.driver 接线：**仅** `kind == "analyzer"` 时把该域并入 libsDirs
      ⇒ 普通工程看不见编译器域包（解析域隔离），z42c 自建（`kind=exe`）不进此块 ⇒ 自举不动点。
- [x] 1.4 打包：`[component.compiler-libs]`（`compiler-libs-glob`，dest `compiler-libs/`）+
      `_pkgStageCompilerLibs` handler（路径走 `_compilerMemberDist` layout helper，不硬编码）+
      **只进 sdk、不进 runtime**。
- [x] 1.5 自检门加**成对**断言：契约包①**在** `compiler-libs/`、②**不在** `libs/`，
      外加 runtime 包回归守卫把 `compiler-libs` 与 z42c/z42vm 同列禁入。
- [x] 1.6 GREEN（已过）：`build compiler` ✓｜`test compiler` 自举不动点 **3/3 byte-identical** ✓｜
      `test packages` 3 packages / 11 components 全 PASS ✓
- [x] 1.7 **端到端对照验收**（真跑，非推断）：

      | 实验 | 结果 |
      |---|---|
      | `kind = "analyzer"` + `[dependencies] z42c.semantics` | ✅ 编过，产出 `probe.gen.zpkg` |
      | 同一份源码改 `kind = "lib"` | ❌ `z42c.semantics 未找到；已查找：<只有 libs/>` |

      对照项证明判别力：编过是解析域起的作用，不是别的什么恰好让它通过。

- [x] 1.8 `build sdk` 产出 `compiler-libs/`（**两条路都要改**：发行包 `_packageDesktop` 与
      本地 SDK `_buildSdk` 是两套组装，只改前者本地 `Z42_HOME` 仍没有该目录）+ SDK 态端到端复验：

      | 场景 | `kind` | `Z42_HOME` | 结果 |
      |---|---|---|---|
      | 开发树 | analyzer | — | ✅ 编过（③档探测） |
      | 开发树 | lib | — | ❌ 未找到（**对照**） |
      | SDK 态 | analyzer | ✓ | ✅ 编过（①档探测） |
      | SDK 态 | analyzer | ✗ | ❌ 未找到（**反证**：确证是 ① 在起作用） |

- [x] 1.9 **接一个真会红的门**：`_e2eAnalyzerDomainChecks`
      （[xtask_compiler_e2e_analyzer.z42](../../../../scripts/build/xtask_compiler_e2e_analyzer.z42)，
      挂在 `_testCompilerE2e`）。两格成对：analyzer 可引用契约 / 普通工程不可。
      **判别力已实证**：临时把解析域接线改成 `if (false)` → 门判红、`test compiler` rc=1；
      恢复 → 复绿。

### 批 1 踩到的坑（记账）

- 🔴 **e2e 的 `flat` 不是 stdlib 目录**。`_assembleAllLibs` 把 stdlib **与全部 compiler member**
  （含 z42c.semantics）硬链进同一个 `artifacts/.scratch/alllibs/<profile>` 给测试用 —— 在那个
  环境里编译器域包对**所有**工程可见，②格恒绿。第一版门就挂在这上面：它报「解析域隔离失效」，
  而实际失效的是**门自己的环境**。门须用 `_libsFlatDist`（真·stdlib 扁平 dist）。
  ⇒ 批 2.5 把编译器包移出 `libs/` 时，这个测试目录是第四处「按域混装」的地方，要一并想清楚。
- 两格若共用目录，增量缓存会把「解析得到吗」混进「缓存命中吗」——两格各用独立目录。
- 改了 `scripts/` 后 `./xtask` **仍跑旧逻辑**（它是已编译的 apphost）——自检门报的还是旧期望值，
  看着像「改了没生效」。须先用自建 z42c 重编 `scripts/xtask.z42.toml` → `artifacts/xtask/xtask.zpkg`，
  再用 `z42vm artifacts/xtask/xtask.zpkg -- …` 跑。同 [[worktree-green-false-signals-stale-xtask-and-zbc-regen]]。

## 批 2 —— `[analyzers]` 真依赖 + ABI 握手

> 批 1 已交付 `kind = "analyzer"` 取值 + 解析域。本批补齐 analyzer 工程的**其余语义**
> （宿主平台构建 / 不进 payload 闭包 / 双向校验）与 `[analyzers]` 的 path 支持。

- [x] 2.1 analyzer 工程的其余语义。**核查后大部分是既有事实，不是待做项**：
      · 「不跟随目标 rid」—— z42c **根本没有 rid 概念**（rid 是 builder/publish 的维度），
        `z42c build` 恒按本机构建，代建 handler 天然如此；
      · 「不进 `[dependencies]` 闭包与 payload」—— `[analyzers]` 与 `PathDepPlan.Resolve`（只走
        `[dependencies]` 边）本就是两条路；本批新增的代建**刻意不把产物并入 libsDirs**，见 2.2。
      真正需要落地的是把这条**变成会红的东西**：2.3 的反向校验 + 门的④格。
- [x] 2.2 `[analyzers]` 升级为与 `[dependencies]` 同构的 DepEntry 解析（支持 `path`）。
      实现 = `_resolveHandlerZpkgs` / `_handlerFromPath` / `_handlerFromLibs`（`BuildPaths.z42`）：
      定位 manifest → 校验 kind 与包名 → **z42c 代建** → 取其 dist 的 `<name>.zpkg`。
      🔴 **代建产物不并入消费方 libsDirs**（与 `[dependencies]` 的 path 闭包刻意不同）——并进去
      就等于让编译期扩展对运行期代码可见，批 1 的解析域隔离当场破掉。
      ⭐ **顺序要害**：解析必须排在 `_handlerFingerprint` 之前，否则指纹看到「zpkg 不存在」⇒
      改了 generator 源码消费方**编出旧结果**且无人报错。顺手把「指纹与 CompileInputs 各扫一遍
      libsDirs」并成一份解析结果。
- [x] 2.3 双向校验：analyzer 工程进 `[dependencies]` → 拒；非 analyzer 进 `[analyzers]` → 拒。
      **未占诊断码**：两条都发在 driver 的 CLI 层（`z42c build:` 前缀，同既有的依赖未找到 /
      pack 冲突），不是 binder 诊断，与 `[analyzers]` 既有的错误信息同族。
      ⚠️ 两条都**只在 path 条目上判得出来**：按名引用时手上只有 zpkg，而 **zpkg 不记 `kind`**
      （要记就是格式 bump，本批 bump=否）。批 2.5 把编译器域包移出 `libs/` 后，按名那条的
      泄漏面本身会收窄。
- [x] 2.4 **handler ABI 握手 fail-fast**（裁决 ⑤ 的对冲，自批 4 提前）。
      ⭐ **两条实测把这一项的描述本身改了**（design 原话：「失败模式是**崩**而不是报错」）：

      | 实验 | 结果 |
      |---|---|
      | 把 handler zpkg 头的 minor 从 49 改成 50（模拟另一代编译器编出）| **E0493、退出码 1，不崩** —— VM strict-pin 拒载 + PR3a 的 catch 早已接住 |
      | 把一个**零 handler 的普通库**按名挂进 `[analyzers]` | **退出码 0、零诊断** —— 扩展干脆不跑，编译照常成功 |

      ⇒ 「同代校验」这件事**格式维度早就做完了**；真正的洞有两个，本批各修一条：

      - **零发现 = 静默空转** → 新码 **E0496**（`PackageCompile`，`_runAnalyzers` 调用点之后）。
        只数**从 zpkg 发现的** handler：`[Forward]` 这类内建 generator 与测试注入的实例不计入，
        否则「声明了外部扩展却零发现」会被内建的存在掩盖。
      - 🔴 **E0493 的提示文字对版本代差这个成因是错的**：它一口咬定「indexed zpkg 必须连同散装
        `.zbc` 一起放；或改用 `--release`」，而用户照做的是完全无关的事 —— **与本批修掉的那条
        （path 被拒时说「未在依赖目录找到」）是同一个形状：指向错误方向的诊断**。
        改为失败时先读 zpkg 头版本分流成因，对不上就说「是 zpkg X.Y 格式、本编译器只认 A.B」。

      ⚠️ **仍盖不住、且本批盖不了**：**同格式、但 `z42c.semantics` 接口形状变了**的 skew ——
      zpkg 里没有任何「契约指纹」可比（DEPS 只记名字/版本，而 semantics 版本恒 `0.1.0`）。
      要真握手得往产物里写契约指纹 = **格式 bump**（本批 bump=否），或走批 4 的 `z42c.abi`。
      **别把 2.4 说成「同代校验做完了」。** 它今天的覆盖面是：格式代差（E0493）+ 零发现（E0496）。

      ⚠️ **符号归属**：`GeneratorLoader` 在 `z42c.pipeline`（与消费者 `PackageCompile` 同包，随便改），
      但 **`AnalyzerLoader` 在 `z42c.semantics`** ⇒ 给它加方法 = 新跨成员符号、卡一个 nightly。
      故 per-zpkg 归属做不成单 PR，本批用**聚合判定**（declared>0 且发现总数==0）+ 版本分流
      （纯 pipeline 内读字节，无新符号）。
- [ ] ~~2.5 退休 `KnownTestOnlyDeps` 白名单~~ —— **前提不成立，待 User 裁决改写**。
      该白名单与整个 `WS0xx` manifest-lint 家族住在 **C# 侧 `src/compiler/z42.Project/ManifestErrors.cs`**，
      随 2026-06-26 删 C# bootstrap 编译器一起蒸发：全仓**零 WS0xx 发射点**（唯一命中是
      `WorkspaceBuild.z42:136` 的一句注释）。
      ⚠️ **两本参考书就此冲突**：`error-codes.md` 诚实标注 WS001–WS039「零发射点」且根本没列
      WS012 / WS040–043；而 `z42-toml.md` 把 WS012/WS040–43 当**现行规则**写，还断言
      「校验在 xtask 发现层做」+「`KnownTestOnlyDeps` 当前为 `{ "z42.test" }`」——一份已腐成假话的
      第二真相（同 diagnostic-code-uniqueness 规则⑦的形状）。
- [x] 2.6 端到端验收（**真跑**，本仓库外的 `/tmp` 工程）：`kind="analyzer"` 的 generator 工程 +
      主工程 `[analyzers] = { path = "../gen" }` → z42c 代建 → generator 注入的 `E2eOut.O.V()`
      在主工程解析得到 → 产物跑起来打印 `42`；改 generator 源码 42→7 → 重建 → 打印 `7`。
      对照/反证四条全部实测：kind=lib 被拒 / analyzer 进 `[dependencies]` 被拒（退出码均 1）/
      `path = "."` 自指被拦（不无限递归）。
      注：analyzer **诊断**侧沿用既有 pipeline 单测（`test_external_analyzer_loaded_and_reports`），
      未经 path 条目——两者共用同一份 `AnalyzerZpkgs`，解析路径完全相同。

### 批 2 的门

`_e2ePathAnalyzerChecks`（[xtask_compiler_e2e_analyzer.z42](../../../../scripts/build/xtask_compiler_e2e_analyzer.z42)）
四格：① 代建+产码可用 ② 改扩展必重编 ③ kind=lib 被拒 ④ analyzer 进 `[dependencies]` 被拒。

**判别力两次实证**（都是真注入、真重编编译器、真看门的颜色）：

| 注入 | 门的反应 |
|---|---|
| `_handlerFingerprint(pm, null)`（指纹看不见 path 条目）| ②格红：「改了 generator 源码，消费方却编出旧结果」stdout=42 |
| kind 校验改 `if (false)` | ③格红（**收紧断言后**才红，见下）|

⭐ **第一版③格的断言是假的**：写作 `stderr.IndexOf("analyzer") >= 0` 即通过。删掉 kind 校验后，
kind=lib 的工程会在**代建阶段**因解析不到契约包而失败，那条失败经本门转述成
「`[analyzers]` … 的工程构建失败」——**含 "analyzer" 字样** ⇒ 宽断言把「校验没了」判成绿。
改成断言那一句原话（`不是 "analyzer"`）后才真红。**「门会红」不等于「门守着对的东西」。**

## 批 2.5 —— role 字段（独立轨，跨 nightly）

- [ ] 2.5.1 **support**：`ProjectInfo.Role` + `ManifestLoader` 解析 `[project].role`（两值，
      省略 = `runtime`）。**无消费者** → byte-identical、可立即合并。
- [ ] 2.5.2 **use**（晚一个 nightly）：publisher 判据换成读 role，退休
      `builder_publish.z42:546-551` 那两条打补丁的注释（`z42.scripting`「越界」/`z42.workload.*`「漏拷」）。
      注：publisher 现在走 `_pubTomlStr(toml, …)` 直读 toml，**不经 ProjectInfo** —— 若维持直读，
      这一步可不等 nightly，实施时再定。
- [ ] 2.5.3 五个编译器域包标 `role=compile-time` 并移出 `libs/`。
      ⚠️ 三处按「stdlib workspace 成员」推导落点会同时动到：`_ensureBootstrapSelfDepLibs` 冷启动预建、
      `xtask_stdlib.z42` 的 `_stdlibList`、扁平视图 hard-link 汇聚。

## 批 2.6 —— scripting 拆两包

- [ ] 2.6.1 拆 `z42.scripting`（eval 内核，零编译器域依赖）/ `z42.scripting.editing`
      （Classifier / Completeness / Completer / Rewriter，依赖 `z42c.syntax`）。
- [ ] 2.6.2 `IReplCompiler` 门面从 `z42.build` 挪到 scripting 自己。
- [ ] 2.6.3 发行形态（`z42-runtime-scripting` vs runtime 不带 scripting）：**User 2026-09-24 裁决
      推到拆包之后再定** —— 先拆包，看实际体积再决策。

## 批 3 —— 大重命名

- [ ] 3.1 `std.*` 前缀：用户库改名（User 既定方向）。
- [ ] 3.2 `z42.ir` / `z42.project` / `z42.build` → `z42c.*`（**并进同一批**：重命名成本主要在种子纪律
      与引用点扫描，合并做边际成本远低于两次）。
- [ ] 3.3 support 先行、晚一个 nightly 再 use（[[bootstrap-seed]] 纪律）。
- [ ] 3.4 `src/libraries/README.md` 那段「两类库（别混淆）」脚注**删除**——它存在的理由被 role + 命名
      同时消灭。**这条是本批的验收信号**：补丁性散文能删掉，才说明机制真的替代了约定。

## 批 4 —— `z42c.abi` 契约包 ⏸️

推迟（裁决 ⑤）。形态稳定后再评估。已知代价：`GenTarget` 暴露 `Z42ClassType` / `SymbolTable`，
抽干净要么把这些一起下沉（拖出一大片），要么契约签名仍引用 semantics（等于没解耦）。
