# Proposal: 让配置文件层真正到达已发布的 app（sidecar-reaches-published-apps）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`vm`（+ `toolchain`）→ 完整流程
> 前置：[`complete-runtime-settings`](../complete-runtime-settings/)（PR #443，已合并 main `11942db9`）
> SoT：`docs/book/src/runtime/runtime-settings.md`（本 change 修正其中一条"目前只对 z42 run 生效"的已知缺口）

---

## Why

`complete-runtime-settings` 立了五层链并让 `z42c build` 把工程 `[profile.*]` 烤进
`dist/<name>.runtimeconfig.toml`。但那条链**只在 `z42 run` 下成立**。探查确认三个断点：

### 断点 1（最深）：嵌入方根本不读配置文件

`RuntimeConfig::from_env()` 是 `resolve(get, **None**)`——**没有文件层**
（[config.rs:200](../../../../src/runtime/src/config.rs)）。文件层是 `z42vm` 的
`main()` 手动 `load_runtime_toml` / `load_app_config` 后传进去的。

于是**每一个走 `z42_host_run_app` 的入口**都静默忽略 `Z42_CONFIG` 与
`Z42_APP_CONFIG`：desktop 自包含 apphost（`apphost_embed.c`）、wasm、iOS、Android、
testhost。用户 `export Z42_CONFIG=my.toml` 后在这些形态上**完全无效、且无任何提示**。

这不是"侧车没拷过去"，是**分层链在非-z42vm 入口上根本没接上**。

### 断点 2：spawn apphost 不告诉 z42vm 侧车在哪

`hostrun.rs::exec_app` 跑 `z42vm <app.zpkg>` 时只设 `Z42_LIBS`
（[hostrun.rs:186](../../../../src/toolchain/workload/desktop/platform/apphost/src/hostrun.rs)）。
侧车就躺在 zpkg 旁边，但没人指给 VM。launcher 会做这件事，apphost 不会——而
`z42 publish` 出来的 app **绕开 launcher 直跑**（`simplify-apphost-direct-run` 的既定设计）。

### 断点 3：`z42 publish` 不拷侧车

`builder_publish.z42` 三处拷 zpkg 的地方都没带上同名 `.runtimeconfig.toml`：
payload 布局（`File.Copy(zpkgSrc, appZpkg)`）、自包含布局（`File.Copy(zpkgSrc, appDir/"app.zpkg")`
——注意它**改名**了，侧车得跟着改成 `app.runtimeconfig.toml`）。

**合起来的用户可见后果**：在 manifest 写 `[profile.release] gc-max-bytes = "512MB"`，
`z42 publish` 出来的二进制跑起来该设置完全不生效，没有任何提示。刚落地的机制在
**部署路径上是死的**。

---

## What（三件事，一一对应三个断点）

### A. `from_env()` 补上文件层 —— 嵌入方与 z42vm 走同一条链

`RuntimeConfig::from_env()` 改为也按 `Z42_CONFIG` / `Z42_APP_CONFIG` 加载两个文件层。

**严重度**：这条路径不能 `exit(2)`（它是库入口，可能在宿主进程里）。文件问题一律
**warn 后继续**——与既有"env/文件层 warn"的分层策略一致；`z42vm` 的 `main()` 保留它
更严格的路径（CLI 致命 + `--strict-config`）。

### B. spawn apphost 把侧车指给 z42vm

`exec_app` 在 zpkg 旁找 `<stem>.runtimeconfig.toml`，存在则设 `Z42_APP_CONFIG`——
**仅当调用方没设**（用户显式指定优先，与 launcher 同款语义）。

### C. `z42 publish` 拷侧车

三处 zpkg 拷贝各带上同名侧车；自包含布局按新 stem 改名为 `app.runtimeconfig.toml`。
**侧车不存在就什么都不做**（多数工程没有 `[profile.*]` 旋钮 → 没有侧车）。

---

## What This Does NOT Do

- **不改优先级链本身**：五层不变，只是让 L3/L4 在更多入口上真正被读到。
- **不给嵌入方 CLI 层**：`--set` 是 `z42vm` 二进制的表面；宿主要覆盖旋钮请设环境变量
  或用 `init_runtime_config` 自己装。
- **不动 iOS/Android/wasm 的打包脚本**：它们经 A 自动受益（只要宿主设了环境变量）；
  各平台"侧车放哪、怎么随包分发"是各自的打包问题，本 change 不碰。
- **不做 `runtimeconfig.template.toml` 合并**（仍是 `complete-runtime-settings` 的 deferred）。
- **不收编 8 个 `ENV_ONLY` 旋钮**（独立项，见 change `complete-runtime-settings` 的剩余队列）。

---

## 三阶段

| 阶段 | 内容 | 子系统 | 风险 |
|---|---|---|---|
| **P0** | `from_env()` 加载两个文件层 + warn 语义 + 单测 | runtime | 低（纯补齐，z42vm 路径不变）|
| **P1** | apphost `exec_app` 发现并注入 `Z42_APP_CONFIG` + 单测 | toolchain(rust) | 低（自包含，有现成单测框架）|
| **P2** | `z42 publish` 三处拷贝带上侧车 + e2e | toolchain(z42) | 低 |

## Scope

| 文件 | 变更 |
|---|---|
| `src/runtime/src/config.rs` | MODIFY（`from_env` 补文件层）|
| `src/runtime/src/config_tests.rs` | MODIFY |
| `src/toolchain/workload/desktop/platform/apphost/src/hostrun.rs` | MODIFY（`exec_app` + 单测）|
| `src/toolchain/builder/core/builder_publish.z42` | MODIFY（三处拷贝）|
| `docs/book/src/runtime/runtime-settings.md` | MODIFY（删掉"已知缺口"段，改写为实际行为）|
| `docs/design/runtime/launcher.md` | MODIFY（apphost 现在读侧车了）|

## 未决
无。spec：[design.md](design.md) / [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md) / [tasks.md](tasks.md)。
