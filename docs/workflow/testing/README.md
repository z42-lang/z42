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
| **stdlib `[Test]`** | `src/libraries/<lib>/tests/*.z42` 经 z42b | [`stdlib-tests.md`](stdlib-tests.md) |
| **cross-zpkg** | 多 zpkg 协作（target lib + ext lib + main app） | [`cross-zpkg.md`](cross-zpkg.md) |

## 增量测试

[`changed-only.md`](changed-only.md) — `./xtask test changed` 根据 `git diff` 只跑受影响的测试命令集合（dev 内循环加速）。

## 平台测试（wasm / iOS / Android）

[`platform-tests.md`](platform-tests.md) — `./xtask test platform <p> [build|assets|run]`：嵌入 R1–R7 契约在浏览器 / iOS Simulator / Android emulator 上跑。**不在**主 GREEN gate 内（各需重型工具链，按需单跑）；CI 各平台独立 job。含从零的本地配方（含 apphost 构建）。

## GREEN 门禁

CI 全绿门禁（`cargo build`（z42vm）+ `./xtask test`（内部串联 e2e / cross-zpkg / stdlib / compiler））的定义见 [`../ci.md`](../ci.md)；规则在 [`.claude/rules/workflow.md`](../../../.claude/rules/workflow.md) 阶段 8。

## 一键全跑

```bash
./xtask test    # 全部 4 层
```

## iteration 期加速手段

`./xtask test` 默认跑全部 stage（cargo build (z42vm) / test e2e /
test e2e --dir cross-zpkg / test stdlib / test compiler）≈ 3-5 min，是 commit 前的
完整 GREEN gate。iteration 期常只改一个 area，现行的缩窄手段有三种（都**不构成
GREEN**，commit 前必须跑完整 `xtask test`）：

| 手段 | 命令 | 作用 |
|------|------|------|
| 按改动挑 stage | `./xtask test changed [base]` | 按 `git diff` 逐文件映射为命令并集，只跑受影响的 stage（`--dry-run` 只打印计划）；见 [`changed-only.md`](changed-only.md) |
| 单跑某 stage | `./xtask test e2e --dir <cat>` / `--file <p>` / `test stdlib <lib>` | 只跑一个类别 / 单库 |
| 跳过重建波 | `./xtask test … --no-build`（或 `--no-rebuild`）| 消费已建产物，反复迭代同一测试时不重编 |

> **注**：早期 C# 版 xtask 曾有 `--scope=full|runtime|compiler|stdlib|auto` 与 `--parallel`
> wave 机制；z42 版 xtask **尚未实现**这两者（源码 `scripts/test/xtask_test.z42` 注明是
> "a later increment"）。当前只有上表三种缩窄手段——不要写 `--scope` / `--parallel`（会报未知 flag）。
