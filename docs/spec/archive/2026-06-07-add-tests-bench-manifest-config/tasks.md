# Tasks: add-tests-bench-manifest-config

> 状态：🟢 已完成 | 创建：2026-06-06 | 完成：2026-06-07 | 类型：lang/compiler/build feat

## Phase 1 — Spec（本提交）

- [x] 1.1 proposal.md：设计 + 约定 + schema + 错误码 + 编译模型 + 迁移
- [x] 1.2 tasks.md：本文件
- [x] 1.3 commit + push

## Phase 2 — C# 编译器 schema 扩展

- [x] 2.1 [ManifestErrors.cs](../../../../src/compiler/z42.Project/ManifestErrors.cs)：添加 WS012 + WS040 + WS041 + WS042 + WS043 错误码常量 + factory 方法
- [x] 2.2 [ProjectManifest.cs](../../../../src/compiler/z42.Project/ProjectManifest.cs)：
    - [x] 2.2.1 新增 record：`TestsConfig` / `BenchConfig` / `TestEntry` / `BenchEntry`（含 `Name` / `Src` / `Sources` / `Dependencies`）
    - [x] 2.2.2 `ProjectManifest` 字段加 `Tests`, `Bench`, `TestEntries`, `BenchEntries`
    - [x] 2.2.3 `KnownTopLevelKeys` 加 `"tests"`, `"bench"`, `"test"`, `"benchmark"`
    - [x] 2.2.4 `KnownTestsKeys` / `KnownBenchKeys`（include / exclude / dependencies）
    - [x] 2.2.5 `KnownTestEntryKeys` / `KnownBenchEntryKeys`（name / src / sources / dependencies）
    - [x] 2.2.6 `ParseTests` / `ParseBench` / `ParseTestEntries` / `ParseBenchEntries` helper（依现有 helper 模式）
    - [x] 2.2.7 `LoadWithWarnings` 中扫描 `[dependencies]`：若包含已知 test-only deps（z42.test）→ emit WS012
    - [x] 2.2.8 [[test]] / [[bench]] 缺 name / src → WS040 / WS041；重名 → WS042；src 不存在 → WS043
- [x] 2.3 C# 单元测试（[z42.Tests](../../../../src/compiler/z42.Tests)）：
    - [x] 2.3.1 ProjectManifestTests：parse [tests] / [bench] / [[test]] / [[bench]] 各组合
    - [x] 2.3.2 ManifestErrorsTests：WS012 / WS040-WS043 round-trip
- [x] 2.4 GREEN：`dotnet test src/compiler/z42.Tests/z42.Tests.csproj`
- [x] 2.5 commit + push（compiler schema 单元）

## Phase 3 — xtask dir-mode 发现 + synthetic mini-manifest + 输出目录隔离

- [x] 3.1 [xtask_test.z42](../../../../scripts/xtask_test.z42)：
    - [x] 3.1.1 测试发现循环：识别 `tests/<name>/source.z42` dir-mode（`_discoverTestUnits`）
    - [x] 3.1.2 dir-mode 测试编译路径：合成 synthetic mini-manifest 至 cache，传 `z42c build <toml>`（`_compileTestUnit` + `_renderSyntheticManifest`）
    - [x] 3.1.3 [tests.dependencies] 双层 dep 合并（合入合成 manifest 的 [dependencies]；`_harvestParentDeps` + `_appendDeps`）— 三层（含 `[[test]].dependencies`）随 Phase 5 demo 引入实际用例时补
    - [x] 3.1.4 合成 manifest `[build].output_dir` → **持久** `<lib>/<profile>/tests`（dist 共享、cache 每 unit）；改自原 `<work>/<lib>/<unit>` 临时目录（commit `75710a80`，§6 布局）
    - [x] 3.1.5 zpkg 命名：单文件 `<lib>.test.<filename_no_ext>`；dir-mode `<lib>.test.<dirname>`
- [x] 3.2 bench 路径（commit `0c37a05e`）：`_testLib(argv, kind)` 按 kind ∈ {test,bench} 泛化；`bench stdlib` 路由到 bench/；迁移 5 个 [Benchmark] 文件 tests/→bench/（collections/json/math + z42.test×2）；输出 `<lib>/<profile>/bench/dist/`。bench 5/5 + test 260/260 GREEN
- [x] 3.3 sort 修复：`Directory.Enumerate` first-wins → `_sortedStrings` 显式 sort（common-pitfalls §1）
- [x] 3.4 [xtask.z42](../../../../scripts/xtask.z42)：`clean` / `clean tests` / `clean bench` / `clean all` 子命令（commit `316ecba7`）；验证 clean tests 删 22 dir、clean bench 删 4 dir，生产 cache/dist 保留
- [x] 3.5 integration smoke：`test stdlib` 260/260 + `bench stdlib` 5/5 + `clean tests|bench` 选择性删除 端到端验证覆盖（xtask 为 z42 程序，无独立单测框架；其自身每次运行即 smoke）
- [x] 3.6 commit + push（xtask dir-mode 单元）

## Phase 4 — 22 stdlib z42.toml 迁移

> 状态：✅ 已落地 by [`6aac4c5d stdlib: migrate 21 packages to [tests.dependencies] = z42.test`](https://github.com/z42-lang/z42/commit/6aac4c5d) (2026-06-06)。21 个使用 z42.test 的 stdlib 包统一在一个 commit 里完成迁移（z42.test 自身不需迁移）；spec 原"逐包 commit"细分降级为单 commit 批量执行，因为 21 处都是同形 diff + 必须一起绿才算成功，分 commit 反而不可拆。

- [x] 4.1 一次性 sed/awk migration（commit 内不入仓库，commit 即落地）
- [x] 4.2 21 包合并到 commit `6aac4c5d`（z42.test 自身不迁）
- [x] 4.3 / 4.4 全量验证：commit 落地时 `xtask test stdlib` 全绿；本 spec Phase 3 落地后再次跑 `xtask test stdlib z42.core --no-build` 通过 6/6（验证新 xtask + 已迁移 toml 仍 GREEN）

## Phase 5 — 多文件测试 demo

> 状态：✅ 已完成（2026-06-07）—— TIDX 聚合阻塞已解除。spec [`aggregate-zpkg-tidx`](../../archive/2026-06-06-aggregate-zpkg-tidx/) 落地 VM 侧 zpkg loader 的多 module TIDX 聚合（commit `0b453b2e`）；secp256k1 dir-mode demo re-introduce（commit `7b2a1b2a`，并修了 GoldenTests dir-mode 误把 [Test] 库测试当 Main 型 golden 的问题）。端到端验证：`xtask test stdlib z42.crypto` 中 secp256k1 dir-mode 10/10 [Test] 全过；VM goldens `OK: secp256k1`。
>
> （历史）原阻塞：xtask 端 dir-mode 基础设施已完整（Phase 3 + Phase 5 polish 落地：dir-mode 发现 / synthetic manifest 写入到源目录旁 / 删除清理 / `[profile.*].pack = true` 强制 packed / `IsSyntheticHarnessProject` 抑制 WS012）；端到端阻塞在 VM 侧 zpkg loader 的 TIDX 聚合，2026-06-06 试跑 secp256k1 demo 时确认。
>
> **触发**：synthetic 多文件项目编译产出 packed zpkg；z42-test-runner 调 `load_artifact(zpkg)` 时，[src/runtime/src/metadata/loader.rs:312-321](../../../../src/runtime/src/metadata/loader.rs#L312-L321) 显式写 `test_index: vec![]` 并附注释 _"R1: zpkg test metadata aggregation deferred. R3 runner reads individual .zbc files directly via load_artifact, where TIDX sections are populated. Setting empty here is correct for now."_ 测试发现因此为空，runner 退出 3 (no tests found)。
>
> **2026-06-06 后续调查发现 gap 比初始估计大**：进一步看 [src/compiler/z42.Project/ZpkgWriter.cs](../../../../src/compiler/z42.Project/ZpkgWriter.cs) 和 [src/runtime/src/metadata/zbc_reader.rs](../../../../src/runtime/src/metadata/zbc_reader.rs) `read_mods_section` 后确认：ZpkgWriter 根本**不**写 TIDX 段到 zpkg（per-module MODS 段只含 FUNC / TYPE / DBUG / REGT）。reader 端 aggregate 无源头可读 —— 必须同时改 writer。完整解锁需：
>
> 1. **格式扩展**：每 module MODS 后加 TIDX 段（或顶层 zpkg 加一个聚合 TIDX）
> 2. **writer**：ZbcWriter / ZpkgWriter 同步写
> 3. **reader**：`read_mods_section` 读 TIDX 并 remap `method_id` / `skip_reason_str_idx`（merge_modules 已有的 function/string 偏移规则可复用）
> 4. **zpkg minor bump**：strict-pin policy，写完成步 1 + version-bumping.md 4 处全套更新（ZbcWriter / zbc_reader / zpkg.md changelog / 6+4 fixture regen）
> 5. **VM 侧 LoadedArtifact.test_index 填充**：把 reader 输出回 [loader.rs](../../../../src/runtime/src/metadata/loader.rs)
>
> 估时上修：~3-4 天（zpkg 格式 + writer + reader + fixture regen + 测试）。比初始 1-2 day 估算翻倍，因 minor bump 触发整套 wire-format protocol。仍是 `vm` 类型，必须 spec-first。
>
> Phase 5 demo 文件本身（`tests/secp256k1/{source,vectors}.z42`）在本次试跑后已恢复回单文件 `tests/ecdsa_secp256k1_vectors.z42`；落地 TIDX 聚合后再 re-introduce。

- [x] 5.0 [起 spec `aggregate-zpkg-tidx`]：vm-side 改动，在 zpkg 加载时聚合多 module TIDX 段 —— 已落地并归档（commit `0b453b2e`）
- [x] 5.1 demo 重做：z42.crypto 的 secp256k1 测试改造为 dir-mode（`tests/secp256k1/{source,vectors}.z42`，commit `7b2a1b2a`）
- [x] 5.2 验证 dir-mode 路径全跑通（compile → load_artifact → test discovery → run）—— secp256k1 10/10 + VM golden `OK: secp256k1`

## Phase 6 — CI 守门 + 文档同步

- [x] 6.1 [.github/workflows/ci.yml](../../../../.github/workflows/ci.yml) 新增 release-guard step（build-and-test job 内，ubuntu-latest only）：
    - [x] 6.1.1 forward：production `dist/` 不许含 `.test.` / `.bench.` zpkg（排除 `*/tests/dist/*` 和 `*/bench/dist/*` 子树）—— commit `55e318fc` 起步 + `<本commit>` 完善排除规则
    - [x] 6.1.2 reverse：`tests/dist/` 只许含 `.test.` zpkg；`bench/dist/` 只许含 `.bench.` zpkg（commit `<本commit>`，防未来 refactor 把生产 zpkg 写进 harness 子树）
- [x] 6.2 [docs/design/compiler/project.md](../../../design/compiler/project.md) 同步新 schema（commit `823ef37f` L5b 章节 + `abd94e25` synthetic 抑制条款）：
    - [x] 6.2.1 `[tests]` / `[bench]` 段语义
    - [x] 6.2.2 `[[test]]` / `[[bench]]` 数组
    - [x] 6.2.3 三层 dep 合并模型
    - [x] 6.2.4 输出目录布局（与 §restructure-build-output-dirs 的 output_dir/cache_dir/dist_dir 衔接）
    - [x] 6.2.5 WS012 synthetic harness 例外
- [x] 6.3 [docs/design/runtime/zpkg.md L104](../../../design/runtime/zpkg.md) 已说明 release zpkg DEPS 段不含 [tests.dependencies] / [bench.dependencies] / [[test]].dependencies；CI release-guard 作最后防线（与本 spec 同期归档）
- [x] 6.4 评估结论：**不**需补 `.claude/rules/` 规则。WS012 已机械化由编译器执行，rule 是给 Claude 看的人工约定；compiler-enforced 规则不必在 .claude 复述（同 WS001-WS043 全家族均无对应 .claude rule）
- [x] 6.5 spec 归档：docs/spec/changes/ → docs/spec/archive/2026-06-07-add-tests-bench-manifest-config/（阻塞已清：Phase 3.2 bench / 3.4 clean / 3.5 smoke 全部完成 2026-06-07，Phase 5 demo 已解锁，Phase 2 schema 早落地）

## 验收（GREEN）

- [x] dotnet test src/compiler/z42.Tests/z42.Tests.csproj 全绿（1516/1516，含 TestBenchManifestTests 32 facts + DiagnosticCatalogRoutingTests 7 facts）
- [x] xtask test stdlib 全绿（22 包，265 test files；commit `6aac4c5d` 落地时验证）
- [x] xtask bench stdlib 跑通（per-lib micro-bench：5 单元 / 4 库，输出 `<lib>/<profile>/bench/dist/`；commit `0c37a05e`）
- [x] 全 stdlib 扫描 WS012 零触发（21 包均无 z42.test 在 [dependencies]）
- [x] 一个 dir-mode 多文件测试 demo 跑通（z42.crypto/tests/secp256k1，10/10；Phase 5 解锁 by aggregate-zpkg-tidx）
- [x] CI release-guard step（forward + reverse）落地
- [x] `xtask clean tests` / `clean bench` 独立可清（commit `316ecba7`：clean/clean tests/bench/all 落地，选择性删除验证通过）
- [x] roadmap.md / 0.3.x A 主线：add-tests-bench 是 0.3.x Stream A（stdlib 重组）配套工程；本 spec 完成不单列 roadmap 行（roadmap 跟踪的是语言/pipeline 里程碑，非工具链子任务）
