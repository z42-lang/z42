# Tasks: z42b 的 dev 目标与 hook 中间产物不再写进源码树

**状态：🟢 已完成 | 开始：2026-09-30 | 完成：2026-10-01（#975）**

类型：`fix`（z42b）+ 新旗标 `--out-root`（加法，不改默认行为）→ 最小化模式。「源码树零写入」系列第 ④⑤ 步。

## ④ hook 中间目录跟随清单（bug）

`builder_publish.z42` 三处各拼一遍 hook 中间目录：本工程两处把 `[build].output_dir` **原样** Join（`${profile}` 等模板不展开），
native 依赖那处（`_pubRunDepProvideNative`）干脆硬编码 `<depDir>/artifacts/<dep>/hooks`、无视依赖自己的 output_dir
⇒ z42.repl（配了 `output_dir`）照样往 `src/toolchain/interactive/repl/artifacts/` 写。
改为共用 `_pubHooksInterDir`：展开 `${profile}`（= release）/ `${project_name}`；未配 output_dir 的默认不变。

## ⑤ `z42b test|bench --out-root <dir>`

stdlib / 编译器成员的清单没有 `[build]`（布局由 workspace 给），z42b 跑它们的 dev 目标时把目标输出写
`<清单目录>/artifacts/<kind>-targets/`、父包写 `<清单目录>/artifacts/<name>/<profile>/` —— 一轮 GREEN 后每个
stdlib / 编译器成员目录下一份。

- z42b：`--out-root` 替换默认产物根。目标输出 `<dir>/<kind>-targets/<目标>`；父包未显式配 output_dir 时在内存里
  补 `<dir>/<name>/${profile}`（与默认布局同形、只换根，`_computeDirs` / `_orchestrate` 不动）。不传 = 行为不变。
- xtask：`_devTargetOutRoot`（xtask_layout.z42）—— workspace 成员 → `<成员 output_dir(debug)>/tests|bench`；
  其它 → `tmp/dev-targets/<name>/…`。lib 单元并行跑与零单元回落两处传入。
- `clean tests|bench` 覆盖新位置。
- ⚠️ out-root 必须是成员输出目录下的**子目录**：z42b 在 out-root 下建父包，直接用 `<output_dir>` 会让父包 dist 覆盖
  z42c 建的正式成员产物。

已知不覆盖：z42b 对「零目标」清单的回落路径（`_buildProject` 编包自身）另行加载清单，不吃 `--out-root`。
stdlib / 编译器成员都经 `tests/` 约定发现目标、不走这条；需要时在「统一默认布局」一步处理。

## 进度概览

- [x] ④ `_pubHooksInterDir` + 三处调用
- [x] ⑤ z42b `--out-root`（test / bench 两个 parser）+ `_runDevTargets` / `_runOneDevTarget`
- [x] ⑤ xtask `_devTargetOutRoot` + 两处调用 + `clean tests|bench`
- [x] 文档：cli-z42.md（test / bench 旗标）、internals testing/framework.md
- [x] 完整 GREEN（9m41s，全阶段通过）。stdlib / 编译器成员目录下不再生成 `test-targets` `bench-targets` 与父包目录，测试产物落 `artifacts/build/libraries/<m>/debug/tests`；repl hooks 中间目录不再进源码树
- [x] **版本错位兜底**（#975 首轮 CI 实测）：bench A/B 的「Capture base micro baseline」用本树 xtask 驱动 **base 树**
      现建的 z42b —— 旧版不认 `--out-root`，`z42b: unknown option '--out-root'` 直接失败。xtask 先用 `z42b test --help`
      探测（`_z42bSupportsOutRoot`，同 `_driverSupportsCompileLibs` 手法；每个 z42b 路径进程内只探测一次），不认就不传
- [x] PR CI + 归档：PR CI 15 pass / 13 skipping（首轮 bench-regression 红 → `_z42bSupportsOutRoot` 兜底后全绿）