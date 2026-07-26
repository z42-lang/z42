# 按改动类型的验证速查

> 本地开发时"我改了 X，该怎么验"的查询入口。机制原理见
> [bootstrap.md](bootstrap.md)（自举模型）与 book [测试门禁](../../book/src/dev/test-gate.md)；
> 本页只回答"跑什么、什么顺序、CI 还会替你验什么"。
> 通用前提：**commit 前必须完整 `xtask test` 全绿**——下表的"快速迭代"列不构成 GREEN。

## 速查表

| 改动 | 快速迭代（改一点验一点） | commit 前额外必跑 | CI 替你验的（模块门控 job） |
|------|------------------------|------------------|------------|
| **编译器 `src/compiler/`** | `xtask test changed`（→ compiler + vm） | 触及 lexer/parser/codegen/格式 writer，或 z42c 源用了新写法 → `xtask test bootstrap` | `compiler-checks`(自举不动点+vscode)、`verify-selfhost`、`test-host`(e2e goldens+cross-zpkg@linux) |
| **标准库 `src/libraries/`（加 API / 改实现）** | `xtask test changed`（→ lib <lib> + vm）或 `xtask test stdlib <lib> --filter=K` | — | `stdlib-interp`(3 OS)+`stdlib-jit`(2 shard)、`test-host`(e2e goldens+cross-zpkg@linux) |
| **标准库（删/改 xtask 或 z42c 在用的 API）** | 同上 + 迁移调用点 | ⚠️ **两步舞**：nightly N 先加新 API（旧暂留）→ N 发布后切调用点+删旧。**完整操作剧本见下**"stdlib 破坏性 API 变更" | ci-bootstrap step 2/3（用**种子** stdlib 编 xtask/z42c 源） |
| **VM `src/runtime/`（Rust）** | `cargo test --manifest-path src/runtime/Cargo.toml` + `xtask test e2e` | — | `test-host`(e2e goldens+cross-zpkg@linux)、`vm-jit`(2 shard)、`stdlib-interp`+`stdlib-jit`、`verify-features` |
| **仅测试用例 `src/tests/`** | `xtask test e2e` | — | `test-host`(e2e goldens+cross-zpkg@linux)。⚠️ `vm-jit`/`stdlib-*` 不跑（测试用例不影响 JIT 行为/stdlib 单元） |
| **xtask 源 `scripts/`** | `z42 publish scripts/xtask.z42.toml` 重建 → 随便跑条命令冒烟 | changed 映射对 `scripts/xtask*` = **full**（完整 gate） | ci-bootstrap step 2（种子编 xtask 源） |
| **新语法 / zbc·zpkg 格式** | 阶段一只落 support（仓库源码不用）→ `xtask test bootstrap` | 格式 bump 另跑 [version-bumping checklist](../../../.claude/rules/version-bumping.md)；等 nightly 发布后才 use | `verify-selfhost` + 全腿 bootstrap；发布死锁自愈见 [ci.md 阶段⑥](../ci.md) |
| **打包 `scripts/package/` / `packages.toml`** | `xtask test packages` | `xtask package sdk` + `xtask test dist` | `package-host` + `package-{ios,android,wasm}` |
| **增量编译（IncrementalBuild / CacheStore / ZbcReader·Instr / IncrementalDriver）** | `xtask test compiler`（含 probe/闭包/meta/往返单测 + 不动点） | `xtask test incremental`（暴力对账器：语料逐文件 touch，增量 == 全量逐字节 + D8 计时） | `compiler-checks`（自举不动点 7/7） |
| **纯文档 / `.claude/`** | 无 | 无（不改代码 → 无 stage 可跑） | 不触发 CI（paths-ignore） |

## 边界为什么管 API（不只是语法）

CI 每条腿冷启动时，**xtask 源和 z42c 源都由"上一 nightly 的种子 z42c + 种子 stdlib"编译**
（`.github/actions/ci-bootstrap` step 2/3）。因此这两个源码域被种子钉死了**两根轴**：

1. **语法/格式轴**：不得用比上一 nightly z42c 更新的语法（[bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md) 的 support-先行纪律）
2. **stdlib API 轴**：不得引用上一 nightly stdlib 里不存在的 API——删改 xtask/z42c 在用的
   API 与用新语法是同一种断链，同样要"晚一个 nightly 再 use"

stdlib 源自身不受种子约束（它由自建的当前 z42c 编译）。

## stdlib 改动的验证流程

### 情形一：加 API / 改内部实现（不破坏边界，最常见）

```bash
xtask test stdlib <lib> --filter=<关键字>   # 迭代：只跑该库相关用例
xtask test changed                          # 或按 diff 自动挑（lib <lib> + vm）
xtask test                                  # commit 前完整 gate
```

不需要任何边界动作——种子编 xtask/z42c 用的是**种子自带的旧 stdlib**，你改当前 stdlib
不影响冷启动。新 API 想被 xtask/z42c 源使用？**等它随 nightly 发布之后**（见情形二的原理）。

### 情形二：删/改 xtask 或 z42c 在用的 API（破坏边界 → 两个 nightly 的提交剧本）

**第 0 步 · 判定是否踩边界**——查改动的 API 是否被两个种子约束域引用：

```bash
grep -rn "GetSize" scripts/ src/compiler/     # 以改名 File.GetSize → File.Size 为例
```

- 无命中 → 不踩边界，走情形一
- 有命中 → 按下面两阶段执行，**两阶段之间必须隔一次成功的 publish-nightly**

---

**阶段一（commit A）：加新、留旧、调用点不动**

1. stdlib 加新 API `File.Size`；**旧 `File.GetSize` 原样保留**（可与新实现同体）。
   这是"不留兼容"规则的**种子例外**：旧 API 只为跨一个 nightly 而暂留，阶段二必删。
2. `scripts/` 与 `src/compiler/` 的调用点**一律不改**（仍用旧 API）。
3. 验证 + 提交：

```bash
xtask test                    # 完整 gate 全绿
git commit -m "feat(stdlib): File.Size 落地（GetSize 暂留一个 nightly，种子例外）"
git push origin main
```

4. **硬等待点：确认新 nightly 已发布**（它把新 API 带进种子）：

```bash
gh run list --workflow=CI --branch=main -L 1        # 等本次 CI 全绿
gh release view nightly --json publishedAt          # publishedAt 晚于 commit A 的 CI 完成时间
```

publish-nightly 只在 main push 且测试过后运行；红了就修，**不得在 nightly 滚过去之前开始阶段二**。

---

**阶段二（commit B）：切调用点 + 删旧 API（同一原子提交）**

1. `scripts/` 与 `src/compiler/` 全部调用点切到 `File.Size`；**同一提交删除 `File.GetSize`**
   （种子例外到此结束，不留长期兼容）。
2. 验证 + 提交：

```bash
xtask test bootstrap          # (A) 绿 = 新 nightly 种子已含 File.Size，z42c 源切换安全
                              #   ⚠️ 它暂不编 xtask 源（已知缺口①）——xtask 侧越界目前只能靠 CI 兜底
xtask test                    # 完整 gate 全绿
git commit -m "refactor(stdlib): 调用点切换 File.Size 并删除 GetSize"
git push origin main
```

3. CI 每腿 ci-bootstrap 用新种子编 xtask/z42c 源（已含新 API）→ 绿。

**踩线的症状与自救**：阶段二在 nightly 发布前 push → 所有腿在 bootstrap step 2/3 红
（种子 stdlib 没有 `File.Size`），publish-nightly 因 needs 不满足而不发布——种子链不被污染。
处置：revert commit B，等 nightly 滚过去再重新 push；**不要**试图往种子里手补。

**附加纪律**：不要与 zbc/zpkg 格式 bump 排在同一个 nightly 周期（双重断链窗口叠加，
见 [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)）。

## 验证覆盖矩阵（谁在守哪个格子）

| 源码域 × 工具链 | 种子（上一 nightly） | 当前（本仓） |
|--------|------|------|
| z42c 源 | `test bootstrap` (A)；CI 每腿 + `verify-selfhost` | gate compiler stage 不动点；`test bootstrap` (B) |
| stdlib 源 | —（不需要） | gate regen 波 `build stdlib` |
| xtask 源 | ⚠️ 仅 CI（本地无手段） | ❌ 无覆盖 |

> **已知缺口**（2026-07-02 识别，待立项）：① `test bootstrap` 不编 xtask 源——本地无法提前
> 发现 xtask 越界；② "当前工具链编 xtask 源"处处不验——z42c 变严格 / stdlib 删 API 的破坏
> 会延迟到下一 nightly 变种子后才在 CI 引爆。修复方向：`test bootstrap` 与 gate 各补一编。
