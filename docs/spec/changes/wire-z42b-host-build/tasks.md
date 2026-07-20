# Tasks: 接入 z42b host build

> 状态：🟡 DRAFT（等待前置：toolchain 锁释放 + extract-compile-pipeline-api 落地）| 创建：2026-06-29
> 子系统：stdlib（z42.project/z42.build 清单）+ z42c（Z42cCompiler）+ toolchain（z42b 登记 + launcher）
> **不持锁**；开工前查 ACTIVE.md 重新登记三子系统（全空闲才开）。

## 进度概览
- [ ] 阶段 1: 包清单（z42.project / z42.build / z42.builder）
- [ ] 阶段 2: 构建登记（workspace + xtask + CI）—— PARKED 代码先能编过
- [ ] 阶段 3: Z42cCompiler : ICompiler（依赖 extract-compile-pipeline-api）
- [ ] 阶段 4: z42b `_hostCompiler` 注入 + `z42b build` 端到端
- [ ] 阶段 5: launcher 转发（AddSpawn）+ z42b apphost 到 bin
- [ ] 阶段 6: GREEN 验证

## 阶段 1: 包清单
- [ ] 1.1 `z42.project.z42.toml`（kind=lib；deps z42.core, z42.toml）
- [ ] 1.2 `z42.build.z42.toml`（kind=lib；deps z42.core, z42.project）
- [ ] 1.3 `z42.builder.z42.toml`（kind=exe，entry z42b；deps z42.build, z42.project, z42c.pipeline, z42.cli, z42.io）
- [ ] 1.4 三包先各自 `z42c build` 编过（PARKED→可编译，修接入时暴露的 API/语法问题）

## 阶段 2: 构建登记
- [ ] 2.1 `src/libraries/z42.workspace.toml` default-members 加 z42.project / z42.build（拓扑序，z42.project 在前）
- [ ] 2.2 xtask `build stdlib` 覆盖新两库（drop-in per-member）
- [ ] 2.3 xtask `build z42b`（或 build sdk 含 z42b）—— 拓扑序在 z42c 之后
- [ ] 2.4 CI 对应 job 纳入（build-and-test 全 OS 编过两库 + z42b）

## 阶段 3: Z42cCompiler : ICompiler
> 等待 extract-compile-pipeline-api 落地 CompileInMemory/CompilePackage
- [ ] 3.1 `z42c.pipeline.z42.toml` 加 dep z42.build（仅接口）
- [ ] 3.2 `Z42cCompiler.z42`：Compile(req) → 读源/deps blobs → CompileInMemory → 写 OutputZpkg → CompileResult
- [ ] 3.3 单测：内存编译 hello 工程 → app.zpkg 可被 z42vm 跑

## 阶段 4: z42b 注入 + 端到端
- [ ] 4.1 builder.z42 `_hostCompiler()` → `new Z42cCompiler()`
- [ ] 4.2 builder.z42 `_initialInputs` 填 Inputs.Deps（扫 Z42_LIBS + 声明依赖 → zpkg 路径）
- [ ] 4.3 `_computeDirs` 与 z42c dist 约定对齐校验（output_dir 默认一致）
- [ ] 4.4 端到端：`z42b new demo` → `z42b build`（cwd demo）产 dist/demo.zpkg → `z42vm dist/demo.zpkg` 输出 Hello

## 阶段 5: launcher 转发 + apphost
- [ ] 5.1 `SubcommandRouter.AddSpawn`（若 add-workload-command-dispatch 未先落则前借到 z42.cli）
- [ ] 5.2 launcher_cli.z42 注册 new/build/publish/export 为 spawn-leaf → `z42vm z42b.zpkg -- <verb>`
- [ ] 5.3 xtask build/sdk 产 z42b apphost（同 launcher 模式 → bin/z42b）
- [ ] 5.4 端到端：`z42 new demo` / `z42 build` / `z42 -h` 列出 z42b 动词

## 阶段 6: GREEN 验证
- [ ] 6.1 `cargo build --release`（z42vm）无错
- [ ] 6.2 `z42 xtask.zpkg test`（全 stage）不回归 —— 新两库 [Test] 纳入 test lib
- [ ] 6.3 端到端 new→build→run 冒烟（阶段 4.4 / 5.4）
- [ ] 6.4 docs：build-orchestrator.md 更新为已接入；ACTIVE.md 释放锁；roadmap 进度

## 阶段 7: build hook —— 免装 workload 的 apphost（2026-07-20 落地）
> 目标（需求③）：`z42 publish` 经**项目 build hook** 现场产 apphost stub → 去掉
> `z42 workload install desktop` 下载依赖。依赖 fix-crosspkg-virtual-override（hook 靠
> override BuildHooks + 读 IPipelineContext 属性运转，两条派发路径经该修复才生效）。
- [x] 7.1 `PipelineContext.Exec` 落地 Std.Process（hook 现场跑 cargo）
- [x] 7.2 z42b `builder_hooks.z42`：manifest `[build] hooks = "<dir>"` → 注入的同一 ICompiler
      编 hook 目录（合成 no-op 入口满足 exe 编译面）→ ModuleLoader.Load → `Build.ProjectHooks`
      → Activator → as BuildHooks；`builder.z42 _orchestrate` 挂 p.Hooks（取代旧「生成一次性
      driver 源码」骨架，也修掉隐式探测 build/ 误判 scripts/build/ 的坑——只认 manifest 显式声明）
- [x] 7.3 publish stub 解析序：① 项目 hook 产出（`_pubHookApphostStub`：BeforeAssets 登记
      kind="apphost-stub"）→ ② Z42_APPHOST_TEMPLATE（已装 workload）→ ③ 报错提示两条路
- [x] 7.4 xtask `scripts/hooks/hooks.z42`（`class ProjectHooks : BuildHooks`，BeforeAssets 现场
      cargo 编 apphost stub 登记）+ `scripts/xtask.z42.toml` 声明 `[build] hooks = "hooks"`
- [x] 7.5 e2e：`z42b publish scripts/xtask.z42.toml`（无 workload、Z42_APPHOST_TEMPLATE 未设）
      → hook cargo 编 apphost → `✅ apphost ready` → apphost 可运行（`xtask -h` 正常）

## 备注
- 阶段 1–2 可在 extract-compile-pipeline-api 之前做（清单 + 登记，_hostCompiler 暂 NoCompiler，build 报「no compiler injected」但能编过）；阶段 3 才需该前置。
- z42c→z42.project 收敛、test/bench、平台 workload、publish 完整迁移 = 各自独立变更（见 proposal Out of Scope）。
