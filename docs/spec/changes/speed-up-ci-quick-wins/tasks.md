# Tasks: CI 第一批提速（快速改动）

**状态：🟡 进行中（等 PR CI 实测）| 开始：2026-09-30**

类型：`refactor`（CI 编排，不改构建/测试行为）→ 最小化模式。

依据：对最近 5 次 CI run 的 job / step 实测耗时（2026-09-28/29）。编译器类 PR 墙钟约 16.7 min、
19 个 job；最长的 test-host(linux-x64) 16.5 min。

## 进度概览

- [x] 阶段 1: ci.yml / composite action 改动
- [x] 阶段 2: 文档同步
- [x] 阶段 3: 本地可验的门（actionlint / `test ci-shell` / `test docs`）
- [ ] 阶段 4: PR CI 实测 + 分支保护调整（移除 `verify-selfhost(linux-x64)` 这个 required check）
- [ ] 阶段 5: 归档

---

## 阶段 1: CI 改动

- [x] **test-host 独立 rust-cache key**（`host-v2` → `test-host-v1`）：compile-toolchain 只编 release
      且先写入 `host-v2`，test-host 命中后不回存 ⇒ debug / test profile 依赖每轮冷编。
- [x] **compile-toolchain 只留 linux 腿**：产物是纯 zpkg、与宿主无关（Windows 一直吃 linux 那份）；
      `xtask-bootstrap-artifact` 一律下载 `toolchain-ubuntu-latest`。去掉一个 macOS job，
      且 linux 下游不再陪等 matrix 的 macOS 腿（实测每个下游 0.5~2.6 min）。
- [x] **compile-test-assets 只留 linux 腿**；zbc-format 字节基线门挪到 test-host 三条非 Windows 腿末尾
      （零额外构建、三个架构、required check）。组装 / 上传不再 continue-on-error（失败就地报）。
- [x] **删 verify-selfhost**：= `ci-bootstrap`（test-host ×4 + compile-toolchain 已跑）+ `test compiler`
      （compiler-checks 已跑）；独有的 no-dotnet 桩守的是 2026-06-26 已删的 C# 路径。
- [x] **test-consume 删 JIT 1/4 探针**（continue-on-error、从不挡人；test-vm-jit 全覆盖）。
- [x] **verify-features 改 `cargo check --release`**；顺带修它的 rust-cache：`workspaces` 写的是裸
      `src/runtime`，而根 `.cargo/config.toml` 把 target-dir 重定向到 `artifacts/build/runtime`
      ⇒ 此前缓存的是空目录。key → `feature-matrix-v2`。
- [x] **路径过滤**：
  - [x] 新 `examples` flag（`examples/**`、`docs/learn/**`）只门控 package-host；从 `platform` 移出
        ⇒ learn PR 不再拉起 package-{ios,android,wasm} + test-desktop。
  - [x] `stdlib` 补 `src/toolchain/builder/**`（z42b 是 [Test] 执行器）+ `scripts/test/xtask_test_lib*.z42`
        —— 覆盖缺口：此前只改它们的 PR 一个 stdlib [Test] job 都不跑。
  - [x] `vm` 补 `.cargo/**`。
  - [x] 删冗余条目（`workload/fixtures/**`、`z42c.core/**`、`z42c.syntax/**` 已被上层通配覆盖）。

### 评估后不做（记录理由）

- **非 linux-x64 腿跳 `gcgen`**：GC 测试在 arm64 弱内存模型上有独立价值，而这两条腿不在关键路径上，省下的不改变墙钟。
- **vm PR 跳 `rust-units`**：该 stage 放在 build wave 之后最前，是 2026-09-26 为「Rust 坏了第一时间红」
  刻意做的；`test runtime` 虽是超集但排在 18 min 之后。
- **bench-pr 不存 rust-cache**：它对同一 PR 的后续 push 有真实收益（~120s）。占 5 GB、挤掉其他缓存的问题
  要靠「main 上预热一份可被 PR 读取的 key」解决，key 对齐需要单独验证 → 下一批。

## 阶段 2: 文档同步

- [x] `docs/internals/src/devinfra/ci.md`：流水线图、flag 表、job 表、编排理由、新增 §3.0.1 Rust 缓存 key；
      顺带修正过时描述（compiler filter 的 `src/libraries/z42c.*` 路径、package-host 实为 4 OS）。
- [x] `docs/agent/rules/version-bumping.md`：基线门位置、`toolchain-ubuntu-latest` 唯一 artifact 名。
- [x] `docs/agent/rules/bootstrap-seed.md`、`docs/internals/src/devinfra/{release,testing}.md`、
      `src/tests/zbc-format/README.md`、`scripts/build/xtask_bootstrap_check.z42`（注释）：去掉 verify-selfhost 引用。

## 阶段 3: 本地验证

- [x] `actionlint`：无新增问题（唯一 error `ubuntu-26.04` 未知 label 为 main 既有）
- [x] `xtask test ci-shell`：7 个 yml / 73 个 run 块，无先用后赋
- [x] `xtask test docs`：无新增死链

## 阶段 4: PR CI 实测

- [ ] 全绿（重点看：test-host 三腿的基线门、macOS 消费方用 linux toolchain 是否正常、verify-features check）
- [ ] 与基线对比墙钟 / job 数
- [ ] 分支保护移除 `verify-selfhost(linux-x64)`（**合并前必须**，否则 required check 永远 pending）
