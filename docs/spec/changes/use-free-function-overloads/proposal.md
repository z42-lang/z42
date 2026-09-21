# use-free-function-overloads — 阶段 2（use）

> free-function-overloads 阶段 1（support 先行，PR #731）已合入 main 并随 nightly 发布
> （nightly `main @ b82282d` 的历史含 #731）。按 bootstrap-seed 的 support/use 分阶段纪律，
> 「晚一个 nightly」条件已满足——上一版已发布 nightly 的 z42c 已具备编译自由函数重载的能力，
> 冷启动自建可编译使用重载的种子消费代码。本变更兑现阶段 2：**在工具链源码里真正使用自由函数重载**，
> 把此前「因语言不支持自由函数重载而被迫改名」的分名函数族合并回同名重载。

## 背景

阶段 1 只给 z42c 加了「解析 + 发射 + 决议」自由函数重载的能力，**stdlib / z42c 自身源 / 工具链源刻意不写任何自由函数重载**——primary-bare 键规则（复用 #414）保证对存量代码零发射字节漂移，从而零格式 bump、零两代自举。阶段 2 是这项能力的首次真实使用，验证端到端可用（含 OverloadResolver 对 arity 重载与类型重载的正确决议）。

## 全库扫描结论

三路穷尽扫描 `src/libraries/`（stdlib）、`src/compiler/`（z42c）、`src/toolchain/`，判据：
必须是**自由函数**（namespace 顶层、非类/struct 成员——类方法本就能重载），且分名**纯粹是语言限制所致**、合并成同名重载能提升可读性（排除「不同名字表达不同语义意图」的正当分名）。

- **stdlib：0 候选**。标准库用「静态类 + 方法」组织（631 class vs 仅 10 个自由函数，全在 `z42.scripting`
  且语义各异）。方法一直可重载、从不受 E0408 限制，故无「被迫改名」遗留。
- **z42c：0 候选**。`z42c.semantics` / `z42c.pipeline` 两包无自由函数（100% 类代码）；自由函数集中在
  `z42c.driver`，但都是正当分名（如 `_buildWorkspaceFlat` / `_buildWorkspacePerMember` 是两种不同
  产物布局策略，名字承载语义意图，非参数变体）。
- **工具链：6 组候选**（全在 `Z42Builder` 与 launcher）。见下。

## 合并清单（6 组）

primary（裸键）均为声明序第一个；全是 `_` 私有 helper、不跨包导出，primary 选择不影响外部。

| 组 | 合并后 | 成员（旧名） | 文件 | 重载形态 |
|---|---|---|---|---|
| 1 | `_orchestrate` ×3 | `_orchestrate`(4) / `_orchestrateFor`(5) / `_orchestrateWith`(6) | builder.z42 | arity（默认透传链） |
| 2 | `_initialInputs` ×2 | `_initialInputs`(1) / `_initialInputsWith`(2) | builder.z42 | arity |
| 3 | `_forwardZ42b` ×2 | `_forwardZ42b`(1) / `_forwardZ42bEnv`(3) | launcher_cli.z42 | arity |
| 4 | `_resolveDevTargets` ×2 | `_resolveDevTargets`(4) / `_resolveDevTargetsF`(5) | builder_dev_targets.z42 | arity |
| 5 | `_dtSort` ×2 | `_dtSortByName`(DevTarget[]) / `_dtSortStrings`(string[]) | builder_dev_targets.z42 | **类型**（首参数组元素类型） |
| 6 | `_pubPlatform` ×2 | `_pubPlatformStr`(末参 string) / `_pubPlatformBool`(末参 bool) | builder_publish.z42 | **类型**（末参类型） |

组 1–4 是纯 For/With/Env/F 后缀的默认透传链，合并消除人为命名后缀。组 5–6 是按参数类型分名，合并后靠实参类型自动派发——正是 OverloadResolver 类型决议的验证点（arity 相同、靠参数类型区分）。

## 明确排除（正当分名，不合并）

- `_workloadInstallLocal` / `_workloadInstallNetwork`（launcher）：两条实质不同的安装路径（本地目录 vs 网络下载校验），名字承载关键意图。
- `_buildWorkspaceFlat` / `_buildWorkspacePerMember`（z42c.driver）：两种不同产物布局策略。
- `_typeStaticMembers` / `_memberComplete`（z42.scripting）：语义不同（静态 vs 实例成员），且签名相同本就无法重载。

## 验收门

1. **`build toolchain` 编译通过**——用含 #731 support 的自建 z42c 编译改后的工具链源（含重载 use），
   这是 support 生效的直接证据。
2. **工具链行为不变**——z42b build/test/publish、launcher 转发功能正常（相关 e2e / target fixtures 绿）。
3. **GREEN**：`xtask test`（interp）相关阶段全绿；改了派发面须补 `test stdlib --mode jit`（本变更改的是
   工具链源、非 stdlib 派发，但重载决议在编译期，仍以全绿为准）。
4. **冷启动自建（`compile-toolchain`）过**——本地 `test bootstrap` 不编工具链源（只编编译器成员），
   工具链源的冷启动越界由 CI 冷启动兜底。

## 非目标

- 方法组 / `BoundFuncRef` 对同名多重载自由函数取引用的 target-type 消解（阶段 1 D5，仍诊断，留后续）。
- stdlib / z42c 源码使用重载（本次扫描确认二者无候选，无可回填遗留）。
