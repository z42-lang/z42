# Tasks: sidecar-reaches-published-apps

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-settings/spec.md)

## P0 — `from_env()` 补文件层（runtime）
- [x] 0.1 `RuntimeConfig::from_env()`：加载 `Z42_CONFIG` + `Z42_APP_CONFIG` 两层；错误 → `eprintln!` warn + 该层视为不存在（**绝不 exit**，它是库入口）
- [x] 0.2 `from_getter` 保持 `resolve(_, None)` 不变（测试/可注入路径不受影响）
- [x] 0.3 单测：两层各自生效 / 叠加用户赢 / 坏 TOML 不 panic 且降级 / `.json` 同样只 warn
- [x] 0.4 非破坏：`z42vm --show-config` 与 `--info` 输出不变（main() 不走 from_env）
- [x] 0.5 GREEN

## P1 — spawn apphost 注入 `Z42_APP_CONFIG`（toolchain / rust）
- [x] 1.1 `exec_app`：`app_zpkg.with_extension("runtimeconfig.toml")` 存在 → `cmd.env("Z42_APP_CONFIG", ..)`
- [x] 1.2 仅当调用方未设时注入（显式优先，与 launcher 同款）
- [x] 1.3 单测：存在/不存在/已设三种；`app.zpkg` → `app.runtimeconfig.toml` 派生正确
- [x] 1.4 GREEN

## P2 — `z42 publish` 拷侧车（toolchain / z42）
- [x] 2.1 新文件 `builder_publish_sidecar.z42`：`_pubStageApp(zpkgSrc, dstZpkg)` = 拷 zpkg + 拷同 stem 侧车（不存在则跳过）
- [x] 2.2 三处 `File.Copy(zpkgSrc, ...)` 改调 `_pubStageApp`；**`builder_publish.z42` 净行数不增**（967 在棘轮基线上）
- [x] 2.3 自包含布局：侧车随 zpkg 改名为 `app.runtimeconfig.toml`
- [x] 2.4 e2e：publish 一个带 `[profile.release] gc-trace = true` 的工程 → 跑产物 → stderr 出现 GC trace
- [x] 2.5 GREEN

## 文档
- [x] `docs/book/src/runtime/runtime-settings.md`：删掉"已知缺口：apphost 不读侧车 / publish 不拷"，改写为实际行为 + 嵌入方也走同一条链
- [x] `docs/design/runtime/launcher.md`：apphost 现在会发现并传递侧车

## 未决
无。


## 落地记录（2026-09-05）

**探查修正了提案的一处低估**：原以为只是"侧车没被拷过去"，实际最深的断点是
`RuntimeConfig::from_env()` 压根不读配置文件（`resolve(get, None)`）——文件层只在
`z42vm` 的 `main()` 里装配，于是**每个嵌入方**（desktop 自包含 apphost / wasm / iOS /
Android / testhost）都静默忽略 `Z42_CONFIG` **和** `Z42_APP_CONFIG`。不是"侧车没到"，
是"链在 z42vm 二进制之外根本没接上"。

实施中遇到的两个非预期：
1. `builder_publish_sidecar.z42` 的 namespace 我写成 `Z42.Build.Builder`，实际该包用
   `Z42Builder` —— 编译期不报（同包多文件各自 namespace 合法），运行期才炸
   `undefined function`。
2. 新源文件要显式加进 `z42.builder.z42.toml` 的 `[sources].include`（该包用显式列表而非
   glob），且**增量缓存不把 manifest 的 sources 变更算进 key** —— 清掉
   `src/toolchain/builder/core/artifacts/` 才重建。后者值得单独记一笔（本 change 未修）。

**手工端到端**（本地唯一能跑通的证明，见下）：demo 工程 `[profile.release] gc-trace = true`
→ `z42c build` 产侧车 → `z42b publish` 侧车进 `publish/payload/` → 跑 `publish/bin/demo`
得 `gc-trace=true source=app-config`；直跑同一个 payload zpkg（无 `Z42_APP_CONFIG`）
得 `source=default`，两相对照证明是 apphost 注入生效。

**GREEN**：runtime cargo 1141 / 0；apphost cargo 21 / 0；e2e 566 + cross-zpkg 17 +
multi-exe 2；launcher dist smoke 3 / 3；lines 33 known / 0 new-grown。

⚠️ **本地 `xtask test dist` 全量跑不了**：570 e2e "compile failed" + apphost smoke
`SKIP (bin/apphost not in package)`。**在不带本 change 的同一棵树上完全一致**，是本地
打包产物陈旧的既有问题。因此新加的
`MODE_SRC=app-config` 断言**本地未被执行过**，以 CI 为权威。

## 剩余（既有 deferred）
- 8 个 `ENV_ONLY` 旋钮收编进 `RuntimeConfig`
- `runtimeconfig.template.toml` 手写模板合并
- iOS/Android/wasm 各自「侧车怎么随包分发」（经 P0 已可用环境变量喂，打包侧未做）
- z42b 增量缓存不感知 manifest `[sources]` 变更（本次踩到，未修）
