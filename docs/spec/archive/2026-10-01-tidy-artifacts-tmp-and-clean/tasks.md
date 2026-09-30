# Tasks: `.scratch` 并入 `tmp/`，alllibs 挪进 `build/views/`，`xtask clean` 补全

**状态：🟢 已完成 | 开始：2026-09-30 | 完成：2026-10-01（#969）**

类型：`refactor` + `fix`（`clean tests|bench` 恒删 0 个）→ 最小化模式。整理 `artifacts/` 布局第 3 步
（前两步：#958 CI 垫片、#962 路径集中定义）。

## 动机

- `.scratch/`（跨步骤复用）与 `tmp/`（自检一次性）两个桶生命周期没有实质差别：都可重生、都不进包、
  都没被 `clean` 覆盖。
- alllibs 在 `.scratch/`，但按 artifacts-layout.md 自己的判据（「会被别的步骤当产物消费 → `build/`」）
  它是工具链编译 / 测试 / publish hook 的关键输入。
- `xtask clean tests|bench` 删的是 `<lib>/<profile>/{tests,bench}` —— 当前布局里不存在，恒删 0 个、恒报 ✅。
- `xtask clean all` 只删 `build/`，`.scratch` / `tmp` / `publish` 与源码树里的残留都不管。

## 进度概览

- [x] 阶段 1: 布局
  - [x] `_scratchDir` 删除，11 个使用点改 `_tmpDir`（`artifacts/tmp/<name>`）
  - [x] alllibs → `_allLibsDir` = `build/views/<profile>/all`；`scripts/hooks/hooks.z42` 的副本同步
  - [x] `xtask layout`：key `scratch` 删、`alllibs` 加（`scratch` 只在 #962 与本 change 之间存在过，无消费方）
- [x] 阶段 2: `xtask clean`
  - [x] `tests`：golden 镜像（`build/tests`、`build/{libraries,compiler}/<m>/tests`）+ `<工程>/artifacts/test-targets`
  - [x] `bench`：`<工程>/artifacts/bench-targets`
  - [x] 新 `tmp`：`tmp/` + 旧 `.scratch/`
  - [x] `all`：+ `tmp/` `.scratch/` `publish/` + 源码树里带清单（`*.z42.toml` 或 `z42.toml`）的工程目录旁的
        `artifacts/` `dist/`，以及 `src/tests/cross-zpkg/*/*/libs/`；保留 xtask / tools / 成品 / 报告
        （初版只认 `*.z42.toml`，实测漏掉了用 `z42.toml` 的测试夹具 —— 一次 GREEN 后 456 + 217 个目录只删了 29 个）
  - [x] 生产 clean 只枚举 `debug` / `release` 两个 profile（不再把 golden 的 `tests/` 当 profile）
- [x] 阶段 3: 文档（artifacts-layout.md §1/§3/§4、build.md、self-hosting.md、xtask.md、scripts/README.md）
- [x] 阶段 4: 验证
  - [x] `clean tmp` / `clean tests` / `clean bench` / 未知 target 用假数据实测
  - [x] `clean all` 实测：源码树 456 个 artifacts/dist + 217 个 cross-zpkg libs → 0；无入库文件被删；~1s
  - [x] 完整 GREEN（`xtask test`，8m47s，全阶段通过；`build all` 后、`Z42_HOME=<本树 build sdk 产物>`）
- [x] 阶段 5: PR CI + 归档：PR CI 10 pass / 15 skipping（路径过滤）
## 验证过程中的观察（不在本 change 修）

- `compiler` stage 的 analyzer-cache 检查（#961 新增）依赖 `Z42_HOME`：分析器工程按名依赖 `z42c.syntax` /
  `z42c.core`，z42c 的编译器域探测在开发树第 ④ 档只看 `z42c.semantics/release/dist/`（那里只有 semantics 自己），
  要靠第 ② 档 `Z42_HOME/programs/z42c/` 才找得到。经 `./xtask`（launcher）启动会自动设 `Z42_HOME`；直接
  `z42vm xtask.zpkg -- test` 则红。且 `Z42_HOME` 必须指向**同代** SDK —— 指向旧种子会让 golden regen 用旧编译器
  组件、3 个新语法用例编不过。CI 为何不设 `Z42_HOME` 也能过未查明。本 change 的 GREEN 以
  `Z42_HOME=<本树 build sdk 产物>` 运行。

## 迁移提示

旧工作树里的 `artifacts/.scratch/` 不再被读写，`xtask clean tmp` 或直接删掉即可。
