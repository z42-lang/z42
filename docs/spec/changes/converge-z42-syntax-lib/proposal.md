# Proposal: 收敛 z42c 可移植前端（core + syntax）进共享库层

> 轴 2「编译器/REPL 库进 stdlib + tier 拆分」的 **PR-A（route A 地基）**。前身 PR1 = 拆
> z42.repl（#314）。程序总纲见记忆 `stdlib-interop-and-repl-split-program`。

## Why

终态目标（route A）：`z42.scripting` 变 **stdlib/共享-build-only**，可搬进 `src/libraries/`，
让 REPL/playground 等运行时工具不必静态链整个编译器。为此 scripting 的每个依赖都必须落在共享
build 里可被 `WorkspaceBuild.DiscoverMembers` 发现。

scripting 现依赖两类编译器件：
- **词法/语法前端**（`z42c.core` 的 Span + `z42c.syntax` 的 Lexer/Token/Parser/AST）——被
  Classifier / Completeness / Rewriter / Completer 直接消费。
- **语义/编译后端**（`z42c.semantics` 的 IrDump.ParseAll、`z42c.pipeline` 的 PackageCompile）——
  被 Script/Engine 的全量编译路径消费。

后端走后续 PR 的 **z42.build ICompiler 门面 + 运行期注入**（route A 核心），本 PR **只解决前端**：
把 `z42c.core` + `z42c.syntax` 从编译器 workspace 挪进共享库层，成为「**恰好可被 runtime/工具/
playground 加载的可移植 `z42c.*` 库**」。这是门面 PR 与 scripting 搬迁的**地基**。

**为什么前端天然适合共享**：Lexer/Parser/AST/Token/Span/Diagnostic 全是 **host-platform-independent
的纯计算**（零 syscall / 零 native），连 wasm/playground（无宿主平台）都能直接加载——这正是
「按宿主平台依赖性切分」原则下最该共享的一层。

## What Changes（本 PR 的最小必要动作）

1. **物理搬迁**：`src/compiler/z42c.core` + `src/compiler/z42c.syntax` → `src/libraries/z42c.core`
   + `src/libraries/z42c.syntax`（含各自 `tests/`）。
2. **构建接线**：
   - `src/compiler/z42.workspace.toml`：`default-members` 去掉 `z42c.core` / `z42c.syntax`（+ 更新拓扑注释）。
   - `src/libraries/z42.workspace.toml`：`default-members` 加 `z42c.core` / `z42c.syntax`（`members=["*"]`
     自动发现子目录，`default-members` 补显式序：core 无依赖在前、syntax 依赖 core 次之）。
   - `z42c.semantics` / `z42c.pipeline` / `z42c.driver`（编译器 workspace）经**跨 workspace dist 发现**
     解析 `z42c.core` / `z42c.syntax`——与它们现在解析 stdlib `z42.ir` / `z42.project` **同机制**。
3. **破 bootstrap 轴④环**：`scripts/build/xtask_compiler.z42` 的 `_ensureBootstrapSelfDepLibs`（或旁加
   `_ensureBootstrapZ42cFrontend`）在建 z42c **前**，用种子 driver 把**当前源** `z42c.core` + `z42c.syntax`
   预建进 build-libs——与 z42.ir 破环**同款**（z42c 自依赖的共享库必须先于消费者进 flat）。
4. **文档**：`docs/design/compiler/self-hosting.md`（轴④预建列表 + 布局图）、`compiler-architecture.md`、
   相关 README。

## 明确不改（Out of Scope / 减少标准库）

- **不改包名**：仍 `z42c.core` / `z42c.syntax`。
- **不改命名空间**：仍 `Z42.Core` / `Z42.Syntax`（`Z42c.*` 改名以后需要再单独做）。
  → **消费方所有 `using Z42.Core` / `using Z42.Syntax`、所有 FQ 引用、所有 toml 依赖名全部不动**，
    本 PR 是**纯物理搬迁 + 构建接线**，无源码符号改动。
- **不搬** `z42c.semantics` / `z42c.pipeline`（留编译器，走后续门面 PR）。
- **不动** `z42.ir`（保 `Z42.IR` / `Z42.Project` 真 stdlib 身份）。
- **零新增 `Std` / `z42.*` API 面**——搬入者全 `z42c.*` 身份，用户面标准库**不增长**（满足「尽量减少标准库」）。
- **零格式 bump**（zbc / zpkg writer 不动）。
- scripting 本轮**仍留 toolchain**（其 semantics/pipeline 依赖未解，未到可搬 libraries 的条件）。

## 前置依赖 / 风险

- **bootstrap 轴④**：z42c 运行期/构建期自依赖 `z42c.core` + `z42c.syntax`，冷启动 flat dist 里没有
  → 必靠 §破环预建。**本地不可验**（种子墙：种子 driver 缺近期字段；z42vm 退出期挂起）→
  **GREEN 判定以 CI 为准**（`ci-bootstrap` 两代自举 + `verify-selfhost` 字节不动点 + test-host×4 + jit）。
- **跨 workspace 短类名 first-wins 碰撞**：`z42.project`(Z42.Build.Project) 与 `z42c.project` 曾因 flat
  `Z42_LIBS` 短类名 first-wins 串味炸过自举，已由 `fix-crosspkg-static-ns-collision`（using-scoped 解析）
  根治。本 PR 不改命名空间，`Z42.Core` / `Z42.Syntax` 仍单包独占（无第二包同 ns），无新碰撞面。
- **default-members 双改**：漏改任一 workspace 的 default-members → 该包不建或重复建。搬迁半径清单见 tasks.md。

## Scope（允许改动的文件）

- `src/compiler/z42c.core/**` → `src/libraries/z42c.core/**`（git mv）
- `src/compiler/z42c.syntax/**` → `src/libraries/z42c.syntax/**`（git mv）
- `src/compiler/z42.workspace.toml`（default-members 去二）
- `src/libraries/z42.workspace.toml`（default-members 加二）
- `scripts/build/xtask_compiler.z42`（破环预建扩至 z42c.core + z42c.syntax）
- `docs/design/compiler/self-hosting.md`、`docs/design/compiler/compiler-architecture.md`、相关 README
- `docs/spec/changes/converge-z42-syntax-lib/**`（本提案 + tasks）

## Open Questions

1. 物理落点：`src/libraries/z42c.*`（z42.ir 先例，最少机械改动，本提案采用）vs 保留在 `src/compiler/`
   并扩 DiscoverMembers 跨树发现（src/libraries 保持纯 z42.*，但需新构建机械）。**倾向前者**（照搬 z42.ir）。
2. 破环 helper：并入现有 `_ensureBootstrapSelfDepLibs`（改名/扩列表）还是旁加独立 helper？倾向扩现有列表（同一破环阶段）。
