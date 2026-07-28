# Tasks: add-tests-bench-manifest-config

> 状态：🔴 DRAFT（待 User 6.5 审批）| 创建：2026-07-29 | 见 [proposal.md](proposal.md) / [design.md](design.md)
> 锁：stdlib + toolchain（均被占 → **User 授权隔离 worktree 预抢**，合并解冲突）
> 分三阶段，每阶段独立可 commit + 可全绿。GREEN 以 CI 权威（cold worktree 不可本地验自举链）。

## 进度概览
- [ ] P1: 清单 schema（stdlib：模型 + 解析器 + 单测）
- [x] P2: toolchain test/bench runner（清单发现 + harness 分派 + 退出码 + 具名过滤）✅ 代码落地（GREEN 待 CI）
- [x] P3: example runner（编译门禁 + `xtask example` + test=true 执行）✅ 代码落地（GREEN 待 CI）
- [x] P4: 端到端夹具 + wire（文档 project.md L5b 已 P4.1；余文档由 owner 收尾）✅ 代码落地（GREEN 待 CI）

## P1 — 清单 schema（stdlib 锁）✅ 代码落地（2026-07-29，GREEN 待 CI）
- [x] 1.1 NEW `RunTarget.z42`：`Name`/`Harness`/`HasEntry`/`Entry`/`Sources[]`/`SrcCount`/`Deps[]`/`DepCount`/`RunInTest`
- [x] 1.2 NEW `TargetSection.z42`：`Include[]`/`IncCount`/`Exclude[]`/`ExcCount`/`Auto`/`Deps[]`/`DepCount`
- [x] 1.3 MODIFY `ProjectManifest.z42`：加 `TargetSection Tests/Bench/Example` + `RunTarget[] Tests/Benches/Examples` + counts（改构造函数）
- [x] 1.4 MODIFY `ManifestLoader.z42`：`_parseTargetSection(root,key,defaultInclude)` + `_parseRunTargets(root,key)`（仿 `_parseExes`）；接进 `ParseText`。**段名复数**（`tests`/`benches`/`examples`）避 TOML key 撞（design D8）
- [x] 1.5 NEW `tests/tests_bench_example_targets.z42`：段默认/include-exclude/auto/目标字段/harness 两态/example test=true/per-target dev-deps/多目标顺序（7 用例）。**错误码校验（缺 name / 重名 / harness=false 缺 entry）下移到 P2 发现层**——ManifestLoader 只做忠实解析，不校验语义
- [x] 1.6 MODIFY `z42.project/README.md`：核心文件补 `RunTarget`/`TargetSection` + ManifestLoader 段说明

## P2 — test/bench runner（toolchain 锁）✅ 代码落地
- [x] 2.1 清单驱动发现：NEW `xtask_test_targets.z42`（engine）——ManifestLoader 读段 + `[[test]]`；auto 走既有约定目录扫描（**扫描前 sort**，确定序），`_dropOverridden` 让显式覆盖同名 auto
- [x] 2.2 harness 分派：harness=true → `_runReflectTarget`（约定单元仍复用 `_runUnitsBatched` z42b）；harness=false → `_runExitTarget`（合成 exe mini-manifest → z42c build → `z42vm <zpkg>`，退出码判定，无 golden）
- [x] 2.3 三层 dep 合并（target > section > project）：`_baseDeps` / `_targetDeps`（ManifestLoader），接进 `_renderTargetManifest`；替换旧 raw-toml `_harvestParentDeps`
- [x] 2.4 MODIFY `xtask_test_lib.z42`：per-lib body → `_runLibKind`（清单感知，保留库约定兜底；对无段/目标的现有 lib 字节等价）
- [x] 2.5 bench 目标：`bench stdlib` 经共享 `_testLibCore`→`_runLibKind` 已清单感知；`bench targets [name]` 具名过滤（xtask_bench.z42 无需改——bench e2e 与本模型正交）
- [x] 2.6 MODIFY `xtask_cli.z42`：`test targets [name]` / `bench targets [name]` 具名精确选择（对齐现有 `test stdlib` 子命令结构；spec 的 `test <name>` 落地为 `test targets <name>`）；名不存在 → 报错列出可用目标名
> **注（事实校正）**：xtask.z42.toml **不加** `[dependencies]`——DepScan.z42:126 `declaredCount==0` 时索引整个 Z42_LIBS；加偏依赖会翻转成「只认声明」模式，反而打断 Std.Cli/Std.IO/Z42.Build 解析。`using Z42.Build.Project` 已从扁平 alllibs 自动解析（z42.project 是 stdlib，已在 Z42_LIBS）。packages.toml 也无需改（z42.project 已在 stdlib-glob）。

## P3 — example runner（toolchain 锁）✅ 代码落地
- [x] 3.1 NEW `xtask_test_example.z42`：发现 example 目标（段 auto 约定 `examples/` + `[[example]]`）；编译全部（门禁）；跑 `RunInTest==true` 的；退出码判定
- [x] 3.2 MODIFY `xtask_cli.z42`：新增 `example [name]` verb（跑单个 / 无 name 跑全部 test=true）
- [x] 3.3 MODIFY `xtask_test.z42`：wire example 编译门禁进主 `xtask test` gate（`examples` stage）

## P4 — 文档 + 夹具 + GREEN
- [x] 4.1 **重写** `docs/design/compiler/project.md` L5b（此前已由 owner 落地，本 worktree commit b53d5018）
- [ ] 4.2 `docs/workflow/testing/` 命令面：`xtask test/bench/example` 目标（owner 收尾）
- [ ] 4.3 `docs/features.md` / `docs/roadmap.md`：清单目标能力状态（owner 收尾）
- [x] 4.4 NEW 端到端夹具 `src/tests/manifest-targets/basic/`（`[[test]]`×2 harness 两态 + auto_conv 约定 + `[[bench]]` + `[[example]]` test=true）；wire 进 `xtask test`（`manifest targets` + `examples` stage）+ golden 扫描器排除（`_isNonRegenCat`/`_isNonRunnableCat` 加 `manifest-targets`）
- [ ] 4.5 命令面 grep 清零：`grep -rn "旧命令/字段" docs/ scripts/ .claude/`（owner 收尾）
- [ ] 4.6 GREEN：CI 权威（cold worktree 本地不可验自举链）

## 备注
- **规范冲突（已在讨论解决）**：project.md L5b 旧设计与本 change 定稿冲突（`src` vs `entry`+`sources`、无 harness、无 example、golden vs exit-code）→ 本 change 重写 L5b（design.md D1–D4）。
- **compiler 不改**：test/bench/example dev-only，xtask 合成 mini-manifest 调 z42c 子进程，driver 零改动 → 不需 compiler 锁。
- **golden 并存**：`src/tests/**` expected_output.txt harness（xtask_test_vm）不动、不迁移。
