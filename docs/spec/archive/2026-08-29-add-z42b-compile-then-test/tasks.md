# Tasks: z42b test compile-then-test

> 状态：🟢 已完成 | 创建：2026-08-29 | 完成：2026-08-29 | 类型：feat(toolchain+compiler)
> 分支/worktree：add-z42b-compile-then-test（基于 origin/main）
> GREEN：`xtask test` 全 stage ✅（不设 Z42_HOME）——0 FAIL；z42c self-host 3/3 byte-identical；
> compile-then-test smoke ✅；z42ccompiler `test_compiles_lib_no_main` ✅。

## 进度概览
- [x] 阶段 1: 抽 `_buildProject`/`BuildOutcome` 助手（refactor，单独 commit）
- [x] 阶段 2: test compile-then-test 接线（feat）
- [x] 阶段 2.5: Z42cCompiler 尊重 kind=lib（扩展，User Option 2）
- [x] 阶段 3: 测试 fixture + fixtures smoke
- [x] 阶段 4: 文档同步 + GREEN

## 阶段 1: 重构（单独 commit）
- [x] 1.1 `builder_commands.z42`：抽 `_buildProject(ParseResult r, string mode) → BuildOutcome{Rc,Dist}`
      （ManifestLoader.Load → `_makeTarget(r,mode)` → `_orchestrate` → copy app.zpkg→dist/<name>.zpkg
      → `_pubBundleProjectDeps`；`Dist`=dist zpkg 路径，`""`=无 dist/失败）。**决策调整**：返回
      `BuildOutcome` 而非裸 string——保留 `_runVerb` 精确 exit code（2 用法错 / orchestrate rc）
- [x] 1.2 `_runVerb` 改调 `_buildProject`（export 保留 --rid 前置校验；行为不变）
- [x] 1.3 GREEN 确认重构无回归（`xtask test`）—— 待阶段 4.3

## 阶段 2: 接线（feat）
- [x] 2.1 `builder_cli.z42`：test/bench ArgParser 加 `--release` flag；test/bench positional help 改
      `<file.zbc|.zpkg|project.z42.toml>`
- [x] 2.2 `builder_test.z42`：`_runModule` — `.zbc/.zpkg` → `RunModule`（不变）；否则（空→`z42.toml`
      默认 / `.toml`）→ `_buildProject(r,"")` → `RunModule(Dist, format)`；缺 manifest 由 `_buildProject`
      报 rc 2
- [x] 2.3 更新 `builder_test.z42` 顶部注释（移除 "pending wire-z42b-host-build"，改述双形态已接入）

## 阶段 2.5: Z42cCompiler 尊重 kind（扩展，User 裁决 Option 2）
- [x] 2.5.1 `ICompiler.z42`：`CompileRequest` 末位加 `string Kind`（"exe"/"lib"）
- [x] 2.5.2 `Pipeline.z42` `Compile` 相位构 req 传 `ctx.Project.Kind`
- [x] 2.5.3 `Z42cCompiler.z42`：`inp.Kind = req.Kind`（`""`→默认 exe）；lib 免 Main（PackageCompile 天然）
- [x] 2.5.4 `builder_hooks.z42` 合成 req 传 `"exe"`
- [x] 2.5.5 `z42ccompiler_tests.z42`：3 构造点补 kind 参 + 新增 `test_compiles_lib_no_main`
- [x] 2.5.6 GREEN：`xtask test compiler`（z42ccompiler 单测含 lib 用例）—— 待阶段 4.3

## 阶段 3: 测试
- [x] 3.1 NEW `src/tests/manifest-targets/compile-then-test/`：`z42.toml`(**kind=lib**——无 Main，靠
      阶段 2.5 kind-honoring + z42.test dep) + `src/Tests.z42`（2 个自由函数 `[Test]` 断言）+
      `.gitignore`（level-4 目录，按约定无 README，同 `basic`）
- [x] 3.2 `xtask_test_fixtures.z42`：加 `_smokeCompileThenTest`（`z42b test <toml>` → assert rc==0）
      挂入 `_testTargetsCore`（仅 `kind=="test" && name==""` 全量跑一次）
- [x] 3.3 `xtask test targets` + 完整 `xtask test` 均绿（smoke `✅ passed`）

## 阶段 4: 文档 + GREEN
- [x] 4.1 `src/toolchain/builder/README.md` `builder_test.z42` 行登记 compile-then-test 双形态
- [x] 4.2 `cross-platform-testing.md` 分阶段块拆 ②a（host compile-then-test 已落）/②b（平台委托待做）
- [x] 4.3b `src/tests/manifest-targets/README.md` 功能索引 + 核心文件登记 compile-then-test 夹具
- [x] 4.3 完整 `xtask test` 全绿
- [x] 4.4 归档 + PR

## 备注
- **环境坑（GREEN 验证时踩，非本变更）**：worktree 里 `export Z42_HOME=.z42` 会让 e2e goldens 跑在
  SDK VM `.z42/bin/z42vm`，其旁 `libz42_repl.dylib` 触发 spurious ext WARN → `xtask_test_vm` 把
  stderr 并入 actual → **全部 golden 假红**（expected==actual 却 FAIL）。**正解：跑 `xtask test`
  不设 Z42_HOME**（实测 e2e `0 FAIL`）。已记 memory [[xtask-test-z42home-repl-warn-pollution]]。
  → 引出 User 的 repl cdylib 隔离修正（独立 change，把 libz42_repl 挪出共享 bin/）。
- **发现的既有文档债（非本变更引入，Scope 外）**：`src/toolchain/builder/README.md` 仍整体描述
  z42b 为「🔴 占位/未接编译」「_hostCompiler 暂返 NoCompiler」「new/build/export 打 pending」——
  这是 **wire-z42b-host-build 功能落地时未同步 README** 的遗留。本变更只精准更新 `builder_test.z42`
  行（compile-then-test），未全面重写该 README（属 wire-z42b 的文档债，另开变更收拾）。已向 User 报告。
- 决策调整：`_buildProjectDist` → `_buildProject(r,mode) → BuildOutcome{Rc,Dist}`（见 1.1）。
- 前置已就绪：`_hostCompiler` 注入 Z42cCompiler、`z42b build <toml>` 端到端可用。
- seed 教训：主树 `.z42` 曾被并发会话构建改成内部不一致（z42c.pipeline 调 `IrDump.ParseAll$3`
  但 semantics 定义不符）→ 用 `install-z42.sh --version nightly` 拉一致 nightly SDK 解决。
