# workflow/testing/

z42 各层级测试的运行命令。**测试设计**（attribute 体系、TIDX section、runner 协议）见 [`docs/design/testing/`](../../design/testing/)。

> **我改了 X，该怎么验？** → [`verify-by-change.md`](verify-by-change.md)（按改动类型的速查表：
> 快速迭代 / commit 前必跑 / CI 替你验什么 / 自举边界注意点）。

## 测试层级

z42 测试分四层，各层独立运行：

| 层 | 跑什么 | 命令 |
|---|---|---|
| **编译器单测** | z42c 自举不动点 + `[Test]` units — lexer / parser / type-check / IR-gen | [`unit-tests.md`](unit-tests.md) |
| **VM golden** | `src/tests/**/source.z42` 端到端（interp + JIT 双模） | [`vm-tests.md`](vm-tests.md) |
| **stdlib `[Test]`** | `src/libraries/<lib>/tests/*.z42` 经 z42-test-runner | [`stdlib-tests.md`](stdlib-tests.md) |
| **cross-zpkg** | 多 zpkg 协作（target lib + ext lib + main app） | [`cross-zpkg.md`](cross-zpkg.md) |

## 增量测试

[`changed-only.md`](changed-only.md) — `./xtask test changed` 根据 `git diff` 只跑受影响的测试命令集合（dev 内循环加速）。

## 平台测试（wasm / iOS / Android）

[`platform-tests.md`](platform-tests.md) — `./xtask test platform <p> [build|assets|run]`：嵌入 R1–R7 契约在浏览器 / iOS Simulator / Android emulator 上跑。**不在**主 GREEN gate 内（各需重型工具链，按需单跑）；CI 各平台独立 job。含从零的本地配方（含 apphost 构建）。

## GREEN 门禁

CI 全绿门禁（`cargo build`（z42vm）+ `./xtask test`（内部串联 vm / cross-zpkg / lib / compiler））的定义见 [`../ci.md`](../ci.md)；规则在 [`.claude/rules/workflow.md`](../../../.claude/rules/workflow.md) 阶段 8。

## 一键全跑

```bash
./xtask test    # 全部 4 层
```

## Scope-aware test-all（add-test-split-by-area, 2026-05-21）

`./xtask test` 默认跑 5 stages（cargo build (z42vm) / test e2e /
test e2e --dir cross-zpkg / test lib / test compiler）≈ 3-5 min。iteration 期常
只改一个 area；用 `--scope=<value>` 跳过不相关 stages：

| Scope | 跑的 stages | 何时用 |
|-------|------------|--------|
| `full`（默认）| 全 5 | commit 前最终 GREEN，PR gate |
| `runtime` | cargo build + test-vm + cross-zpkg + stdlib | 改 `src/runtime/**` |
| `compiler` | test compiler + test-vm + cross-zpkg + stdlib | 改 `src/compiler/**` |
| `stdlib` | test-vm + cross-zpkg + stdlib | 改 `src/libraries/**` / `examples/**` |
| `docs-only` | 0 stages（clean exit） | 仅 `docs/**` / `.claude/**` |
| `auto` | 由 `git diff --name-only HEAD` 推断 | 让脚本判断 |

`auto` 路径分类：

- `src/compiler/**` → `compiler`
- `src/runtime/**` + `src/tests/**` → `runtime`
- `src/libraries/**` + `examples/**` → `stdlib`
- `docs/**` + `.claude/**` → 不升级（默认 `docs-only`）
- 其他（Cargo.toml / .github / 顶层 / scripts/）→ `full`（保守）
- compiler + runtime 都改 → `full`

例：

```bash
./xtask test --scope=stdlib    # 跳 compiler stage，约省 30-40s
./xtask test --scope=runtime   # 跳 compiler stage，约省 60-90s
./xtask test --scope=auto      # 自动判断当前 diff
./xtask test --scope=full      # full，commit 前必走
```

**GREEN gate 规则**：iteration 期允许缩窄 scope 加速。**commit 前最终
GREEN 必须 `--scope=full`**（或 `--scope=auto` 自动等价 full，
即未缩窄）。Partial scope 不算 GREEN，不可作为 commit 通过依据。
详见 [`.claude/rules/workflow.md`](../../../.claude/rules/workflow.md) 阶段 8。

### Parallel waves（add-test-parallel-stages, 2026-05-21）

正交于 `--scope`。加 `--parallel` 让 `./xtask test` 按依赖图分 wave 跑，每个
wave 内的 stage 并发；wave 之间串行。期望加速 ~38%（scope=full）。

各 scope 的 wave 排列：

| Scope | Wave 1 | Wave 2 | Wave 3 |
|-------|--------|--------|--------|
| full | cargo build (z42vm) | test compiler \|\| test-stdlib | test-vm `--no-rebuild` \|\| cross-zpkg |
| runtime | cargo build | test-stdlib | test-vm `--no-rebuild` \|\| cross-zpkg |
| compiler | test compiler | test compiler \|\| test-stdlib | test-vm `--no-rebuild` \|\| cross-zpkg |
| stdlib | test-stdlib | test-vm `--no-rebuild` \|\| cross-zpkg | — |
| docs-only | — | — | — |

**关键安全约束**：`--parallel` 强制 test-vm 走 `--no-rebuild`。Default
test-vm 在启动时重建 stdlib（写 `artifacts/build/libraries/dist/release/`）；与 W2 的
test-stdlib 时间窗口重合 → race。Wave 3 时 W2 已完成 stdlib build，
test-vm 直接读现有 zpkgs 即可。

输出处理：每个 wave 内并发 stage 的 stdout/stderr 各自 capture 到 temp
文件；`wait` 完后按原 stage 顺序串行 print。transcript 与 sequential
模式视觉一致，无 interleaving。

**失败时保留 temp 文件**：wave 内任一 stage 失败，wave 结束后保留所有
stage 的 temp 输出文件，echo 完整路径供 debug。成功 wave 自动清理。

例：

```bash
./xtask test --parallel                       # full + parallel ≈ 160s
./xtask test --parallel                       # runtime + parallel
./xtask test --parallel                       # auto + parallel
```

`--parallel` + `--quick`、`--parallel` + `--with-dist` 都兼容（额外 stage
进入相应 wave 或单独 wave）。

**GREEN gate 不强制 `--parallel`**：commit 前可用 sequential `--scope=full`
（最保守）或 `--scope=full --parallel`（更快）。两者都算 full coverage GREEN。
