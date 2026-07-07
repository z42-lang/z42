# Tasks: 重构 xtask test 分类

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：refactor（命令面重构 + 补 runtime 覆盖）
> 占用子系统：`toolchain`（ACTIVE.md 已登记）

> **GREEN**：本地 `xtask test`（e2e + stdlib + compiler，无 runtime）✅ 全绿 + 自举不动点 7/7；
> 命令面/`--dir`/`--file`/cross-zpkg 路由 smoke 全过；文件移动后 build test/package resolve OK。
> **CI 才能验的**（沙箱/Windows 限制，push 后盯）：Windows 恢复的 build test+test runtime 步、
> 全腿 `test runtime`、signal_handler_e2e 本身。

## 进度概览
- [x] 阶段 1: `test e2e`（合 vm + cross-zpkg + --dir/--file）— smoke 验证 --dir/--file/cross-zpkg 路由
- [x] 阶段 2: `test runtime`（cargo test --test-threads=1）+ 进 gate
- [x] 阶段 3: build test + regen 合并（删 regen 命令，保留 `_regenCore`/`_regenGolden`）
- [x] 阶段 4: test changed 映射 + CI 迁移（删 Windows cargo-test+regen 步；vm/cross→e2e；regen→build test）
- [x] 阶段 5: 验证（smoke 全过；完整 GREEN gate 运行中）
- [x] 阶段 6: 文档同步（README/build/test-gate/vm-tests/cross-zpkg/verify-by-change/ci/version-bumping/rules 等）

> **实施发现（z42c bug）**：overloaded **free function**（同名不同 arity 的自由函数）crash
> z42c 的 TSIG 导出（`ExportedTypeExtractor._extractFunc` FieldGet null）。workaround：用
> distinct 名（`_enumerateCasesF`/`_runVmGoldensF`）。root-cause 另立 issue（方法重载 OK，
> 自由函数重载的导出路径未覆盖）。

## 阶段 1: test e2e
- [ ] 1.1 `xtask_test.z42`：`_testVm`/`_testVmCore` → `_testE2e`/`_testE2eCore`；加 `--dir`/`--file` 过滤 + cross-zpkg 分派（调 `_testCrossZpkgCore`）
- [ ] 1.2 `xtask_cli.z42`：router `vm`+`cross-zpkg` → `e2e`（选项 --dir/--file/--mode/--jobs/--shard/--no-build/--no-rebuild）+ dispatch
- [ ] 1.3 默认（无 --dir/--file）跑全部；cross-zpkg 类走多包 runner

## 阶段 2: test runtime
- [x] 2.1 `xtask_test.z42`：`_testRuntime`（cargo test --locked --test-threads=1 + RUST_MIN_STACK）
- [x] 2.2 `xtask_cli.z42`：router `runtime` + dispatch
- [x] 2.3 **修订**：`test runtime` **不进** `_testAll`（signal_handler_e2e crash-helper 在信号受限沙箱挂死 → 会卡死 always-run gate）；改每条 CI test-host 腿单独一步 + standalone 命令 + `test changed` 映射。见 design Decision 3。

## 阶段 3: build test + regen 合并（+ 删 audit）
- [x] 3.1 `xtask.z42`：删 `_regen` wrapper（保留 `_regenCore`/`_regenForTest`/`_regenGolden`）
- [x] 3.2 `xtask_cli.z42`：删顶层 `regen` router + dispatch
- [x] 3.3 `build test`（`_buildTest`）确认为唯一 golden 资产命令
- [x] 3.4 删 `audit`：删 `xtask_audit.z42` + router + dispatch + xtask.z42 注释 + 文档（README/namespace-using.md）

## 阶段 4: test changed 映射 + CI
- [ ] 4.1 `xtask_test_changed.z42`：src/tests→e2e / cross-zpkg→e2e --dir cross-zpkg / src/runtime→test runtime
- [ ] 4.2 `ci.yml`：Windows cargo-test step（决 删/改 `test runtime`——依 Open Question）；`test vm jit`→`test e2e --mode jit`；`regen --no-stdlib`×2→`build test`
- [ ] 4.3 CI Open Question（runtime 进全腿 gate vs 单腿）→ User 拍板后落地

## 阶段 5: 验证
- [ ] 5.1 smoke：runtime / e2e（全 + --dir + --file + --mode jit）/ build test
- [ ] 5.2 旧命令消失（vm/cross-zpkg/regen → 2）
- [ ] 5.3 `test changed --dry-run` 映射验证
- [ ] 5.4 `xtask test` 完整 GREEN（含 runtime + e2e）

## 阶段 6: 文档同步
- [ ] 6.1 scripts/README.md（命令树/流程图）
- [ ] 6.2 docs/book/src/dev/{xtask,build}.md
- [ ] 6.3 docs/workflow/testing/{vm-tests,verify-by-change,README}.md + ci.md
- [ ] 6.4 docs/design/testing/testing.md + runtime/zbc.md + compiler-architecture.md
- [ ] 6.5 .claude/rules/version-bumping.md（regen→build stdlib && build test）

## 备注
- cargo test race 用 --test-threads=1 stopgap，root-cause 另立 issue（ci.yml TODO）。
