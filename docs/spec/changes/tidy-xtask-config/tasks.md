# Tasks: versions.toml 挪进 scripts/；xtask 依赖改引工具链 zpkg；精简 xtask.z42.toml 与根 .gitignore

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`refactor`（仓库布局 + 配置清理；构建产物不变）→ 最小化模式。

## 改动

1. **`versions.toml` → `scripts/versions.toml`**。读它的几乎全是 xtask；另有 release.yml 的 tag 校验、
   install-z42.{sh,bat}、z42b 的 iOS xcframework 构建（`builder_device_ios.z42`）。
   - xtask 全部读取方改走新增的 `_versionsPath(root)`（此前 5 处各自 `Path.Join(root, "versions.toml")`）。
   - z42b 那处原本「文件不在就**静默**回落 16.0」——挪位置时它不会报任何错、只会悄悄用错 `min_ios` ⇒ 改路径的同时
     改为回落前打 warning。
   - 头注里的过时信息一并修正：`scripts/_lib/versions.sh` / `setup-tools.sh`（早已不存在，现为 `xtask deps …`）、
     「仓库根 Cargo.toml」（实为 `src/runtime/Cargo.toml`）。路径类值（`install_root` 等）仍相对仓库根，消费方都是
     `Path.Join(root, …)`，语义不变。
2. **`scripts/xtask.z42.toml`**：
   - 删 `cache_dir = "${output_dir}/.cache"`（= 默认值）。
   - 删 `[profile.release.runtime] probing-paths = "../build/compiler/*/release/dist"`：两个 path 依赖按部署规则被复制进
     `artifacts/xtask/`，而 VM 的搜索顺序是「入口 zpkg 目录 → probing-paths → libs」——旁边的副本排在前面，这条路径
     从未被用到。实测：移走 `artifacts/build/compiler` 后 `xtask layout compiler-build`（经 `WorkspaceLayout`）照常运行。
   - 注释里的旧名 `CentralizedBuildLayout` → `BuildLayout`。
   - **`z42.project` / `z42.build` 改为引用工具链里的 zpkg**（用户裁定：「直接引用 SDK 的 zpkg，构建时复制过来」）：
     `{ path = "${compiler_libs}/z42.project.zpkg" }`，不再 path 依赖源码代建。`${compiler_libs}` 由 z42c 按
     `Z42_COMPILER_LIBS → Z42_HOME/programs/z42c → Z42_PORTABLE_VM 反推的 <sdk>/programs/z42c → 开发树` 解析；
     构建时按部署规则复制到 xtask.zpkg 旁边。配套：
     - z42c `CompilerDomain` 第 ④ 档（开发树）由 `z42c.semantics` 的 dist 改为 `z42c.driver` 的 dist —— 后者是自包含
       闭包、与 SDK `programs/z42c/` 同形；前者只有它自己（analyzer 契约包碰巧够用，`z42.project` 引用不到）。
     - ci-bootstrap [2/5] 设 `Z42_COMPILER_LIBS=<boot driver 所在目录>`（快路径 .ci-seed / 两代路径 gen2 driver dist）；
       test-consume 设 `Z42_COMPILER_LIBS=$TC/programs/z42c`。
     - `test incremental` 的 xtask 拷贝：宏路径原样照抄。
     - 自举纪律（`bootstrap-seed.md`「stdlib API 面」）：xtask 用这两个包的新 API 要晚一个 nightly；改布局规则那次合入里
       CI 上的 xtask 仍按上一 nightly 的规则算路径。
3. **根 `.gitignore`**：删 C#/.NET 时代条目（`*.suo` / `obj/` / `bin/` / `*.nupkg` / `BenchmarkDotNet.Artifacts/` /
   `TestResults/` 等；C# 2026-06-26 已全删）、已无产出的 `*.z42c` / `*.zlib` / `*.zmod`、两处重复条目；`/xtask` 的注释改为
   现行生成方式。99 → 67 行。保留项逐条核对过仍有产出方（`.ci-seed` / `.ci-gen1run` / `.xtask-*.z42.toml` / cross-zpkg `libs/`）。

根目录其余项不动：`.cargo/` 必须在根（cargo 从**当前目录**向上找 config，CI 都在根上 `--manifest-path` 调用）；
`.gitattributes` / `.gitignore` / `LICENSE` / `README.md` 是惯例位置；`docs` / `examples` / `scripts` / `src` 是一级结构。

## 进度概览

- [x] versions.toml 搬家 + 全部读取方
- [x] xtask.z42.toml 精简
- [x] .gitignore 清理
- [x] xtask 依赖改为工具链 zpkg：本地三种构建方式实测 —— 显式 `Z42_COMPILER_LIBS`；不设任何变量靠开发树第 ④ 档
      （新 z42c）；**真实 SDK** 的 `bin/z42c build` 不设任何环境变量（自推 `programs/z42c`），产物带两份副本且可运行。
      副本与工具链里的 zpkg 逐字节相同
- [x] 文档：位置性引用（各页头「对齐代码」、`$EDITOR` 命令、ci.md filter 表、scripts/README）
- [x] 本地 GREEN（基底 d54f7ba46，9m09s，src 下零产物）+ `xtask deps check` + install-z42.sh / release.yml 的 awk 读取实测；`test incremental` 的 xtask 拷贝清单手工模拟编过
- [x] 顺带修正过时文档：`project-model.md`「解析域」一节仍按已删的 `compiler-libs/` 目录与 `_compilerLibsDirs()`
      三档探测描述（代码早已是 `CompilerDomain.Dirs()` 四档、落 `programs/z42c/`）——重写，并补上 `${compiler_libs}`
      显式引用这条用法；`xtask_compiler_e2e_analyzer.z42` 一处注释同。（目录已删、**宏仍是现行机制**——用户曾问「宏不是删了吗」，
      混淆点正在这里）
- [ ] PR CI
