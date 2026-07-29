# Tasks: add-tests-bench-manifest-config

> 状态：🔴 DRAFT（待 User 6.5 审批）| 创建：2026-07-29 | 见 [proposal.md](proposal.md) / [design.md](design.md)
> 锁：stdlib + toolchain（均被占 → **User 授权隔离 worktree 预抢**，合并解冲突）
> 分三阶段，每阶段独立可 commit + 可全绿。GREEN 以 CI 权威（cold worktree 不可本地验自举链）。

## 进度概览
- [ ] P1: 清单 schema（stdlib：模型 + 解析器 + 单测）
- [ ] P2: toolchain test/bench runner（清单发现 + harness 分派 + 退出码 + 具名过滤）
- [ ] P3: example runner（编译门禁 + `xtask example` + test=true 执行）
- [ ] P4: 文档同步 + 端到端夹具 + GREEN

## P1 — 清单 schema（stdlib 锁）✅ 代码落地（2026-07-29，GREEN 待 CI）
- [x] 1.1 NEW `RunTarget.z42`：`Name`/`Harness`/`HasEntry`/`Entry`/`Sources[]`/`SrcCount`/`Deps[]`/`DepCount`/`RunInTest`
- [x] 1.2 NEW `TargetSection.z42`：`Include[]`/`IncCount`/`Exclude[]`/`ExcCount`/`Auto`/`Deps[]`/`DepCount`
- [x] 1.3 MODIFY `ProjectManifest.z42`：加 `TargetSection Tests/Bench/Example` + `RunTarget[] Tests/Benches/Examples` + counts（改构造函数）
- [x] 1.4 MODIFY `ManifestLoader.z42`：`_parseTargetSection(root,key,defaultInclude)` + `_parseRunTargets(root,key)`（仿 `_parseExes`）；接进 `ParseText`。**段名复数**（`tests`/`benches`/`examples`）避 TOML key 撞（design D8）
- [x] 1.5 NEW `tests/tests_bench_example_targets.z42`：段默认/include-exclude/auto/目标字段/harness 两态/example test=true/per-target dev-deps/多目标顺序（7 用例）。**错误码校验（缺 name / 重名 / harness=false 缺 entry）下移到 P2 发现层**——ManifestLoader 只做忠实解析，不校验语义
- [x] 1.6 MODIFY `z42.project/README.md`：核心文件补 `RunTarget`/`TargetSection` + ManifestLoader 段说明

## P2 — test/bench runner（toolchain 锁）
- [ ] 2.1 MODIFY `xtask_test_lib_units.z42`：发现从纯目录约定 → 清单段(include glob)+`[[test]]`；**扫描前 sort**（确定序）；显式覆盖同名 auto
- [ ] 2.2 harness 分派：harness=true 复用 `_runUnitsBatched`（z42b）；harness=false 新分支 `z42vm <artifact> <entry>` 判退出码
- [ ] 2.3 三层 dep 合并（target > section > [dependencies]）接进 mini-manifest 合成
- [ ] 2.4 MODIFY `xtask_test_lib.z42`：段/目标驱动发现；保留库约定兜底
- [ ] 2.5 MODIFY `xtask_bench.z42`：清单 bench 目标 + `bench <name>` 过滤
- [ ] 2.6 MODIFY `xtask_cli.z42`：`test <name>` / `bench <name>` 具名选择；名不存在报错列名

## P3 — example runner（toolchain 锁）
- [ ] 3.1 NEW `xtask_test_example.z42`：发现 example 目标（段 glob + `[[example]]`）；编译全部（门禁）；跑 `RunInTest==true` 的；退出码判定
- [ ] 3.2 MODIFY `xtask_cli.z42`：新增 `example [name]` verb（跑单个）
- [ ] 3.3 MODIFY `xtask_test.z42`：wire example 编译门禁进主 `xtask test` gate

## P4 — 文档 + 夹具 + GREEN
- [ ] 4.1 **重写** `docs/design/compiler/project.md` L5b（harness/exit-code/example/glob；对齐 `[[exe]]`；错误码更新；删悬空前向引用改指向本 archive）
- [ ] 4.2 `docs/workflow/testing/` 命令面：`xtask test/bench/example <name>`
- [ ] 4.3 `docs/features.md` / `docs/roadmap.md`：清单目标能力状态
- [ ] 4.4 NEW 端到端夹具（含 `[[test]]`×2(harness 两态)+`[[bench]]`+`[[example]]` test=true 的工程）；wire 进 `xtask test`
- [ ] 4.5 命令面 grep 清零：`grep -rn "旧命令/字段" docs/ scripts/ .claude/`
- [ ] 4.6 GREEN：`cargo build` + `xtask test`（全 stage + 新 example 门禁）+ z42.project 单测 + 自举不动点（A/B z42.project 不漂移 z42c 字节）—— CI 权威

## 备注
- **规范冲突（已在讨论解决）**：project.md L5b 旧设计与本 change 定稿冲突（`src` vs `entry`+`sources`、无 harness、无 example、golden vs exit-code）→ 本 change 重写 L5b（design.md D1–D4）。
- **compiler 不改**：test/bench/example dev-only，xtask 合成 mini-manifest 调 z42c 子进程，driver 零改动 → 不需 compiler 锁。
- **golden 并存**：`src/tests/**` expected_output.txt harness（xtask_test_vm）不动、不迁移。
