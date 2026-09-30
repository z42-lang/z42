# Tasks: 删掉 Z42cCompiler 的平铺字段编译路径（drop-legacy-compile-request）

> 状态：🟢 已完成 | 创建：2026-09-30 | 类型：refactor（最小化模式；add-build-session 设计里约定「第二步删」的那条旧路径）

**变更说明**：#970 起 z42b 的编译请求带整份清单、走 BuildSession；`req.Manifest == null` 的请求仍退回一条「只看平铺
字段」的旧编译流程，唯一调用方是 hooks 编译（`builder_hooks.z42`）。两份编译流程并存已经分叉过一次（BuildSession
首版漏调 `HandlerRegistry.RunAst`）。

**原因**：hooks 编译改传合成的 lib 清单后，旧路径没有调用方 ⇒ 删掉，`Z42cCompiler` 只剩 BuildSession 薄封装。

**文档影响**：internals `toolchain/z42b.md`（BuildSession 一节 + hooks 机制步骤）、pipeline README。

## 任务
- [x] 1.1 `Z42cCompiler`：删平铺字段编译流程（~110 行）；`req.Manifest == null` → 失败并说明原因
- [x] 1.2 `builder_hooks.z42`：合成 lib 清单（`<proj>.hooks`、include `**/*.z42`）直接编 hook 目录；删 staging 拷贝 + 合成 `Main()`
- [x] 1.3 `xtask_test_targets.z42`：`_smokeManifestSections` 改跑 `tt.Z42bFixturesRoot` 下的拷贝（isolate-xtask-fixture-builds
      漏改了这一条，它此前在 `src/tests` 下留产物）
- [x] 2.1 新 z42b smoke `_smokeBuildHooks` + 夹具 `src/tests/z42b/build-hooks`：`z42b build` 后输出里必须有 hook 标记行
      （判别力已验：请求不带清单 ⇒ hooks 编译失败 ⇒ 判红）。此前门禁里 hooks 只经 `z42 publish` 执行，那条路用的是 `.z42`
      已安装的 z42b + 它自带的编译器，开发树的 hooks 编译路径零覆盖
- [x] 2.2 `z42ccompiler` 单测全部改为带清单请求；新增「无清单请求被拒」
- [x] 3.1 文档
- [x] 3.2 `xtask test` 全绿

## 备注（待 User 裁决，未改）
- hooks 编译 / 加载失败时 z42b 打印诊断后**降级为无 hook 继续构建、退出 0**（`builder.z42` 注释：「保守降级」）。
  publish 路径同样降级（原生依赖不并置 / apphost 缺失）。清单明确声明了 hooks 却失败不判红，是否改为构建失败需要 User 决定。
