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

## CI 模块门控

CI 按模块变更检测（`dorny/paths-filter`）有条件地跳过无关 job，减少冗余运行。
`schedule`/`workflow_dispatch` 事件始终跑全量（nightly 完整覆盖）。

| CI job | 门控条件（任一为 true 触发） | 主要 stage |
|--------|---------------------------|-----------|
| `test-host`(×4 OS) | 始终运行 | e2e goldens（interp）+ cross-zpkg（仅 linux-x64） |
| `compiler-checks`(linux) | compiler 改动 | z42c 自举不动点 + vscode-syntax |
| `vm-jit`(×2 shard) | vm 改动 | JIT 模式 goldens |
| `stdlib-interp`(×3 OS) | vm \|\| stdlib 改动 | stdlib `[Test]` interp 模式（不分片） |
| `stdlib-jit`(×2 shard) | vm \|\| stdlib 改动 | stdlib `[Test]` JIT 模式 |
| `verify-features` | vm 改动 | feature-matrix 编译检查 |

本地 `xtask test` 始终跑全量 gate；CI 门控仅影响远端 job 调度。
按改动类型的完整验证速查见 [`verify-by-change.md`](verify-by-change.md)。

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

## 测试文件归属（放哪 / 加新用例往哪放）

「被测对象在哪，测试就在哪」+「中央 VM e2e 按特性分类」（对标 dotnet/runtime）。

| 目录 | 形态 | 谁运行 |
|------|------|-------|
| `src/tests/<category>/<name>/` | `.z42` + `expected_output.txt`（golden） | `xtask test e2e`（interp+jit）|
| `src/tests/cross-zpkg/<name>/` | target/ext/main 多 toml 工程 | `xtask test e2e --dir cross-zpkg` |
| `src/tests/{zbc,zpkg}-format/<name>/` | check-in 的 `source.zbc`/`.zpkg` 字节基线 | `cargo test zbc_compat`/`lazy_loader`（`git diff` = 格式漂移）|
| `src/compiler/z42c.<member>/tests/` | 按特性分（lexer/parser/decl/stmt/dump…）| `xtask test compiler`（z42b 跑 `[Test]` + 不动点）|
| `src/libraries/<lib>/tests/` | `<name>/source.z42`（golden）+ 顶层 `*.z42`（`[Test]`）| golden→`test e2e`；`[Test]`→`test stdlib`（z42b）|
| `src/runtime/src/<mod>_tests.rs` | Rust 单元测试 | `xtask test runtime`（`cargo test`）|
| `src/runtime/tests/*.rs` | Rust 集成测试（跨语言契约 / native e2e） | `xtask test runtime` |

**加新用例的归属（先到先得）**：

1. **库 API 行为** → `src/libraries/<lib>/tests/`（与库源码同居）
2. **编译器 pipeline 单元**（lexer/parser/typecheck/IR-gen） → `src/compiler/z42c.<member>/tests/`
3. **VM 内部**（GC/interp/decoder） → `src/runtime/src/<mod>_tests.rs`；**跨语言契约 / native e2e** → `src/runtime/tests/*.rs`
4. **跨多个 zpkg** → `src/tests/cross-zpkg/<name>/`
5. **其他 VM / 语言特性 e2e** → `src/tests/<category>/<name>/`（不确定类别先归 `basic/`）

**用例文件约定**（golden 类）：

| 文件 | 何时 | 含义 |
|------|------|------|
| `source.z42` | 必须 | z42 源码 |
| `source.zbc` | 由 `xtask build test` 生成 | **不与 `source.z42` 同处**——镜像到 `artifacts/build/<...>`（gitignored）；唯一例外 `{zbc,zpkg}-format/*` 是 check-in 基线，就地重写 |
| `expected_output.txt` | run 用例 | stdout 期望；**空 / 缺失 = 靠内置 `Assert.*` 自验**（跑通无输出即过）|
| `interp_only` | 可选 marker | JIT 模式跳过该用例 |
| `features.toml` | 可选 | LanguageFeatures override |

> 测试**设计**（attribute 体系 / TIDX section / z42b runner 协议）见 [`docs/design/testing/`](../../design/testing/)（迁移中；`testing.md` 已冻结）。
