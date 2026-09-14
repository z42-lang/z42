# Tasks: cache 目录布局 —— 不论是否增量都落 cache，默认 `${output_dir}/.cache`

> 状态：🟢 已完成 | 完成：2026-09-15
> 变更类型：`fix`（最小化模式）

**变更说明：** User 要求 cache 四条：①workspace 也支持增量 ②**不管是否增量都输出 cache** ③默认 cache =
`output_dir/.cache` ④补全 toolchain 等工程的 `output_dir`；有配置用配置、没配置用默认值（已有的
`cache_dir = "${output_dir}/cache"` 保留）。本 change 落 ②③④ 和 ① 的「写」半边；workspace **读** cache
须先让缓存键含编译器身份（否则自举 gen2 会命中 gen1 由种子编译器写的 cache），留给后续 change。

**原因：** 此前 cache 只在「单工程 + 无 `--output-dir`」时写：
1. `--no-incremental` 全量路径不捕获标识符（写 meta 需要），workspace / flat 成员（`outputDirOverride` 非空）整段跳过；
2. 多 `[[exe]]` 分支在写 cache 之前提前 `return`，从不落盘；
3. `_resolveCacheDir` 对「无任何 [build]」特判回落 `<projectDir>/.cache`（写进源码树），与有配置时的
   `${output_dir}/.cache` 两套口径，也与 `project.md` 表格不一致；
4. z42b `builder.z42::_computeDirs` 把 `[build]` 相对路径按 cwd 解析、`${output_dir}`/`${profile}` 当字面量。

**修法：**
- cache 写盘抽到 `z42c.driver/src/BuildCache.z42::_writeBuildCache`（try/catch 非致命），单产物与多 exe 都调；
  全量 parse 恒捕获标识符。增量开关只管下次「读不读」（probe 门未动），不管「写不写」。
- `_resolveCacheDir` 去掉特判；`_build` 加 `cacheDirOverride` 参数。
- workspace：`WsPlan.CacheDirs` + `WorkspaceBuild.ResolveMemberCacheDir`（`[workspace.build].cache_dir` 模板，
  没配置 `${output_dir}/.cache`；两模板都不含 `${member_name}`/`${project_name}` 时追加成员子目录防碰撞）；
  flat 模式 `<out>/.cache/<成员名>`。
- toolchain 9 个 toml 补 `output_dir`（`artifacts/build/toolchain/<n>`、workload 与 testagent 在 `…/workload/<n>`）。
- 写盘热点顺手去平方：`CacheStore.HexEnc` 改 `char[]` + `FromChars`，`Serialize` / label 行改 `ConcatParts`
  （meta 输出逐字节不变，418 个 meta 对拍一致）。

**代价（实测，已告知 User）：** stdlib 全 workspace `--no-incremental`，两个 driver 二进制背靠背各 3 次：
指令 151.0G → 170.8G（**+13.1%**，去平方前 +14.1%），墙钟 12.7s → 14.5s，RSS +46 MB，cache 共 5.7 MB。
profile 差量：逐文件 fullMode `.zbc` 序列化约一半、parse 期标识符捕获 + SurfaceHash 约三成——是「全量也写 cache」
这个要求本身的成本；workspace 读 cache 落地后 warm 重建收回。

**文档影响：** `docs/design/compiler/project.md`（默认值示例 + workspace wiring 前置依赖）、driver README、
`docs/book/src/dev/build.md`、`.gitignore` 注释。

- [x] 1.1 `z42c.driver/src/BuildCache.z42`（新）+ `Main.z42`：统一写 cache、多 exe 也写、全量恒捕获
- [x] 1.2 `z42c.driver/src/BuildPaths.z42::_resolveCacheDir` 去特判
- [x] 1.3 `z42c.pipeline/src/WorkspaceBuild.z42` + `z42c.driver/src/WorkspaceBuild.z42`：成员 cache 目录
- [x] 1.4 toolchain 9 个 toml 补 `output_dir`；`scripts/test/xtask_test_embedded_golden.z42` 跟随 testagent dist 位置
- [x] 1.5 z42b `builder.z42::_computeDirs`：相对 toml 目录解析 + 展开模板；默认 cache `.cache`
- [x] 1.6 `CacheStore.HexEnc` / `Serialize` / label 行去平方拼接
- [x] 1.7 e2e `scripts/build/xtask_compiler_e2e_cache.z42::_e2eCacheLayoutChecks`：单工程 `--no-incremental` /
      多 `[[exe]]` / workspace 成员均落 cache 且在默认位置。**阴性对照**：换旧 driver 跑同批夹具，三处全缺
      （单工程落在旧的 `<proj>/.cache`，另两处不写）
- [x] 1.8 手测：`build stdlib` 后 25 个 `artifacts/build/libraries/<pkg>/release/cache` 与编译器成员 cache 均生成
- [x] 1.9 CI 首跑暴露：builder / devtools / interactive 的 `dist_dir == output_dir`，默认 cache 落进产物目录，
      `build stage-toolchain` 把 `.cache` 子目录当文件拷 → 崩。修：暂存只拷文件；并按 User 要求把 xtask 里
      z42 工程产物路径**全部改为从 toml 解析**（`_libsBuildRoot`/`_compilerBuildRoot` 取 workspace.toml
      output_dir 模板前缀、`_xtaskZpkg`/testagent 走 `_toolchainZpkg`、`clean` 按 cache_dir/output_dir 模板、
      stage-toolchain 目标位置 = 源路径相对仓库根；cross/incremental/package-desktop/install-vscode 改用既有 helper）。
      本地验证 stage-toolchain 布局与原来一致且无 `.cache`
- [x] 1.10 GREEN：`xtask test` 全 stage 绿
