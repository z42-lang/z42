# Design: 简化编译器构建

## Architecture

**核心洞察**：z42vm 加载 `<app>.zpkg` 时，解析依赖的搜索目录 = `[<app>.zpkg 所在目录, Z42_LIBS]`
（`src/runtime/src/main.rs:498-509`，entry dir 优先，再 libs）。所以只要 exe 的输出目录里
放着它的依赖 zpkg，就地即可解析 —— 和 .NET `bin/` 一致。

```
现状（拼接目录）：                          目标（自包含 exe）：
compiler/z42c.driver/dist/                  compiler/z42c.driver/dist/
    z42c.driver.zpkg                            z42c.driver.zpkg
compiler/z42c.core/dist/z42c.core.zpkg          z42c.core.zpkg      ┐ 非 stdlib 依赖
compiler/…/dist/…                               z42c.ir.zpkg        │ build 时复制进来
+ selfbuild-runlibs/{stdlib+7包}  ← 删          …                  ┘
+ dogfood/run-/{stdlib+7包}       ← 删
                                            跑：z42vm compiler/z42c.driver/dist/z42c.driver.zpkg
                                                Z42_LIBS=<stdlib dist>
                                                （兄弟包在 entry 目录，stdlib 在 Z42_LIBS）
```

## Decisions

### Decision 1: exe 依赖复制放 build 而非另建目录

**问题**：怎样让 `z42c.driver` 跑起来解析兄弟包，又不引入 scratch 目录？
**选项**：
- A（拼接目录）：现状 —— 复制到 selfbuild-runlibs/dogfood。潜规则、脏。
- B（Z42_LIBS 多目录）：改 runtime `Z42_LIBS` 单→多。要动 Rust。
- C（自包含 exe，本方案）：build 时把非 stdlib 依赖复制进 exe 的 dist。runtime 零改动（entry-dir 搜索已存在）。lib 输出不变。
**决定**：选 **C**。理由：① 运行时已支持 entry-dir 搜索；② 与 .NET 一致，直觉；③ publish 已这么做（`restructure-publish-output-dirs`），只是提前到 build；④ 满足铁律 ①（lib 干净、exe 是正常自包含产物，无独立 scratch 目录）+ ②（复用 `Z42_LIBS` 只管 stdlib，不新增 env）。

### Decision 2: 「非标准库依赖」的判定

exe 复制哪些依赖？= **非 stdlib 依赖**。判定：依赖包名前缀 —— stdlib = `z42.*`（libraries workspace 产出，经 `Z42_LIBS` 解析）；本地/workspace 依赖 = 其它（如 `z42c.*`）。
参考 publish 已有实现（`restructure-publish-output-dirs` 的 exe 依赖复制）—— 复用同一判定，勿另造。
**边界**：`z42.io`（driver 声明的依赖之一）是 stdlib → **不复制**。

### Decision 3: 编译器自建改 `z42c build --workspace`

**问题**：手写 per-member 循环 + 累积能否换成 `--workspace`？
**验证**：本次实测 —— `z42c build --workspace`（CWD=src/compiler，seed 建当前源）7 包全建成 + **gen2/gen3 逐字节 identical**（不动点成立）。"E0402 wrinkle" 注释过时（那是当年给 `--output-dir` flat 覆盖时的现象，正常 per-member 布局无此问题）。
**决定**：`_buildCompilerViaZ42c` → 一句 `z42c build --workspace`（CWD=src/compiler，Z42_LIBS=运行种子 driver 所需的 {stdlib+种子7包} —— 种子来自 SDK 的 `programs/z42c`（本就 colocated），无需拼 selfbuild-runlibs）。

### Decision 4: 目录名 z42c → compiler

`src/compiler/` 应镜像到 `artifacts/build/compiler/`（与 `src/libraries/`→`libraries/` 一致）。
改 `z42.workspace.toml` 的 `output_dir` 模板 + 全部 `artifacts/build/z42c` 引用（~40 处，机械）。

### Decision 5: env 收拢（本会话新债优先）

本会话我为种子解析加了 `Z42C_DIR` + 扩了 `Z42_TOOLCHAIN` 用法。按铁律 ② 收拢：
- **删 `Z42C_DIR`**：种子 z42c 目录一律从 SDK 根 `programs/z42c` 派生。
- `Z42_TOOLCHAIN` 保留为"显式 SDK 根 override"（CI 在用），但 `_seedSdkDir` 的多级回退不变（Z42_TOOLCHAIN → Z42_HOME → apphost SDK → ./.z42）。
- launcher 三 VM 变量合一 = 独立后续（Out of Scope）。

## Implementation Notes

- **顺序**（每步独立可验证 + 分提交）：
  1. Phase 1（compiler）：z42c build exe 复制非 stdlib 依赖 + 单测。
  2. Phase 2（toolchain）：xtask 自建改 --workspace + 编 stdlib 跑自包含 driver；删 selfbuild-runlibs + dogfood。
  3. Phase 3（toolchain+compiler）：z42c → compiler 全量改名（含 workspace toml）。
  4. Phase 4（toolchain）：env 收拢（删 Z42C_DIR）。
  5. Phase 5（docs）。
- **Phase 1 与 Phase 3 的耦合**：Phase 1 改 z42c.project 属 `compiler` 锁；Phase 2/4 属 `toolchain`；Phase 3 跨两者。本 change 同时占 `compiler` + `toolchain`。
- **CI**：ci-bootstrap 的种子 stage 路径 + 后续 build compiler/stdlib 均受 Phase 2/3 影响 —— 每步后本地能验 warm；cold/CI 路径以 CI 为准。

## Testing Strategy

- 单元/golden：`src/compiler/z42c.project/tests/` 新增 —— build 一个 exe（带 1 个本地 lib 依赖）→ 断言 dist 含依赖 zpkg；build 一个 lib → 断言 dist 只有自己。
- 自举不动点：`xtask test compiler` —— 7/7 zpkg + **gen 逐字节 identical**（Phase 2 后必须仍绿）。
- 全 gate：`xtask test`（vm/cross-zpkg/lib/compiler）—— Phase 2/3 后全绿。
- cold 供种：`rm -rf artifacts/build && xtask build compiler`（自动供种 + --workspace）本地验（我环境有 `.z42`）。
- CI：Phase 3（改名）+ Phase 4（env）push 后盯 ci-bootstrap。
