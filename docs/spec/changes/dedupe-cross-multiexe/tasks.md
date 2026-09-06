# dedupe-cross-multiexe — 收敛剩余的两处字节级重复（e2e 构建前置 + curl 下载）

> 类型：`refactor`（最小化模式）。scripts/ 结构优化程序**第 3 批**续
> （#489 命名 → #492 分层 → #496 一致性门 → 本次）。

## 先做的事：核实 backlog，发现三条陈述过期

开工前逐条核了程序 backlog，**四项里三项的记述与现状不符**——直接照着做会返工或做无用功：

| backlog 记的 | 实况 |
|---|---|
| cross-zpkg ↔ multi-exe 的 `_testXImpl` / `_runOneX` **逐行同构** | **大半已被解决**：① toolset 解析早已抽成 `_resolveToolset`（`common/xtask_toolset.z42`，两文件注释都写着「此前两处逐字重复」）；② `_testCrossZpkgImpl` 已被 parallel-cross-zpkg 重写成**分波并发**，`_runOneCross` 根本不存在了，与串行的 `_testMultiExeImpl` 结构上不再同构；③ 通用 helper（`_crossSummary`/`_append`/`_enumDirsSorted`/`_firstLines`/`_copyStdlibZpkgs`/`_findToml`/`_fixtureDist`/`_invokeBuildCompiler`）全部已共用。**只剩 12 行构建前置。** |
| `deps check` 反调 install 侧 `_setup*(mode="check")` 是「可做项」 | **不是缺陷**：`mode="install"｜"check"` 是**有意复用**且已在 `xtask_install.z42:283` 注释说明（「check 供 deps check 复用，verify-only 不装」）。拆开反而要把各平台 presence 探测复制一份。**应关闭，不做。** |
| 大文件：`xtask_bench.z42` 795 行、`xtask_test_embedded.z42` ~630 行 | `xtask_bench.z42` 已**涨到 1139**；`xtask_test_embedded.z42` 只有 **317**（早被拆过）。数字全部过期。 |
| curl 调用 3 处字节级重复 | ✅ 属实。 |

> 教训与 `scripts/` 这轮的主题一脉相承：**没有测试盯着的记述迟早会烂**，backlog 也一样。
> 动手前先核，不要照着旧笔记施工。

## 本次做的两项

### 1. e2e fixture 构建前置去重

`_testCrossZpkgCore`（`test/xtask_test.z42`）与 `_testMultiExeCore`
（`test/xtask_test_multiexe.z42`）的构建前置**逐行相同**（cargo 建 debug z42vm →
缺 stdlib 则 `_buildStdlib` → `_buildCompiler`，外加 `noBuild` / `--toolchain` 两个跳过条件），
只有最后一行调用的 `_testXImpl` 不同。

抽成 `_ensureFixtureToolchain(bool noBuild)`（放 `test/xtask_test.z42`，两个 harness 共用）。

**为什么这处特别值得去重**：这里编码的是**次序**——z42vm 必须先于 stdlib 自建
（stdlib 正是用 z42vm 建的）。次序存两份 = 改一处漏一处。**在飞的 #497 修的正是
另一条构建波上的同类次序问题**（`_buildStdlib` 排在 `_buildRuntime` 前 → 报错指不到真因）。

### 2. `_curlDownload(url, outPath)`

三个下载点（node / android cmdline-tools / gradle）各自手拼同样的
`--fail --location --silent --show-error -o <out> <url>`。收成一处。

**这四个 flag 不是可选装饰**，尤其 `--fail`：没有它，curl 遇到 4xx/5xx 会把错误页
**以 0 退出码**写进 outPath，于是一个 HTML 错误页被当成 tarball/zip 交给下游解压；
android 那两处还会先过 sha256 校验，**报出来的是「哈希不符」而不是「下载失败」**——
又一个指不到真因的报错。收敛后新增下载点不会漏 flag。

## 验证

1. **编译期即证**：扁平 namespace + 裸名互调 → 少一个函数即 `E0401`。
   `z42c build scripts/xtask.z42.toml --release` 0 错误。
2. `grep 'Process("curl")'` 只剩 helper 自身一处。
3. `xtask test` 全绿 10/10 stage —— gate 里 **cross-zpkg 与 multi-exe 两个 stage 都在**，
   正好覆盖被抽出的前置（两条路径各走一次）。
4. curl 三个下载点属 `deps install`（装 Android SDK / node / gradle），**不在 GREEN gate 内**、
   本地也不宜真跑（会下载几百 MB 并改动机器全局状态）。故此处**只做静态等价论证**：
   改动是纯粹的「同样 4 个 flag + 同样两个参数」提取，`Process` 构造与 `_exec` 调用
   逐字未变——已在 tasks.md 记明未做运行时验证，不谎称跑过。

## 状态

🟢 完成
