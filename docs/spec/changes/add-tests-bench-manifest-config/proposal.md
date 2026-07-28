# Proposal: 清单声明式 test / bench / example 目标（add-tests-bench-manifest-config）

> 状态：🔴 DRAFT（待 User 6.5 审批）| 创建：2026-07-29 | 类型：`lang/工程模型` + `toolchain` → 完整流程
> 子系统：`stdlib`（z42.project 模型 + ManifestLoader 解析）‖ `toolchain`（xtask 发现/编译/运行/过滤）
> 锁：stdlib + toolchain 两把（**均被占**：stdlib←`converge-z42c-onto-z42-project`，toolchain←`unify-run-modes`(P1)）
>     → **User 授权隔离 worktree 预抢**（2026-07-29），合并时解冲突。
> 前置：`[[exe]]` 多目标模型（`unify-run-modes` P3 已在分支）、Std.Toml、z42b 反射运行层
> 设计 SoT：重写 `docs/design/compiler/project.md` L5b（旧纸面设计 2026-06-06 → 本 change 落地形态）

---

## Why

z42 的 test / bench / example 目前**全靠目录约定扫描发现，清单里零声明**，导致三个缺口：

1. **无法在清单里声明具名运行目标**——`z42.toml` 只有 `[[exe]]`（release 产物），没有 `[[test]]`/`[[bench]]`/`[[example]]`。想跑"某一个"只能靠三套互不统一的路径 filter（golden `--dir/--file`、stdlib `<lib>+--filter`、bench 仅 `--quick`）。
2. **多文件合并成一个运行单元**只有目录约定（`tests/<name>/` 合并），路径不规则时无处声明。
3. **example 完全没有 runner**——[examples/README.md](../../../../examples/README.md) 还写手动 `z42vm`，`xtask test changed` 显式跳过 examples，示例长期无"永远能编能跑"的门禁。

设计文档 L5b（2026-06-06）早已规划 `[tests]`/`[[test]]`，但**从未落地**（[project.md:774](../../../design/compiler/project.md) 有悬空前向引用）。且其旧设计与本次讨论定稿不一致（见 design.md 决策 D1–D4）。本 change 落地 + 重写。

---

## What Changes

- **stdlib（z42.project）**：新增统一目标模型 `RunTarget`（test/bench/example 共用）+ 段配置 `TargetSection`；`ProjectManifest` 加三类段 + 三个目标数组；`ManifestLoader` 加解析器。
- **清单新增段**（全可省 → 走约定）：
  - `[tests]` / `[benches]` / `[examples]`（段名复数，避免与单数 `[[test]]`/`[[bench]]`/`[[example]]` array key 撞 → design D8）：`include`/`exclude` glob（批量发现）+ `auto` 开关 + `[<plural>.dependencies]` dev-deps。
  - `[[test]]` / `[[bench]]` / `[[example]]`：显式具名目标，字段对齐 `[[exe]]`（`name` + `entry` + `sources`）+ `harness` 布尔 + per-target deps。
- **toolchain（xtask）**：test/bench 发现改为「清单段/目标 + 约定 glob 兜底」；统一 `xtask test <name>` / `xtask bench <name>` / **新增 `xtask example [name]`** 具名选择；example 纳入 `xtask test` 作**编译门禁**（默认只编不跑，`test = true` 才跑）。
- **验证语义**：`harness=true` → z42b 反射跑 `[Test]`/`[Benchmark]`，assert 失败即非零；`harness=false` → 跑目标 `entry` 的 Main，**退出码判定**（无 golden/expected 比对）。
- **docs**：重写 project.md L5b；同步 README / testing workflow。

## 不做（明确划走）

- **golden stdout 比对不进本模型**：现有 `src/tests/**`（`source.z42`+`expected_output.txt`）继续作**独立约定扫描 harness**（[xtask_test_vm.z42](../../../../scripts/test/xtask_test_vm.z42) 那条链）存在，不改造、不迁移。本 change 的清单目标一律 exit-code 语义（决策 D3）。
- **compiler（z42c.driver）不改**：test/bench/example 是 dev-only，release 忽略；xtask 用现有「合成 mini-manifest + 调 z42c 子进程」编译，driver 代码零改动。
- **函数级 filter**（`-- <substr>` 选 `[Test]` 内某函数）：沿用 z42b 现状，不在本 change 扩展。
- **workspace 级聚合跑**（一次跑所有 member 的 test）：本 change 只做单工程；workspace 聚合留 future。

---

## Scope（允许改动的文件）

### stdlib（z42.project 模型 + 解析）
| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/libraries/z42.project/src/RunTarget.z42` | NEW | 统一目标模型（`Name`/`Harness`/`HasEntry`/`Entry`/`Sources[]`/`Deps[]`/`RunInTest`）|
| `src/libraries/z42.project/src/TargetSection.z42` | NEW | `[tests]`/`[benches]`/`[examples]` 段（`Include[]`/`Exclude[]`/`Auto`/`Deps[]`）|
| `src/libraries/z42.project/src/ProjectManifest.z42` | MODIFY | 加三段 + `RunTarget[] Tests/Benches/Examples` + counts |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | `_parseTargetSection` + `_parseRunTargets`（仿 `_parseExes`）|
| `src/libraries/z42.project/README.md` | MODIFY | 功能索引 + 核心文件表补三类目标 |
| `src/libraries/z42.project/tests/tests_bench_example_targets.z42` | NEW | 解析单测（段/目标/dev-deps/harness/默认值/错误输入）|

### toolchain（xtask 发现 / 运行 / 过滤）
| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `scripts/xtask_cli.z42` | MODIFY | `test <name>`/`bench <name>` 收敛为具名选择；新增 `example [name]` verb |
| `scripts/test/xtask_test_lib.z42` | MODIFY | 发现从「纯目录约定」→「清单段/目标 + glob 兜底」|
| `scripts/test/xtask_test_lib_units.z42` | MODIFY | 目标发现 + harness 分派 + exit-code 判定 + 确定序 sort |
| `scripts/xtask_bench.z42` | MODIFY | 清单 bench 目标 + `bench <name>` 过滤 |
| `scripts/test/xtask_test_example.z42` | NEW | example runner：`xtask test` 编译全部 + 跑 `test=true`；`xtask example <name>` 跑单个 |
| `scripts/test/xtask_test.z42` | MODIFY | wire example 编译门禁进主 gate |
| `src/toolchain/README.md` / `scripts/README.md`（若存在对应段）| MODIFY | 命令面同步 |

### docs
| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `docs/design/compiler/project.md` | MODIFY | **重写 L5b**（harness/exit-code/example/glob；对齐 `[[exe]]`；错误码更新）|
| `docs/workflow/testing/` 对应页 | MODIFY | `xtask test/bench/example <name>` 命令面 |
| `docs/features.md` / `docs/roadmap.md` | MODIFY | test/bench/example 清单目标能力状态 |

### 测试夹具
| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/libraries/<某库>/tests/` 或 `examples/` 下样例 | NEW/MODIFY | 端到端夹具：含 `[[test]]`/`[[bench]]`/`[[example]]` 的工程，验证发现+运行+过滤 |

**只读引用**（理解上下文，不改）：
- `src/libraries/z42.project/src/ExeTarget.z42` — 目标模型范式
- `src/toolchain/builder/core/builder_test.z42` — z42b 反射运行层（harness=true 落点）
- `scripts/test/xtask_test_vm.z42` — golden 独立 harness（并存，不改）

## Out of Scope
- golden（expected_output）比对机制迁移；compiler/driver 改动；函数级 filter；workspace 聚合跑；单文件源码测试。

## Open Questions
- [ ] example 默认 include glob 取 `examples/*.z42` 还是 `examples/**/*.z42`？（design.md D5 暂定前者 + dir-mode 合并）
- [ ] 端到端夹具放 stdlib 某库 tests 下，还是 `src/tests/` 新类目？（倾向复用 stdlib 库以走 z42b 现成调度）
