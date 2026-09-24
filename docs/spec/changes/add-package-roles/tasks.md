# Tasks: 包角色（role）与编译期扩展的解析域

> 设计 SoT：[design.md](design.md)｜动机：[proposal.md](proposal.md)。
> 每批单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| 批 | 内容 | bump | 状态 |
|----|------|:---:|------|
| 0 | 断 scripting 对 `z42.ir` 的假依赖：`FormatVersion` → z42i | 否 | ✅ 完成 |
| **1** | **`kind="analyzer"` + `compiler-libs/` 解析域** —— 让用户能写 generator | 否 | ✅ 完成 |
| 2 | `[analyzers]` 支持 `path` + 隔离校验 + handler ABI 握手 | 否 | ⬜ |
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

- [ ] 2.1 analyzer 工程的其余语义：恒按宿主平台构建（不跟随目标 rid）/ 不进 `[dependencies]`
      闭包与 publish payload。
- [ ] 2.2 `[analyzers]` 从「按名在 LibsDirs 找 `<name>.zpkg`」升级为与 `[dependencies]` 同构的
      DepEntry 解析（支持 `path`，优先 path → libs 兜底）。**这条是「用户自定义」从纸面变可用的关键。**
- [ ] 2.3 双向校验诊断：`kind="analyzer"` 的包出现在 `[dependencies]` → error；
      非 analyzer 包出现在 `[analyzers]` → error。诊断码按 diagnostic-code-uniqueness 规则分配
      （**逐个 `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`**，扫 main 不够）。
- [ ] 2.4 **handler ABI 握手 fail-fast**（裁决 ⑤ 的对冲，自批 4 提前）：`GeneratorLoader` /
      `AnalyzerLoader` 加载前校验 handler zpkg 与当前编译器同代，不同代 → 明确诊断而非崩。
- [ ] 2.5 退休 `KnownTestOnlyDeps = { "z42.test" }` 硬编码白名单——让包自己声明 kind/role。
- [ ] 2.6 端到端验收：在本仓库外建一个 `kind="analyzer"` 工程，主工程 `[analyzers]` 用 `path` 引用，
      跑出诊断 + 生成代码。**必须真跑，不接受"应该能行"。**

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
