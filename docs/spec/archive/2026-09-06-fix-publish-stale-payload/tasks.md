# Tasks: fix-publish-stale-payload

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06 | 类型：fix（最小化模式）

**变更说明：** `z42 publish` 不再把「zpkg 文件存在」当作「zpkg 是最新的」——每次都过一遍 z42c 的
增量编译（源码变了就重出产物），并把依赖/native/payload 的拷贝从「dst 在就不拷」改为 overwrite；
新增 `--no-build` 显式承载旧的「就用现成字节」契约（xtask 的 SDK 组装 / 自举不动点路径）。

**原因：** `_pubEnsureBuilt` 的 `if (File.Exists(zpkgSrc)) { return zpkgSrc; }` 让改完源码再
`z42 publish <toml>` 静默把**上一次的旧 payload** 重新签进 apphost，退出码 0、无任何提示。
这是**所有 publish 路径共有**的洞（单 app / 多 exe / --self-contained / 依赖闭包），不是
xtask 特有——xtask 只是最先暴露，因为 `z42 publish scripts/xtask.z42.toml` 是它文档化的唯一构建方式。
`z42 build`（z42c）**本身没有这个问题**，其文件级内容哈希增量是正确的；故根因修复 = publish 停止
自己判断新旧、把决定权整个交回 z42c。

**文档影响：** `docs/design/toolchain/launcher-command-dispatch.md`（publish 自带编译语义）、
`scripts/README.md`（xtask 构建方式说明）。

## 任务
- [x] 1.1 `builder_publish_build.z42`（新文件）：`_pubEnsureBuilt` 改为无条件走 z42c 增量编译 +
      `noBuild` 分支；连同 `_pubFindZ42c` 一起从 `builder_publish.z42` 搬出（该文件在 line-limit
      棘轮基线 967 上，越界文件不得增长）
- [x] 1.2 `builder_publish.z42`：`noBuild` 贯穿单 app / 多 exe / --self-contained / `_pubBundleProjectDeps`；
      4 处 `if (!File.Exists(dst)) File.Copy(...)` 改 overwrite
- [x] 1.3 `builder_cli.z42` + `z42.builder.z42.toml`：`z42b publish --no-build` 旗标 + 新源文件登记
- [x] 1.4 `launcher_cli.z42` + `launcher_export.z42`：`z42 publish --no-build` 旗标 + 转发
- [x] 1.5 `xtask_package_desktop.z42` `_z42bPublish`：显式传 `--no-build`（每个调用点都已用 in-tree
      自建 z42c `_z42cBuildToml` 编好；若让 publish 拿 SDK 种子 z42c 重编，gen2 字节不动点会破）
- [x] 1.6 文档同步（launcher-command-dispatch.md / scripts/README.md）
- [x] 1.7 GREEN：`xtask test` 全绿（10 stage / 2m52s，见下）+ `xtask test packages` PASS

## GREEN
分支 `fix-publish-stale-payload`（worktree `wt-publish-stale`，基于 `origin/main` 40d274c2）：
`cargo build --manifest-path src/runtime/Cargo.toml --release`（阶段 8 步骤 1）+ `xtask test`
→ **✅ GREEN — all stages passed**（3m18s；含 `✅ z42c self-host 不动点: 3/3 packages gen1==gen2`）。
另：`xtask build toolchain` ✔（5 个 apphost 全走 `--no-build`）、`xtask build sdk` ✔、
`xtask test packages` PASS。

> **两处与本变更无关的红，均已定位并排除：**
> 1. `unknown builtin __array_copy` → `z42c --workspace self-build failed`：in-tree
>    `artifacts/build/runtime/release/z42vm` 旧于 `src/runtime/`（`perf-bulk-array-copy` 的新
>    builtin 未进已建 VM）。`xtask test` 的构建波里 **`build stdlib` 排在 `build runtime` 前**，
>    而 stdlib 自建要用 z42vm → VM 一旦落后，第一个 stage 就炸在指不到真因的 `unknown builtin`
>    上。补跑阶段 8 步骤 1 的 cargo build 后消失。**建议另开 change**：把 `build runtime` 提到
>    stdlib 前，或在 stdlib 自建前加一道「VM 旧于 runtime 源」显式检查。
> 2. `sections mismatch: z42c.{semantics,pipeline,driver} gen2 != gen1`：种子代差——warm
>    `artifacts/` 建自 33ffb3ca，而 worktree 在 40d274c2，中间隔着 **#490（zpkg 内容标识换
>    MurmurHash3，直接改 zpkg 字节）**；gen1 由旧编译器产、gen2 由新编译器产，差一代不收敛。
>    **对照实验**：同一 worktree `git stash -u` 掉全部改动、纯 `origin/main` 重跑 `test compiler`
>    → `✅ 不动点 3/3 gen1==gen2`，证明与本变更无关（本变更一行未碰 z42c）。跑过一代后自愈。

## 验证（publish 行为的手工验证）
| 场景 | 期望 | 结果 |
|------|------|------|
| 改 `scripts/xtask_cli.z42` → `z42 publish scripts/xtask.z42.toml` | 重编 + 重出 zpkg | ✅ `cached: 0/61` → `wrote -> xtask.zpkg`，SHA 变化（修复前 SHA 不变、无任何编译输出） |
| 无改动再 publish | 空转、零字节重写 | ✅ `cached: 61/61` / `no changes; preserved ->`（~0.5s） |
| 改源码 + `publish --no-build` | 完全不编译 | ✅ 直接 `apphost ready`，无 z42c 输出 |
| staged 依赖 zpkg 被替换成陈旧内容后重 publish | 覆盖回真字节 | ✅ `z42c.semantics.zpkg` SHA 复原（修复前保持陈旧） |

## 备注
- **旁证观察（不在本 Scope）**：xtask 61 文件里只改一个 `.z42`（甚至只加一行注释），z42c 增量报
  `cached: 0/61`——保守闭包把全包判失效。正确但成本高；与
  `docs/spec/changes/add-switch-exhaustiveness/tasks.md` 记的「增量构建 staleness」是同一片区域，
  建议另开 change 评估。
- `z42 publish` 至今**不转发 `--self-contained`**（launcher 的 ArgParser 未注册该旗标，只有
  `z42b publish` 有）。已存在的缺口，非本次引入，未在本 Scope 内修。
